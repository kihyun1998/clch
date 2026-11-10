mod common;

use common::{Reply, Sandbox, prints, removes, says, tool};
use std::path::PathBuf;

/// A sandbox whose npm cache holds one orphan file that `npm cache verify`
/// removes, behind a fake npm; and the npm root the fake reports.
fn npm_machine() -> (Sandbox, PathBuf) {
    let sb = Sandbox::new();
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/content-v2/kept"), 10);
    sb.write_file(&root.join("_npx/0abc/node_modules/x/index.js"), 3);
    let orphan = root.join("_cacache/content-v2/sha512/00/orphan");
    sb.write_file(&orphan, 5);
    tool(
        &sb,
        "npm",
        vec![
            ("--version", says("11.6.2")),
            ("config get cache", prints(&root)),
            ("cache verify", removes(&orphan)),
        ],
    );
    (sb, root)
}

/// Like `npm_machine`, but `npm cache verify` fails with two lines of complaint.
fn failing_npm_machine() -> Sandbox {
    let (sb, root) = npm_machine();
    tool(
        &sb,
        "npm",
        vec![
            ("--version", says("11.6.2")),
            ("config get cache", prints(&root)),
            (
                "cache verify",
                Reply {
                    unix: "echo first complaint >&2; echo second complaint >&2; exit 1".into(),
                    windows: "echo first complaint 1>&2\r\necho second complaint 1>&2\r\nexit /b 1"
                        .into(),
                },
            ),
        ],
    );
    sb
}

fn stderr_lines(out: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stderr)
        .lines()
        .map(str::to_string)
        .collect()
}

/// Asserts that each of `wanted` is in a line of `lines`, each after the one
/// before it.
fn assert_in_order(lines: &[String], wanted: &[&str]) {
    let mut from = 0;
    for want in wanted {
        match lines[from..].iter().position(|l| l.contains(want)) {
            Some(at) => from += at + 1,
            None => panic!(
                "no line with {want:?} after line {from} in:\n{}",
                lines.join("\n")
            ),
        }
    }
}

#[test]
fn a_cleanup_names_its_steps_and_each_reclaimable_it_cleans() {
    let (sb, _) = npm_machine();

    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--yes"])
        .output()
        .unwrap();

    assert!(out.status.success());
    assert_in_order(
        &stderr_lines(&out),
        &[
            "[1/3] Scanning",
            "[2/3] Cleaning 1 reclaimable",
            "[1/1] npm cache: npm cache verify",
            "cleaned (",
            "[3/3] Measuring what was freed",
        ],
    );
}

#[test]
fn a_failed_cleanup_says_why_on_one_line_and_keeps_the_rest_in_the_result() {
    let sb = failing_npm_machine();

    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--yes"])
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    let told = stderr_lines(&out);
    assert_in_order(
        &told,
        &[
            "[1/1] npm cache: npm cache verify",
            "failed: first complaint (",
        ],
    );
    assert!(
        !told.iter().any(|l| l.contains("second complaint")),
        "{told:?}"
    );
    let result = String::from_utf8_lossy(&out.stdout);
    assert!(result.contains("second complaint"), "{result}");
}

#[test]
fn a_reclaimable_that_is_skipped_takes_no_number() {
    let (sb, _) = npm_machine();

    let out = sb.clch().args(["clean", "--yes"]).output().unwrap();

    assert!(out.status.success());
    let told = stderr_lines(&out);
    assert_in_order(&told, &["[2/3] Cleaning 1 reclaimable", "[1/1] npm cache:"]);
    assert!(!told.iter().any(|l| l.contains("/2]")), "{told:?}");
    let result = String::from_utf8_lossy(&out.stdout);
    assert!(
        result.contains("npx cache") && result.contains("skipped"),
        "{result}"
    );
}

#[test]
fn a_project_artifact_is_named_by_its_folder() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let first = sb.node_project(&root.join("first"), 1);
    let second = sb.node_project(&root.join("second"), 1);

    let out = sb
        .clch()
        .args(["clean", "node_modules", "--yes", "--root"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(out.status.success());
    let told = stderr_lines(&out);
    assert_in_order(&told, &["[2/3] Cleaning 2 reclaimables"]);
    for folder in [&first, &second] {
        let named = folder.display().to_string();
        assert!(
            told.iter().any(|l| l.contains("/2]")
                && l.contains(&named)
                && l.contains("delete permanently")),
            "{named} not named in:\n{}",
            told.join("\n")
        );
    }
}

#[test]
fn each_scan_says_where_it_searches_and_what_it_found() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    sb.node_project(&root.join("app"), 1);

    let out = sb
        .clch()
        .args(["clean", "node_modules", "--yes", "--root"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(out.status.success());
    assert_in_order(
        &stderr_lines(&out),
        &[
            "[1/3] Scanning",
            &format!("searching {}", root.display()),
            "measuring 1 project artifact",
            "found 1 reclaimable (",
            "[2/3] Cleaning 1 reclaimable",
            "[3/3] Measuring what was freed",
            &format!("searching {}", root.display()),
            "found 0 reclaimables (",
        ],
    );
}

#[test]
fn a_preview_shows_only_the_scanning_step() {
    let (sb, _) = npm_machine();

    let out = sb.clch().args(["clean", "npm-cache"]).output().unwrap();

    assert!(out.status.success());
    let told = stderr_lines(&out);
    assert_in_order(&told, &["[1/1] Scanning", "found 2 reclaimables ("]);
    assert!(
        !told.iter().any(|l| l.contains("/3]")),
        "a preview has one step: {told:?}"
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Nothing has been removed"));
}

#[test]
fn json_output_stays_clean_while_the_steps_are_told() {
    let (sb, _) = npm_machine();

    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--yes", "--json"])
        .output()
        .unwrap();

    let told = stderr_lines(&out);
    assert_in_order(&told, &["[2/3] Cleaning 1 reclaimable", "cleaned ("]);
    let result: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("only JSON on stdout");
    assert_eq!(result["results"][0]["status"], "cleaned");
}

#[test]
fn the_lines_hold_no_terminal_controls() {
    let (sb, _) = npm_machine();

    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--yes"])
        .output()
        .unwrap();

    assert!(!out.stderr.is_empty());
    assert!(!out.stderr.contains(&0x1b), "{:?}", out.stderr);
    assert!(!out.stderr.contains(&b'\r') || cfg!(windows));
}

#[test]
fn a_failure_reason_holds_no_terminal_controls() {
    let (sb, root) = npm_machine();
    tool(
        &sb,
        "npm",
        vec![
            ("--version", says("11.6.2")),
            ("config get cache", prints(&root)),
            (
                "cache verify",
                Reply {
                    unix: "printf 'bad\\033[2Jtail\\n' >&2; exit 1".into(),
                    windows: "echo bad\u{1b}[2Jtail 1>&2\r\nexit /b 1".into(),
                },
            ),
        ],
    );

    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--yes"])
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr_lines(&out).iter().any(|l| l.contains("failed: bad")),
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.stderr.contains(&0x1b), "{:?}", out.stderr);
}

#[test]
fn a_scan_says_which_tool_cache_it_is_looking_for() {
    let (sb, _) = npm_machine();

    let out = sb.clch().args(["clean", "npm-cache"]).output().unwrap();

    assert!(out.status.success());
    let told = stderr_lines(&out);
    assert_in_order(
        &told,
        &[
            "[1/1] Scanning",
            "looking for the npm cache",
            "looking for the npx cache",
            "found 2 reclaimables (",
        ],
    );
}
