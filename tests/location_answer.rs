mod common;

use common::{Reply, Sandbox, prints, reclaimable, says, tool};
use std::path::PathBuf;

fn no_answer() -> Reply {
    Reply {
        unix: "exit 1".into(),
        windows: "exit /b 1".into(),
    }
}

fn plan_row<'a>(plan: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    plan["plan"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} in {plan}"))
}

fn assert_skipped_for_silence(sb: &Sandbox, id: &str, tool_name: &str) {
    let plan = sb.json(&["clean", id, "--full"]);
    let row = plan_row(&plan, id);
    assert_eq!(row["cleanable"], false, "{row}");
    let reason = row["reason"].as_str().unwrap();
    assert!(reason.contains("did not say"), "{reason}");
    assert!(reason.contains(tool_name), "{reason}");

    sb.json(&["clean", id, "--full", "--yes"]);
    let ran: Vec<String> = sb
        .calls()
        .into_iter()
        .filter(|c| c.contains("clean") || c.contains("purge") || c.contains("rm "))
        .collect();
    assert!(ran.is_empty(), "{ran:?}");
}

#[test]
fn npm_failing_the_location_question_is_not_asked_to_clean() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/entry"), 2);
    tool(&sb, "npm", vec![("config get cache", no_answer())]);

    let scan = sb.json(&["scan"]);
    assert!(reclaimable(&scan, "npm-cache").is_some(), "still measured");

    assert_skipped_for_silence(&sb, "npm-cache", "npm");
}

#[test]
fn npm_printing_more_than_one_line_counts_as_no_answer() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/entry"), 2);
    let two_lines = Reply {
        unix: "echo one; echo two".into(),
        windows: "echo one\r\necho two".into(),
    };
    tool(&sb, "npm", vec![("config get cache", two_lines)]);

    assert_skipped_for_silence(&sb, "npm-cache", "npm");
}

#[test]
fn the_npx_cache_shares_that_answer() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_npx/abc/entry"), 2);
    tool(
        &sb,
        "npm",
        vec![
            ("config get cache", no_answer()),
            ("--version", says("11.6.2")),
        ],
    );

    assert_skipped_for_silence(&sb, "npx-cache", "npm");
}

fn local_default(sb: &Sandbox, windows: &str, unix: &str) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join(windows)
    } else {
        sb.home().join(unix)
    }
}

#[test]
fn every_tool_with_a_location_question_is_treated_the_same() {
    let cases: [(&str, &str, &str, &str, &str); 4] = [
        (
            "pip-cache",
            "pip",
            "cache dir",
            "pip\\Cache",
            "Library/Caches/pip",
        ),
        ("uv-cache", "uv", "cache dir", "uv\\cache", ".cache/uv"),
        (
            "go-build-cache",
            "go",
            "env GOCACHE",
            "go-build",
            "Library/Caches/go-build",
        ),
        (
            "pnpm-store",
            "pnpm",
            "store path",
            "pnpm\\store",
            "Library/pnpm/store",
        ),
    ];
    for (id, tool_name, question, windows, unix) in cases {
        let sb = Sandbox::new();
        let cache = if id == "pnpm-store" && cfg!(windows) {
            sb.home().join("AppData/Local/pnpm/store")
        } else {
            local_default(&sb, windows, unix)
        };
        sb.write_file(&cache.join("v10/files/entry"), 2);
        tool(&sb, tool_name, vec![(question, no_answer())]);

        let plan = sb.json(&["clean", id]);
        let row = plan_row(&plan, id);
        assert_eq!(row["cleanable"], false, "{id}: {row}");
        assert!(
            row["reason"].as_str().unwrap().contains("did not say")
                || row["reason"].as_str().unwrap().contains("use --full"),
            "{id}: {row}"
        );
        assert!(
            sb.calls()
                .iter()
                .all(|c| !c.contains("clean") && !c.contains("prune"))
        );
    }
}

#[test]
fn a_tool_that_answers_is_cleaned_as_before() {
    let sb = Sandbox::new();
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 2);
    tool(&sb, "npm", vec![("config get cache", prints(&cache))]);

    let plan = sb.json(&["clean", "npm-cache", "--full"]);

    assert_eq!(plan_row(&plan, "npm-cache")["cleanable"], true);
}

#[test]
fn an_absent_tool_gets_its_own_reason_not_this_one() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/entry"), 2);

    let plan = sb.json(&["clean", "npm-cache", "--full"]);

    let reason = plan_row(&plan, "npm-cache")["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(reason.contains("not on PATH"), "{reason}");
}

#[test]
fn a_full_cleanup_deleting_a_folder_it_recognises_is_not_held_back() {
    let sb = Sandbox::new();
    let store = sb.root().join("no-answer-store");
    sb.write_file(&store.join("v10/files/entry"), 2);
    tool(
        &sb,
        "pnpm",
        vec![("store path", prints(&store.join("v10")))],
    );

    let plan = sb.json(&["clean", "pnpm-store", "--full"]);

    assert_eq!(
        plan_row(&plan, "pnpm-store")["action"],
        "delete permanently"
    );
}
