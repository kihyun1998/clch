//! The local dashboard: a web server on 127.0.0.1 over the same core as the CLI.

use std::io::Cursor;
use std::net::TcpListener;
use std::sync::Arc;

use tiny_http::{Header, Method, Request, Response, Server};

use crate::config;
use crate::env::Env;
use crate::{scan, snapshots};

include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));

/// The header every API request carries this run's token in.
const TOKEN_HEADER: &str = "x-clch-token";

/// What makes a request one this run may answer.
struct Guard {
    host: String,
    origin: String,
    token: String,
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
    });
    for request in server.incoming_requests() {
        let guard = Arc::clone(&guard);
        std::thread::spawn(move || {
            let reply = answer(&guard, &request);
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

fn answer(guard: &Guard, request: &Request) -> Reply {
    if count(request, "host") != 1 || header(request, "host") != Some(guard.host.as_str()) {
        return error(403, "this request is for another host");
    }
    if count(request, "origin") > 1
        || header(request, "origin").is_some_and(|origin| origin != guard.origin)
    {
        return error(403, "this request comes from another origin");
    }
    let (path, query) = request.url().split_once('?').unwrap_or((request.url(), ""));
    if path.starts_with("/api/") {
        let token = header(request, TOKEN_HEADER).unwrap_or_default();
        if !same_secret(token, &guard.token) {
            return error(401, "this request lacks this run's token");
        }
        return api(request, path);
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

fn api(request: &Request, path: &str) -> Reply {
    match (path, request.method()) {
        ("/api/scan", Method::Post) => run_scan(),
        ("/api/scan", _) => error(405, "use POST"),
        _ => error(404, "no such endpoint"),
    }
}

fn run_scan() -> Reply {
    let env = Env::from_process();
    let scope = match config::configured_scope(&env) {
        Ok(scope) => scope,
        Err(e) => return error(500, &e.to_string()),
    };
    let mut result = scan::run(&env, &scope);
    if let Err(e) = snapshots::track(&env, &mut result) {
        eprintln!("warning: snapshot history not available: {e}");
    }
    json(200, &result)
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
