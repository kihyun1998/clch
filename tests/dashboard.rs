mod common;

use common::{Dashboard, Sandbox, blocks, reclaimable};

fn with_npm_cache(sb: &Sandbox) {
    sb.write_file(&sb.default_npm_cache().join("_cacache").join("entry"), 3);
}

fn dashboard(sb: &Sandbox) -> Dashboard {
    Dashboard::start(sb, &["--no-open"])
}

#[test]
fn it_prints_a_url_on_loopback_carrying_a_fresh_token_each_run() {
    let sb = Sandbox::new();
    let first = dashboard(&sb);
    let second = dashboard(&sb);

    assert!(first.token.len() >= 32, "{}", first.token);
    assert_ne!(first.token, second.token);
    assert_ne!(first.port, second.port);
}

#[test]
fn no_open_opens_no_browser() {
    let sb = Sandbox::new();
    opener(&sb);

    let running = dashboard(&sb);

    assert_eq!(running.send("GET", "/assets/none.js", &[], "").status, 404);
    std::thread::sleep(std::time::Duration::from_millis(400));
    assert!(sb.calls().is_empty(), "{:?}", sb.calls());
}

fn opener(sb: &Sandbox) {
    let name = if cfg!(windows) { "rundll32" } else { "open" };
    sb.fake_tool(name, "", "");
}

#[test]
fn without_no_open_the_browser_gets_the_url() {
    let sb = Sandbox::new();
    opener(&sb);

    let running = Dashboard::start(&sb, &[]);

    let mut calls = Vec::new();
    for _ in 0..100 {
        calls = sb.calls();
        if !calls.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains(&running.token), "{calls:?}");
}

#[test]
fn without_any_browser_opener_it_still_runs() {
    let sb = Sandbox::new();
    with_npm_cache(&sb);

    let running = Dashboard::start(&sb, &[]);

    assert_eq!(running.api("POST", "/api/scan", "").status, 200);
}

#[test]
fn the_scan_endpoint_returns_reclaimables_with_per_disk_totals() {
    let sb = Sandbox::new();
    with_npm_cache(&sb);
    let running = dashboard(&sb);

    let response = running.api("POST", "/api/scan", "");

    assert_eq!(response.status, 200);
    assert!(response.headers.contains("application/json"));
    let scan = response.json();
    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm cache")["size_bytes"],
        blocks(3)
    );
    let disk = scan["disks"][0].clone();
    assert_eq!(disk["reclaimable_bytes"], blocks(3));
    assert!(disk["free_bytes"].as_u64().is_some());
}

#[test]
fn a_scan_through_the_dashboard_records_a_snapshot_as_the_cli_does() {
    let sb = Sandbox::new();
    with_npm_cache(&sb);
    let running = dashboard(&sb);

    let first = running.api("POST", "/api/scan", "").json();
    assert_eq!(
        reclaimable(&first, "npm-cache").unwrap()["change_state"],
        "new"
    );

    let cli = sb.json(&["scan"]);
    assert_eq!(
        reclaimable(&cli, "npm-cache").unwrap()["change_state"],
        "known"
    );

    let again = running.api("POST", "/api/scan", "").json();
    assert_eq!(reclaimable(&again, "npm-cache").unwrap()["change_bytes"], 0);
}

#[test]
fn configured_scan_roots_are_searched() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("app"), 2);
    sb.clch()
        .args(["roots", "add", root.to_str().unwrap()])
        .assert()
        .success();
    let running = dashboard(&sb);

    let scan = running.api("POST", "/api/scan", "").json();

    assert_eq!(
        reclaimable(&scan, "node_modules").expect("found")["path"],
        modules.display().to_string()
    );
}

#[test]
fn a_request_without_the_token_is_rejected_and_records_nothing() {
    let sb = Sandbox::new();
    with_npm_cache(&sb);
    let running = dashboard(&sb);

    assert_eq!(running.send("POST", "/api/scan", &[], "").status, 401);
    let wrong = "0".repeat(running.token.len());
    assert_eq!(
        running
            .send("POST", "/api/scan", &[("X-Clch-Token", wrong.as_str())], "")
            .status,
        401
    );

    let first = sb.json(&["scan"]);
    assert_eq!(
        reclaimable(&first, "npm-cache").unwrap()["change_state"],
        "new"
    );
}

#[test]
fn the_token_in_the_query_string_does_not_stand_in_for_the_header() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);

    let path = format!("/api/scan?token={}", running.token);

    assert_eq!(running.send("POST", &path, &[], "").status, 401);
}

#[test]
fn a_foreign_origin_is_rejected_even_with_the_token() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);
    let token = running.token.clone();

    for origin in [
        "https://evil.example".to_string(),
        "null".to_string(),
        format!("http://localhost:{}", running.port),
        format!("http://127.0.0.1:{}", running.port + 1),
    ] {
        let response = running.send(
            "POST",
            "/api/scan",
            &[
                ("X-Clch-Token", token.as_str()),
                ("Origin", origin.as_str()),
            ],
            "",
        );
        assert_eq!(response.status, 403, "{origin}");
    }
}

#[test]
fn the_servers_own_origin_is_accepted() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);
    let token = running.token.clone();
    let origin = format!("http://{}", running.host());

    let response = running.send(
        "POST",
        "/api/scan",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Origin", origin.as_str()),
        ],
        "",
    );

    assert_eq!(response.status, 200);
}

#[test]
fn a_foreign_host_is_rejected_on_every_path() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);
    let token = running.token.clone();

    for host in ["evil.example", "localhost", "127.0.0.1"] {
        let api = running.send(
            "POST",
            "/api/scan",
            &[("X-Clch-Token", token.as_str()), ("Host", host)],
            "",
        );
        assert_eq!(api.status, 403, "{host}");
        let page = running.send("GET", "/", &[("Host", host)], "");
        assert_eq!(page.status, 403, "{host}");
    }
}

#[test]
fn the_page_needs_the_token_in_its_url() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);

    assert_eq!(running.send("GET", "/", &[], "").status, 401);
    assert_eq!(running.send("GET", "/?token=wrong", &[], "").status, 401);
}

#[test]
fn a_second_host_or_origin_is_rejected() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);
    let token = running.token.clone();
    let host = running.host();
    let origin = format!("http://{host}");

    let two_hosts = running.send(
        "POST",
        "/api/scan",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Host", host.as_str()),
            ("Host", "evil.example"),
        ],
        "",
    );
    assert_eq!(two_hosts.status, 403);

    let two_origins = running.send(
        "POST",
        "/api/scan",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Origin", origin.as_str()),
            ("Origin", "https://evil.example"),
        ],
        "",
    );
    assert_eq!(two_origins.status, 403);
}

#[test]
fn only_post_scans() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);

    assert_eq!(running.api("GET", "/api/scan", "").status, 405);
    assert_eq!(running.api("GET", "/api/nothing", "").status, 404);
}

#[test]
fn the_embedded_ui_is_served_with_its_assets() {
    let sb = Sandbox::new();
    let running = dashboard(&sb);

    let page_path = format!("/?token={}", running.token);
    let page = running.send("GET", &page_path, &[], "");

    assert_eq!(page.status, 200);
    assert!(page.headers.contains("text/html"), "{}", page.headers);
    assert!(page.headers.contains("no-referrer"), "{}", page.headers);
    let html = page.text();
    assert!(html.contains("id=\"root\""), "{html}");
    let script = html
        .split("src=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or_else(|| panic!("a script in {html}"));
    let asset_path = format!(
        "/{}",
        script.trim_start_matches("./").trim_start_matches('/')
    );
    let asset = running.send("GET", &asset_path, &[], "");
    assert_eq!(asset.status, 200, "{asset_path}");
    assert!(asset.headers.contains("javascript"), "{}", asset.headers);
    assert!(!asset.body.is_empty());

    assert_eq!(running.send("GET", "/assets/none.js", &[], "").status, 404);
}

#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_server() {
    let sb = Sandbox::new();
    let mut running = dashboard(&sb);
    let port = running.port;

    // SAFETY: `kill` takes a process id and a signal, and touches no memory.
    let sent = unsafe { libc::kill(running.pid() as i32, libc::SIGINT) };
    assert_eq!(sent, 0);

    assert!(running.wait_for_exit().is_some(), "still running");
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
}
