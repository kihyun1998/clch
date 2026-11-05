mod common;

use common::{Dashboard, Sandbox, blocks};

fn npm_cache(sb: &Sandbox, count: usize) {
    let cache = sb.default_npm_cache().join("_cacache");
    let _ = std::fs::remove_dir_all(&cache);
    sb.write_file(&cache.join("entry"), count);
}

fn history(sb: &Sandbox) -> serde_json::Value {
    let running = Dashboard::start(sb, &["--no-open"]);
    let response = running.api("GET", "/api/history", "");
    assert_eq!(response.status, 200, "{}", response.text());
    response.json()
}

fn series<'a>(history: &'a serde_json::Value, id: &str) -> Vec<&'a serde_json::Value> {
    history["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == id)
        .collect()
}

fn sizes(series: &serde_json::Value) -> Vec<i64> {
    series["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["size_bytes"].as_i64().unwrap())
        .collect()
}

#[test]
fn with_no_scans_the_history_is_empty() {
    let sb = Sandbox::new();

    let history = history(&sb);

    assert_eq!(history["snapshots"].as_array().unwrap().len(), 0);
    assert_eq!(history["reclaimables"].as_array().unwrap().len(), 0);
    assert_eq!(history["disks"].as_array().unwrap().len(), 0);
}

#[test]
fn each_reclaimable_has_its_sizes_over_time() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    npm_cache(&sb, 3);
    sb.json(&["scan"]);
    npm_cache(&sb, 2);
    sb.json(&["scan"]);

    let history = history(&sb);

    assert_eq!(history["snapshots"].as_array().unwrap().len(), 3);
    let npm = series(&history, "npm-cache");
    assert_eq!(npm.len(), 1);
    assert_eq!(sizes(npm[0]), [blocks(1), blocks(3), blocks(2)]);
    assert_eq!(npm[0]["kind"], "tool-cache");
    assert!(npm[0]["points"][0]["taken_at_ms"].as_i64().unwrap() > 0);
    assert_eq!(npm[0]["points"][0]["complete"], true);
}

#[test]
fn a_tool_cache_that_moved_stays_one_series_at_its_latest_path() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    let moved = sb.root().join("moved");
    sb.write_file(&moved.join("_cacache").join("entry"), 2);
    sb.fake_npm_reporting_cache(&moved);
    sb.json(&["scan"]);

    let history = history(&sb);

    let npm = series(&history, "npm-cache");
    assert_eq!(npm.len(), 1, "{history}");
    assert_eq!(sizes(npm[0]), [blocks(1), blocks(2)]);
    assert_eq!(npm[0]["path"], moved.join("_cacache").display().to_string());
}

#[test]
fn project_artifacts_are_a_series_each() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    sb.node_project(&root.join("one"), 1);
    sb.node_project(&root.join("two"), 2);
    sb.clch()
        .args(["roots", "add", root.to_str().unwrap()])
        .assert()
        .success();
    sb.json(&["scan"]);

    let history = history(&sb);

    assert_eq!(series(&history, "node_modules").len(), 2);
}

#[test]
fn what_went_away_is_marked_in_its_series() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(sb.default_npm_cache().join("_cacache")).unwrap();
    sb.json(&["scan"]);

    let history = history(&sb);

    let points = series(&history, "npm-cache")[0]["points"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0]["gone"], false);
    assert_eq!(points[1]["gone"], true);
    assert_eq!(points[1]["size_bytes"], 0);
}

#[test]
fn each_disks_totals_are_kept_with_its_free_space() {
    let sb = Sandbox::new();
    npm_cache(&sb, 3);
    let scan = sb.json(&["scan"]);

    let history = history(&sb);

    let disk = &history["disks"][0];
    assert_eq!(disk["disk"], scan["disks"][0]["disk"]);
    let point = &disk["points"][0];
    assert_eq!(point["reclaimable_bytes"], blocks(3));
    assert!(point["free_bytes"].as_u64().is_some());
    assert_eq!(point["complete"], true);
}

#[test]
fn a_scan_of_one_off_roots_is_left_out_of_disk_history_only() {
    let sb = Sandbox::new();
    npm_cache(&sb, 3);
    let root = sb.root().join("code");
    sb.node_project(&root.join("one"), 1);
    sb.json(&["scan"]);
    sb.json(&["scan", "--root", root.to_str().unwrap()]);

    let history = history(&sb);

    assert_eq!(history["snapshots"].as_array().unwrap().len(), 2);
    assert_eq!(history["snapshots"][1]["partial"], true);
    assert_eq!(history["disks"][0]["points"].as_array().unwrap().len(), 1);
    assert_eq!(series(&history, "node_modules").len(), 1);
    assert_eq!(sizes(series(&history, "npm-cache")[0]).len(), 2);
}

#[test]
fn snapshots_are_ordered_by_id_when_the_clock_went_backwards() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    npm_cache(&sb, 2);
    sb.json(&["scan"]);
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    db.execute("UPDATE snapshot SET taken_at_ms = 5 WHERE id = 2", [])
        .unwrap();
    drop(db);

    let history = history(&sb);

    assert_eq!(
        sizes(series(&history, "npm-cache")[0]),
        [blocks(1), blocks(2)]
    );
    assert_eq!(history["snapshots"][1]["id"], 2);
}

#[test]
fn it_needs_the_token_and_changes_nothing() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    let running = Dashboard::start(&sb, &["--no-open"]);

    assert_eq!(running.send("GET", "/api/history", &[], "").status, 401);
    running.api("GET", "/api/history", "");
    let again = running.api("GET", "/api/history", "").json();

    assert_eq!(again["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(running.api("POST", "/api/history", "").status, 405);
}

#[test]
fn a_store_from_a_newer_clch_says_so() {
    let sb = Sandbox::new();
    npm_cache(&sb, 1);
    sb.json(&["scan"]);
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    db.pragma_update(None, "user_version", 9999).unwrap();
    drop(db);
    let running = Dashboard::start(&sb, &["--no-open"]);

    let response = running.api("GET", "/api/history", "");

    assert_eq!(response.status, 500);
    assert!(response.text().contains("newer"), "{}", response.text());
}
