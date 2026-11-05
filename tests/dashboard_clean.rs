mod common;

use common::{Dashboard, Response, Sandbox, blocks, says, tool};
use std::path::{Path, PathBuf};

/// A sandbox with scan root `code` holding projects `one` and `two`.
fn projects() -> (Sandbox, PathBuf, PathBuf) {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let one = sb.node_project(&root.join("one"), 2);
    let two = sb.node_project(&root.join("two"), 3);
    sb.clch()
        .args(["roots", "add", root.to_str().unwrap()])
        .assert()
        .success();
    (sb, one, two)
}

fn post(running: &Dashboard, path: &str, body: serde_json::Value) -> Response {
    let token = running.token.clone();
    running.send(
        "POST",
        path,
        &[
            ("X-Clch-Token", token.as_str()),
            ("Content-Type", "application/json"),
        ],
        &body.to_string(),
    )
}

fn artifact(path: &Path) -> serde_json::Value {
    serde_json::json!({ "id": "node_modules", "path": path })
}

fn preview(running: &Dashboard, selected: Vec<serde_json::Value>, full: bool) -> serde_json::Value {
    let response = post(
        running,
        "/api/preview",
        serde_json::json!({ "selected": selected, "full": full }),
    );
    assert_eq!(response.status, 200, "{}", response.text());
    response.json()
}

fn confirm(running: &Dashboard, plan_id: &serde_json::Value) -> Response {
    post(
        running,
        "/api/cleanup",
        serde_json::json!({ "plan_id": plan_id, "confirm": true }),
    )
}

fn scanned(sb: &Sandbox) -> Dashboard {
    let running = Dashboard::start(sb, &["--no-open"]);
    assert_eq!(running.api("POST", "/api/scan", "").status, 200);
    running
}

fn snapshots(running: &Dashboard) -> usize {
    running.api("GET", "/api/history", "").json()["snapshots"]
        .as_array()
        .unwrap()
        .len()
}

#[test]
fn a_preview_is_the_plan_clch_clean_prints_and_changes_nothing() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let before = snapshots(&running);

    let shown = preview(&running, vec![artifact(&one)], false);

    let cli = sb.json(&["clean", one.to_str().unwrap()]);
    assert_eq!(shown["plan"], cli["plan"]);
    assert_eq!(shown["level"], cli["level"]);
    assert_eq!(shown["expected_total_bytes"], blocks(2));
    assert!(shown["plan_id"].as_str().is_some_and(|id| !id.is_empty()));
    assert_eq!(shown["uses_level"], false);
    assert!(one.exists());
    assert_eq!(snapshots(&running), before);
}

#[test]
fn a_full_preview_uses_the_full_commands() {
    let sb = Sandbox::new();
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 2);
    sb.fake_npm_reporting_cache(&cache);
    let running = scanned(&sb);
    let row = serde_json::json!({ "id": "npm-cache", "path": cache.join("_cacache") });

    let unused = preview(&running, vec![row.clone()], false);
    let full = preview(&running, vec![row], true);

    assert_eq!(unused["plan"][0]["action"], "npm cache verify");
    assert_eq!(full["plan"][0]["action"], "npm cache clean --force");
    assert_eq!(full["level"], "full");
    assert_eq!(full["uses_level"], true);
    assert!(
        sb.calls()
            .iter()
            .all(|c| !c.contains("verify") && !c.contains("clean"))
    );
}

#[test]
fn a_preview_needs_a_scan_first() {
    let (sb, one, _) = projects();
    let running = Dashboard::start(&sb, &["--no-open"]);

    let response = post(
        &running,
        "/api/preview",
        serde_json::json!({ "selected": [artifact(&one)], "full": false }),
    );

    assert_eq!(response.status, 409);
}

#[test]
fn an_empty_selection_cleans_nothing_rather_than_everything() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/entry"), 2);
    let running = scanned(&sb);

    let response = post(
        &running,
        "/api/preview",
        serde_json::json!({ "selected": [], "full": true }),
    );

    assert_eq!(response.status, 400);
}

#[test]
fn something_not_in_the_scan_is_refused() {
    let (sb, _, _) = projects();
    let running = scanned(&sb);

    let response = post(
        &running,
        "/api/preview",
        serde_json::json!({
            "selected": [{ "id": "node_modules", "path": sb.home() }],
            "full": false,
        }),
    );

    assert_eq!(response.status, 409);
    assert!(sb.home().exists());
}

#[test]
fn only_the_row_selected_is_planned_where_two_share_a_path() {
    let sb = Sandbox::new();
    let report = sb.root().join("df.json");
    let verbose = sb.root().join("df-v.json");
    std::fs::write(
        &report,
        "{\"Reclaimable\":\"1GB\",\"Type\":\"Build Cache\"}\n",
    )
    .unwrap();
    std::fs::write(
        &verbose,
        "{\"Images\":[{\"Containers\":\"0\",\"UniqueSize\":\"2GB\"}]}\n",
    )
    .unwrap();
    let shows = |p: &Path| common::Reply {
        unix: format!("cat '{}'", p.display()),
        windows: format!("type \"{}\"", p.display()),
    };
    tool(
        &sb,
        "docker",
        vec![
            (
                "context inspect --format {{.Endpoints.docker.Host}}",
                says("unix:///var/run/docker.sock"),
            ),
            ("system df --format json", shows(&report)),
            ("system df --verbose --format json", shows(&verbose)),
        ],
    );
    let running = scanned(&sb);

    let shown = preview(
        &running,
        vec![serde_json::json!({ "id": "docker-images", "path": "docker engine" })],
        false,
    );

    let plan = shown["plan"].as_array().unwrap();
    assert_eq!(plan.len(), 1, "{shown}");
    assert_eq!(plan[0]["id"], "docker-images");
}

#[test]
fn a_cleanup_runs_only_on_an_explicit_confirm() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one)], false);

    for body in [
        serde_json::json!({ "plan_id": shown["plan_id"] }),
        serde_json::json!({ "plan_id": shown["plan_id"], "confirm": false }),
    ] {
        assert_eq!(post(&running, "/api/cleanup", body).status, 400);
        assert!(one.exists());
    }
}

#[test]
fn a_confirmed_cleanup_reports_what_it_freed_and_records_a_snapshot() {
    let (sb, one, two) = projects();
    let running = scanned(&sb);
    let before = snapshots(&running);
    let shown = preview(&running, vec![artifact(&one)], false);

    let response = confirm(&running, &shown["plan_id"]);

    assert_eq!(response.status, 200, "{}", response.text());
    let result = response.json();
    assert!(!one.exists());
    assert!(two.exists());
    let outcome = &result["outcome"];
    assert_eq!(outcome["results"][0]["status"], "cleaned");
    assert_eq!(outcome["results"][0]["freed_bytes"], blocks(2));
    assert_eq!(outcome["freed_bytes"], blocks(2));
    assert_eq!(outcome["disks"][0]["freed_bytes"], blocks(2));
    assert_eq!(snapshots(&running), before + 1);
    let rows = result["scan"]["reclaimables"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "the scan after it");
}

#[test]
fn a_plan_runs_once() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one)], false);

    assert_eq!(confirm(&running, &shown["plan_id"]).status, 200);
    assert_eq!(confirm(&running, &shown["plan_id"]).status, 409);
}

#[test]
fn a_plan_nobody_previewed_is_refused() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    preview(&running, vec![artifact(&one)], false);

    let response = confirm(&running, &serde_json::json!("not-a-plan"));

    assert_eq!(response.status, 409);
    assert!(one.exists());
}

#[test]
fn a_newer_preview_replaces_the_one_before() {
    let (sb, one, two) = projects();
    let running = scanned(&sb);
    let first = preview(&running, vec![artifact(&one)], false);
    let second = preview(&running, vec![artifact(&two)], false);

    assert_eq!(confirm(&running, &first["plan_id"]).status, 409);
    assert!(one.exists());
    assert_eq!(confirm(&running, &second["plan_id"]).status, 200);
    assert!(!two.exists());
    assert!(one.exists());
}

#[test]
fn what_appeared_after_the_preview_is_not_cleaned() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one)], false);
    let later = sb.node_project(&sb.root().join("code").join("three"), 1);

    assert_eq!(confirm(&running, &shown["plan_id"]).status, 200);

    assert!(!one.exists());
    assert!(later.exists());
}

#[test]
fn a_folder_gone_since_the_preview_refuses_the_whole_plan() {
    let (sb, one, two) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one), artifact(&two)], false);
    std::fs::remove_dir_all(&one).unwrap();

    let response = confirm(&running, &shown["plan_id"]);

    assert_eq!(response.status, 409, "{}", response.text());
    assert!(response.text().contains("changed"), "{}", response.text());
    assert!(two.exists());
}

#[test]
fn a_project_edited_since_the_preview_refuses_the_plan() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one)], false);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    sb.write_file(&one.parent().unwrap().join("index.js"), 1);

    let response = confirm(&running, &shown["plan_id"]);

    assert_eq!(response.status, 409, "{}", response.text());
    assert!(one.exists());
}

#[test]
fn a_tool_that_fails_is_reported_and_the_rest_still_run() {
    let (sb, one, _) = projects();
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 2);
    tool(
        &sb,
        "npm",
        vec![
            ("config get cache", common::prints(&cache)),
            (
                "cache verify",
                common::Reply {
                    unix: "echo exploded >&2; exit 1".into(),
                    windows: "echo exploded 1>&2\r\nexit /b 1".into(),
                },
            ),
        ],
    );
    let running = scanned(&sb);
    let shown = preview(
        &running,
        vec![
            serde_json::json!({ "id": "npm-cache", "path": cache.join("_cacache") }),
            artifact(&one),
        ],
        false,
    );

    let response = confirm(&running, &shown["plan_id"]);

    assert_eq!(response.status, 200);
    let results = response.json()["outcome"]["results"].clone();
    let by_id = |id: &str| {
        results
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by_id("npm-cache")["status"], "failed");
    assert!(
        by_id("npm-cache")["output"]
            .as_str()
            .unwrap()
            .contains("exploded")
    );
    assert_eq!(by_id("node_modules")["status"], "cleaned");
    assert!(!one.exists());
}

#[test]
fn a_git_tracked_folder_stays_skipped_as_in_the_cli() {
    let (sb, one, _) = projects();
    tool(
        &sb,
        "git",
        vec![(
            "-c core.fsmonitor=false ls-files -- node_modules",
            says("node_modules/dep/index.js"),
        )],
    );
    let running = scanned(&sb);

    let shown = preview(&running, vec![artifact(&one)], false);

    assert_eq!(shown["plan"][0]["cleanable"], false);
    let result = confirm(&running, &shown["plan_id"]).json();
    assert_eq!(result["outcome"]["results"][0]["status"], "skipped");
    assert!(one.exists());
}

#[test]
fn both_endpoints_refuse_requests_without_the_token_or_from_elsewhere() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let shown = preview(&running, vec![artifact(&one)], false);
    let token = running.token.clone();
    let cleanup = serde_json::json!({ "plan_id": shown["plan_id"], "confirm": true }).to_string();
    let plan = serde_json::json!({ "selected": [artifact(&one)], "full": false }).to_string();
    let json = ("Content-Type", "application/json");

    for (path, body) in [("/api/preview", &plan), ("/api/cleanup", &cleanup)] {
        assert_eq!(running.send("POST", path, &[json], body).status, 401);
        let foreign = running.send(
            "POST",
            path,
            &[
                ("X-Clch-Token", token.as_str()),
                json,
                ("Origin", "https://evil.example"),
            ],
            body,
        );
        assert_eq!(foreign.status, 403);
    }
    assert!(one.exists());
    assert_eq!(
        confirm(&running, &shown["plan_id"]).status,
        200,
        "still valid"
    );
}

#[test]
fn a_body_must_be_json_and_not_huge() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);
    let token = running.token.clone();
    let body = serde_json::json!({ "selected": [artifact(&one)], "full": false }).to_string();

    let text = running.send(
        "POST",
        "/api/preview",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Content-Type", "text/plain"),
        ],
        &body,
    );
    assert_eq!(text.status, 415);
    let broken = post(&running, "/api/preview", serde_json::json!(5));
    assert_eq!(broken.status, 400);
    let huge = running.send(
        "POST",
        "/api/preview",
        &[
            ("X-Clch-Token", token.as_str()),
            ("Content-Type", "application/json"),
        ],
        &" ".repeat(2_000_000),
    );
    assert_eq!(huge.status, 413);
    assert_eq!(running.api("GET", "/api/preview", "").status, 405);
    assert_eq!(running.api("GET", "/api/cleanup", "").status, 405);
}

#[test]
fn a_docker_cleanup_on_windows_carries_the_compaction_guidance() {
    let sb = Sandbox::new();
    let report = sb.root().join("df.json");
    let verbose = sb.root().join("df-v.json");
    std::fs::write(
        &report,
        "{\"Reclaimable\":\"1GB\",\"Type\":\"Build Cache\"}\n",
    )
    .unwrap();
    std::fs::write(&verbose, "{\"Images\":[]}\n").unwrap();
    let shows = |p: &Path| common::Reply {
        unix: format!("cat '{}'", p.display()),
        windows: format!("type \"{}\"", p.display()),
    };
    tool(
        &sb,
        "docker",
        vec![
            (
                "context inspect --format {{.Endpoints.docker.Host}}",
                says("unix:///var/run/docker.sock"),
            ),
            ("system df --format json", shows(&report)),
            ("system df --verbose --format json", shows(&verbose)),
        ],
    );
    let running = scanned(&sb);
    let shown = preview(
        &running,
        vec![serde_json::json!({ "id": "docker-build-cache", "path": "docker engine" })],
        false,
    );

    let result = confirm(&running, &shown["plan_id"]).json();

    let notes = result["outcome"]["notes"].as_array().unwrap();
    assert_eq!(notes.len(), usize::from(cfg!(windows)), "{result}");
}

fn fake_npm_with_cache(sb: &Sandbox) -> PathBuf {
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 2);
    sb.fake_npm_reporting_cache(&cache);
    cache.join("_cacache")
}

#[test]
fn a_plan_that_could_run_again_still_runs_once() {
    let sb = Sandbox::new();
    let cache = fake_npm_with_cache(&sb);
    let running = scanned(&sb);
    let row = serde_json::json!({ "id": "npm-cache", "path": cache });
    let shown = preview(&running, vec![row], false);

    assert_eq!(confirm(&running, &shown["plan_id"]).status, 200);
    assert_eq!(confirm(&running, &shown["plan_id"]).status, 409);

    let verified = sb.calls().iter().filter(|c| c.contains("verify")).count();
    assert_eq!(verified, 1);
}

#[test]
fn a_row_of_another_kind_at_that_path_is_not_in_the_scan() {
    let (sb, one, _) = projects();
    let running = scanned(&sb);

    let response = post(
        &running,
        "/api/preview",
        serde_json::json!({ "selected": [{ "id": "target", "path": one }], "full": false }),
    );

    assert_eq!(response.status, 409);
}

#[test]
fn a_command_that_changed_since_the_preview_refuses_the_plan() {
    let sb = Sandbox::new();
    let cache = sb.root().join("pip-custom");
    sb.write_file(&cache.join("entry"), 2);
    tool(&sb, "pip", vec![("cache dir", common::prints(&cache))]);
    let running = scanned(&sb);
    let row = serde_json::json!({ "id": "pip-cache", "path": cache });
    let shown = preview(&running, vec![row], true);
    assert_eq!(shown["plan"][0]["action"], "pip cache purge");

    let name = if cfg!(windows) { "pip.cmd" } else { "pip" };
    std::fs::remove_file(sb.bin().join(name)).unwrap();
    tool(&sb, "pip3", vec![("cache dir", common::prints(&cache))]);

    assert_eq!(confirm(&running, &shown["plan_id"]).status, 409);
    assert!(sb.calls().iter().all(|c| !c.contains("purge")));
}
