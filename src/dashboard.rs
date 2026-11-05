//! The local dashboard: a web server on 127.0.0.1 over the same core as the CLI.

use std::io::{Cursor, Read};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use tiny_http::{Header, Method, Request, Response, Server};

use crate::clean::{self, Level, Plan, PlanSignature, Selection};
use crate::config;
use crate::env::Env;
use crate::scan::Scan;
use crate::{scan, snapshots};

include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));

/// The header every API request carries this run's token in.
const TOKEN_HEADER: &str = "x-clch-token";

/// The most a request body may hold.
const BODY_LIMIT: u64 = 1 << 20;

/// What makes a request one this run may answer, and what it remembers.
struct Guard {
    host: String,
    origin: String,
    token: String,
    /// The latest scan, which previews are built from.
    latest: Mutex<Option<Scan>>,
    /// The plan last previewed, which is all that can be carried out.
    previewed: Mutex<Option<Previewed>>,
    /// Held while a cleanup runs.
    cleaning: Mutex<()>,
}

/// What a person was shown and may confirm.
struct Previewed {
    id: String,
    selected: Vec<(String, PathBuf)>,
    level: Level,
    signature: PlanSignature,
}

#[derive(Deserialize)]
struct Row {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct PreviewRequest {
    selected: Vec<Row>,
    #[serde(default)]
    full: bool,
}

#[derive(Deserialize)]
struct CleanupRequest {
    plan_id: String,
    #[serde(default)]
    confirm: bool,
}

#[derive(Serialize)]
struct PreviewReply {
    plan_id: String,
    /// Whether anything here is a tool cache, the only thing the level changes.
    uses_level: bool,
    #[serde(flatten)]
    plan: Plan,
}

#[derive(Serialize)]
struct CleanupReply<'a> {
    outcome: &'a clean::Outcome,
    /// The scan taken after the cleanup.
    scan: &'a Scan,
}

/// Serves until the process is stopped, opening the browser unless `no_open`.
pub fn serve(env: &Env, no_open: bool) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let server =
        Server::from_listener(listener, None).map_err(|e| std::io::Error::other(e.to_string()))?;
    let token = new_token()?;
    let host = format!("127.0.0.1:{port}");
    let url = format!("http://{host}/?token={token}");
    println!("{url}");
    if !no_open {
        open_browser(env, &url);
    }
    let guard = Arc::new(Guard {
        origin: format!("http://{host}"),
        host,
        token,
        latest: Mutex::new(None),
        previewed: Mutex::new(None),
        cleaning: Mutex::new(()),
    });
    for mut request in server.incoming_requests() {
        let guard = Arc::clone(&guard);
        std::thread::spawn(move || {
            let reply = answer(&guard, &mut request);
            let _ = request.respond(reply);
        });
    }
    Ok(())
}

fn new_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Asks the system to open `url`. Where it cannot, the printed URL remains.
fn open_browser(env: &Env, url: &str) {
    let (opener, args): (&str, &[&str]) = if cfg!(windows) {
        ("rundll32", &["url.dll,FileProtocolHandler"])
    } else if cfg!(target_os = "macos") {
        ("open", &[])
    } else {
        ("xdg-open", &[])
    };
    let Some(program) = env.find_tool(opener) else {
        return;
    };
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Ok(mut child) = command.spawn() {
        std::thread::spawn(move || child.wait());
    }
}

type Reply = Response<Cursor<Vec<u8>>>;

fn answer(guard: &Guard, request: &mut Request) -> Reply {
    if count(request, "host") != 1 || header(request, "host") != Some(guard.host.as_str()) {
        return error(403, "this request is for another host");
    }
    if count(request, "origin") > 1
        || header(request, "origin").is_some_and(|origin| origin != guard.origin)
    {
        return error(403, "this request comes from another origin");
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    if path.starts_with("/api/") {
        let token = header(request, TOKEN_HEADER).unwrap_or_default();
        if !same_secret(token, &guard.token) {
            return error(401, "this request lacks this run's token");
        }
        return api(guard, request, path);
    }
    if *request.method() != Method::Get {
        return error(405, "only GET is served here");
    }
    if path == "/" {
        let token = query.strip_prefix("token=").unwrap_or_default();
        if !same_secret(token, &guard.token) {
            return error(
                401,
                "open the URL clch printed, which carries this run's token",
            );
        }
    }
    asset(path)
}

fn api(guard: &Guard, request: &mut Request, path: &str) -> Reply {
    let method = request.method().clone();
    match (path, &method) {
        ("/api/preview", Method::Post) => match body(request) {
            Ok(wanted) => preview(guard, wanted),
            Err(reply) => reply,
        },
        ("/api/preview", _) => error(405, "use POST"),
        ("/api/cleanup", Method::Post) => match body(request) {
            Ok(wanted) => cleanup(guard, wanted),
            Err(reply) => reply,
        },
        ("/api/cleanup", _) => error(405, "use POST"),
        ("/api/scan", Method::Post) => run_scan(guard),
        ("/api/scan", _) => error(405, "use POST"),
        ("/api/history", Method::Get) => read_history(),
        ("/api/history", _) => error(405, "use GET"),
        _ => error(404, "no such endpoint"),
    }
}

/// Reads a request's JSON body.
fn body<T: for<'de> Deserialize<'de>>(request: &mut Request) -> Result<T, Reply> {
    let is_json = header(request, "content-type")
        .is_some_and(|t| t.split(';').next() == Some("application/json"));
    if !is_json {
        return Err(error(415, "send application/json"));
    }
    let mut bytes = Vec::new();
    request
        .as_reader()
        .take(BODY_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(400, &e.to_string()))?;
    if bytes.len() as u64 > BODY_LIMIT {
        return Err(error(413, "that is too much to read"));
    }
    serde_json::from_slice(&bytes).map_err(|e| error(400, &e.to_string()))
}

fn preview(guard: &Guard, wanted: PreviewRequest) -> Reply {
    if wanted.selected.is_empty() {
        return error(400, "nothing is selected");
    }
    let selected: Vec<(String, PathBuf)> = wanted
        .selected
        .into_iter()
        .map(|row| (row.id, row.path))
        .collect();
    let level = if wanted.full {
        Level::Full
    } else {
        Level::Unused
    };
    let env = Env::from_process();
    let latest = guard.latest.lock().expect("no thread panics holding this");
    let Some(scan) = latest.as_ref() else {
        return error(409, "scan first");
    };
    let in_scan = |(id, path): &(String, PathBuf)| {
        scan.reclaimables
            .iter()
            .any(|r| r.id == *id && r.path == *path)
    };
    if !selected.iter().all(in_scan) {
        return error(409, "that is not in the latest scan; scan again");
    }
    let plan = match plan_for(&env, scan, &selected, level) {
        Ok(plan) => plan,
        Err(reply) => return reply,
    };
    let id = match new_token() {
        Ok(id) => id,
        Err(e) => return error(500, &e.to_string()),
    };
    *guard
        .previewed
        .lock()
        .expect("no thread panics holding this") = Some(Previewed {
        id: id.clone(),
        selected,
        level,
        signature: plan.signature(),
    });
    json(
        200,
        &PreviewReply {
            plan_id: id,
            uses_level: plan.uses_level(),
            plan,
        },
    )
}

fn plan_for(
    env: &Env,
    scan: &Scan,
    selected: &[(String, PathBuf)],
    level: Level,
) -> Result<Plan, Reply> {
    let selection = Selection {
        exact: selected.to_vec(),
        ..Selection::default()
    };
    clean::plan(env, scan, &selection, level).map_err(|_| error(400, "that cannot be selected"))
}

/// Carries out the plan last previewed, once, if a fresh scan still gives that
/// same plan.
fn cleanup(guard: &Guard, wanted: CleanupRequest) -> Reply {
    if !wanted.confirm {
        return error(400, "a cleanup needs an explicit confirm");
    }
    let Ok(_running) = guard.cleaning.try_lock() else {
        return error(409, "a cleanup is already running");
    };
    let previewed = {
        let mut held = guard
            .previewed
            .lock()
            .expect("no thread panics holding this");
        if held
            .as_ref()
            .is_none_or(|p| !same_secret(&p.id, &wanted.plan_id))
        {
            return error(409, "that is not the plan last previewed; preview again");
        }
        held.take().expect("just checked")
    };
    let env = Env::from_process();
    let scope = match config::configured_scope(&env) {
        Ok(scope) => scope,
        Err(e) => return error(500, &e.to_string()),
    };
    let fresh = scan::run(&env, &scope);
    let plan = match plan_for(&env, &fresh, &previewed.selected, previewed.level) {
        Ok(plan) => plan,
        Err(reply) => return reply,
    };
    if plan.signature() != previewed.signature {
        *guard.latest.lock().expect("no thread panics holding this") = Some(fresh);
        return error(
            409,
            "what was previewed has changed; nothing was removed. Preview again",
        );
    }
    let (outcome, mut after) = clean::execute(&env, plan, &fresh, &scope);
    if let Err(e) = snapshots::track_after_cleanup(&env, &mut after, &outcome.attempted) {
        eprintln!("warning: snapshot not recorded: {e}");
    }
    let reply = json(
        200,
        &CleanupReply {
            outcome: &outcome,
            scan: &after,
        },
    );
    *guard.latest.lock().expect("no thread panics holding this") = Some(after);
    reply
}

fn run_scan(guard: &Guard) -> Reply {
    let env = Env::from_process();
    let scope = match config::configured_scope(&env) {
        Ok(scope) => scope,
        Err(e) => return error(500, &e.to_string()),
    };
    let mut result = scan::run(&env, &scope);
    if let Err(e) = snapshots::track(&env, &mut result) {
        eprintln!("warning: snapshot history not available: {e}");
    }
    let reply = json(200, &result);
    *guard.latest.lock().expect("no thread panics holding this") = Some(result);
    reply
}

fn read_history() -> Reply {
    match snapshots::history(&Env::from_process()) {
        Ok(history) => json(200, &history),
        Err(e) => error(500, &e.to_string()),
    }
}

fn asset(path: &str) -> Reply {
    let wanted = if path == "/" {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    match ASSETS.iter().find(|(name, _)| *name == wanted) {
        Some((name, bytes)) => respond(200, content_type(name), bytes.to_vec()),
        None => error(404, "no such file"),
    }
}

fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn count(request: &Request, name: &str) -> usize {
    request
        .headers()
        .iter()
        .filter(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .count()
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

/// Whether two tokens are equal, taking the same time wherever they differ.
fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

fn json(status: u16, value: &impl serde::Serialize) -> Reply {
    let body = serde_json::to_vec(value).expect("result serializes");
    respond(status, "application/json", body)
}

fn error(status: u16, message: &str) -> Reply {
    json(status, &serde_json::json!({ "error": message }))
}

fn respond(status: u16, content_type: &str, body: Vec<u8>) -> Reply {
    let mut reply = Response::from_data(body).with_status_code(status);
    for (name, value) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Content-Type-Options", "nosniff"),
        (
            "Content-Security-Policy",
            "default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    ] {
        let header = Header::from_bytes(name, value).expect("these headers are valid");
        reply = reply.with_header(header);
    }
    reply
}
