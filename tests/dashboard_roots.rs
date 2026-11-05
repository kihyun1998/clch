mod common;

use common::{Dashboard, Response, Sandbox};
use std::path::PathBuf;

fn change(running: &Dashboard, action: &str, folder: &str) -> Response {
    let token = running.token.clone();
    let body = serde_json::json!({ "action": action, "folder": folder }).to_string();
    running.send(
        "POST",
        "/api/roots",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Content-Type", "application/json"),
        ],
        &body,
    )
}

fn listed(running: &Dashboard) -> Vec<PathBuf> {
    let reply = running.api("GET", "/api/roots", "").json();
    reply["scan_roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| PathBuf::from(r.as_str().unwrap()))
        .collect()
}

fn cli_roots(sb: &Sandbox) -> serde_json::Value {
    sb.json(&["roots", "list"])["scan_roots"].clone()
}

fn folder(sb: &Sandbox, name: &str) -> PathBuf {
    let path = sb.root().join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn it_reads_what_the_cli_set() {
    let sb = Sandbox::new();
    let code = folder(&sb, "code");
    sb.clch()
        .args(["roots", "add", code.to_str().unwrap()])
        .assert()
        .success();
    let running = Dashboard::start(&sb, &["--no-open"]);

    assert_eq!(listed(&running), [code]);
}

#[test]
fn adding_lands_in_the_file_the_cli_reads_and_the_next_scan_uses_it() {
    let sb = Sandbox::new();
    let code = folder(&sb, "code");
    let modules = sb.node_project(&code.join("app"), 2);
    let running = Dashboard::start(&sb, &["--no-open"]);

    let reply = change(&running, "add", code.to_str().unwrap());

    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(listed(&running), vec![code.clone()]);
    assert_eq!(cli_roots(&sb)[0], code.display().to_string());
    assert!(sb.config_file().is_file());
    let scan = running.api("POST", "/api/scan", "").json();
    assert!(scan.to_string().contains("node_modules"), "{scan}");
    assert!(modules.exists());
}

#[test]
fn removing_lands_there_too() {
    let sb = Sandbox::new();
    let code = folder(&sb, "code");
    let other = folder(&sb, "other");
    let running = Dashboard::start(&sb, &["--no-open"]);
    change(&running, "add", code.to_str().unwrap());
    change(&running, "add", other.to_str().unwrap());

    let reply = change(&running, "remove", code.to_str().unwrap());

    assert_eq!(reply.status, 200);
    assert_eq!(listed(&running), vec![other.clone()]);
    assert_eq!(cli_roots(&sb)[0], other.display().to_string());
    assert!(code.exists(), "only the setting goes");
}

#[test]
fn the_reply_is_the_list_after_the_change() {
    let sb = Sandbox::new();
    let code = folder(&sb, "code");
    let running = Dashboard::start(&sb, &["--no-open"]);

    let reply = change(&running, "add", code.to_str().unwrap()).json();

    assert_eq!(reply["scan_roots"][0], code.display().to_string());
}

#[test]
fn a_leading_tilde_means_the_home_folder() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.home().join("code")).unwrap();
    let running = Dashboard::start(&sb, &["--no-open"]);

    let reply = change(&running, "add", "~/code");

    assert_eq!(reply.status, 200, "{}", reply.text());
    assert_eq!(listed(&running), [sb.home().join("code")]);
}

#[test]
fn what_cannot_be_a_root_is_refused_with_the_reason() {
    let sb = Sandbox::new();
    let file = sb.root().join("a-file");
    std::fs::write(&file, "x").unwrap();
    let running = Dashboard::start(&sb, &["--no-open"]);

    for (action, target, says) in [
        ("add", file.to_str().unwrap(), "not a folder"),
        ("add", "relative/path", "full path"),
        ("add", "", "full path"),
        ("remove", sb.home().to_str().unwrap(), "not a scan root"),
    ] {
        let reply = change(&running, action, target);
        assert_eq!(reply.status, 400, "{target}");
        assert!(reply.text().contains(says), "{}", reply.text());
    }
    let unknown = change(&running, "rename", sb.home().to_str().unwrap());
    assert_eq!(unknown.status, 400);
    assert!(
        unknown.text().contains("add or remove"),
        "{}",
        unknown.text()
    );
    assert!(listed(&running).is_empty());
}

#[test]
fn it_needs_the_token_and_its_own_origin() {
    let sb = Sandbox::new();
    let code = folder(&sb, "code");
    let running = Dashboard::start(&sb, &["--no-open"]);
    let token = running.token.clone();
    let body = serde_json::json!({ "action": "add", "folder": code }).to_string();
    let json = ("Content-Type", "application/json");

    assert_eq!(running.send("GET", "/api/roots", &[], "").status, 401);
    assert_eq!(
        running.send("POST", "/api/roots", &[json], &body).status,
        401
    );
    let foreign = running.send(
        "POST",
        "/api/roots",
        &[
            ("X-Clch-Token", token.as_str()),
            json,
            ("Origin", "https://evil.example"),
        ],
        &body,
    );
    assert_eq!(foreign.status, 403);
    assert!(listed(&running).is_empty());
    assert_eq!(running.api("DELETE", "/api/roots", "").status, 405);
}
