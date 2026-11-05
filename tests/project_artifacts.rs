mod common;

use common::{Sandbox, blocks};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn roots(sb: &Sandbox) -> PathBuf {
    let dir = sb.root().join("code");
    std::fs::create_dir_all(&dir).unwrap();
    sb.clch()
        .args(["roots", "add"])
        .arg(&dir)
        .assert()
        .success();
    dir
}

fn artifact<'a>(scan: &'a serde_json::Value, path: &Path) -> Option<&'a serde_json::Value> {
    scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| Path::new(r["path"].as_str().unwrap()) == path)
}

fn artifact_paths(scan: &serde_json::Value) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "project-artifact")
        .map(|r| PathBuf::from(r["path"].as_str().unwrap()))
        .collect();
    paths.sort();
    paths
}

fn age(dir: &Path, days: u64) {
    let when = SystemTime::now() - Duration::from_secs(days * 24 * 3600);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            age(&path, days);
        } else {
            let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.set_modified(when).unwrap();
        }
    }
}

// Scan roots

#[test]
fn scan_roots_are_added_listed_removed_and_kept() {
    let sb = Sandbox::new();
    let (a, b) = (sb.root().join("a"), sb.root().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();

    sb.clch().args(["roots", "add"]).arg(&a).assert().success();
    sb.clch().args(["roots", "add"]).arg(&b).assert().success();
    sb.clch().args(["roots", "add"]).arg(&a).assert().success();
    let listed = sb.json(&["roots", "list"]);
    assert_eq!(listed["scan_roots"].as_array().unwrap().len(), 2);

    sb.clch()
        .args(["roots", "remove"])
        .arg(&a)
        .assert()
        .success();
    let listed = sb.json(&["roots", "list"]);
    let only = listed["scan_roots"].as_array().unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(Path::new(only[0].as_str().unwrap()), b);
    assert!(sb.config_file().is_file());
}

#[test]
fn a_folder_that_does_not_exist_cannot_be_added() {
    let sb = Sandbox::new();

    sb.clch()
        .args(["roots", "add"])
        .arg(sb.root().join("nope"))
        .assert()
        .failure()
        .code(2);

    assert!(!sb.config_file().exists());
}

#[test]
fn removing_a_scan_root_that_is_not_there_is_an_error() {
    let sb = Sandbox::new();

    sb.clch()
        .args(["roots", "remove"])
        .arg(sb.root().join("nope"))
        .assert()
        .failure()
        .code(2);
}

#[test]
fn a_hand_edited_config_file_is_read_and_its_comments_survive() {
    let sb = Sandbox::new();
    let dir = sb.root().join("code");
    sb.node_project(&dir.join("app"), 2);
    let other = sb.root().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    let dir_toml = toml_string(&dir);
    std::fs::write(
        sb.config_file(),
        format!("# my projects\nscan_roots = [{dir_toml}]\n"),
    )
    .unwrap();

    let scan = sb.json(&["scan"]);
    assert_eq!(artifact_paths(&scan).len(), 1);

    sb.clch()
        .args(["roots", "add"])
        .arg(&other)
        .assert()
        .success();
    let written = std::fs::read_to_string(sb.config_file()).unwrap();
    assert!(written.contains("# my projects"), "{written}");
    assert_eq!(
        sb.json(&["roots", "list"])["scan_roots"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

/// `path` as a TOML literal string.
fn toml_string(path: &Path) -> String {
    format!("'{}'", path.display())
}

#[test]
fn a_config_file_that_cannot_be_parsed_stops_clch_and_is_left_alone() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    std::fs::write(sb.config_file(), "scan_roots = [unterminated").unwrap();
    let dir = sb.root().join("code");
    std::fs::create_dir_all(&dir).unwrap();

    sb.clch().arg("scan").assert().failure().code(2);
    sb.clch()
        .args(["roots", "add"])
        .arg(&dir)
        .assert()
        .failure()
        .code(2);

    let written = std::fs::read_to_string(sb.config_file()).unwrap();
    assert_eq!(written, "scan_roots = [unterminated");
}

#[test]
fn a_root_on_the_command_line_overrides_the_config_for_one_run() {
    let sb = Sandbox::new();
    let configured = roots(&sb);
    let one_off = sb.root().join("one-off");
    let in_configured = sb.node_project(&configured.join("a"), 1);
    let in_one_off = sb.node_project(&one_off.join("b"), 1);
    let before = std::fs::read_to_string(sb.config_file()).unwrap();

    let out = sb
        .clch()
        .args(["scan", "--json", "--root"])
        .arg(&one_off)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(artifact_paths(&scan), vec![in_one_off]);
    assert_eq!(std::fs::read_to_string(sb.config_file()).unwrap(), before);
    let later = sb.json(&["scan"]);
    assert!(artifact(&later, &in_configured).is_some());
}

#[test]
fn with_no_scan_roots_project_artifacts_are_skipped_and_the_output_says_so() {
    let sb = Sandbox::new();

    let scan = sb.json(&["scan"]);
    assert_eq!(scan["project_artifacts_skipped"], true);

    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("clch roots add"), "output was:\n{text}");
}

#[test]
fn a_scan_root_that_is_not_there_is_reported() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    std::fs::remove_dir_all(&dir).unwrap();

    let scan = sb.json(&["scan"]);

    let missing = scan["unavailable_roots"].as_array().unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(Path::new(missing[0].as_str().unwrap()), dir);
}

// What counts as a project artifact

#[test]
fn node_modules_with_its_marker_file_is_listed_and_without_it_is_not() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let real = sb.node_project(&dir.join("app"), 3);
    sb.write_file(&dir.join("not-a-project/node_modules/x"), 3);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![real.clone()]);
    let found = artifact(&scan, &real).unwrap();
    assert_eq!(found["id"], "node_modules");
    assert_eq!(found["size_bytes"], blocks(3));
}

#[test]
fn nothing_inside_a_node_modules_is_listed_separately() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let real = sb.node_project(&dir.join("app"), 1);
    sb.node_project(&real.join("dep"), 2);
    let unmarked = dir.join("other/node_modules");
    sb.node_project(&unmarked.join("dep"), 2);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![real.clone()]);
    assert_eq!(artifact(&scan, &real).unwrap()["size_bytes"], blocks(4));
}

#[test]
fn hidden_folders_and_tool_caches_are_not_searched() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/a"), 1);
    sb.node_project(&sb.default_npm_cache().join("_npx/abc"), 1);
    sb.node_project(&sb.home().join(".vscode/extensions/ext"), 1);
    let visible = sb.node_project(&sb.home().join("code/app"), 1);
    sb.clch()
        .args(["roots", "add"])
        .arg(sb.home())
        .assert()
        .success();

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![visible]);
}

#[test]
fn folders_the_operating_system_keeps_for_applications_are_not_searched() {
    let sb = Sandbox::new();
    let kept = if cfg!(windows) { "AppData" } else { "Library" };
    sb.node_project(&sb.home().join(kept).join("Some App"), 1);
    let visible = sb.node_project(&sb.home().join("code/app"), 1);
    sb.clch()
        .args(["roots", "add"])
        .arg(sb.home())
        .assert()
        .success();

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![visible]);
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_neither_listed_nor_searched() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let elsewhere = sb.root().join("elsewhere");
    sb.node_project(&elsewhere.join("outside"), 1);
    std::os::unix::fs::symlink(&elsewhere, dir.join("link")).unwrap();
    let app = dir.join("app");
    sb.write_file(&app.join("package.json"), 1);
    std::os::unix::fs::symlink(
        elsewhere.join("outside/node_modules"),
        app.join("node_modules"),
    )
    .unwrap();

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), Vec::<PathBuf>::new());
}

#[test]
fn nested_scan_roots_list_an_artifact_once() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let real = sb.node_project(&dir.join("app"), 1);
    sb.clch()
        .args(["roots", "add"])
        .arg(dir.join("app"))
        .assert()
        .success();

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![real]);
}

// Age

#[test]
fn an_artifact_shows_when_it_was_last_modified() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    age(modules.parent().unwrap(), 40);
    let expected = (SystemTime::now() - Duration::from_secs(40 * 24 * 3600))
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    let scan = sb.json(&["scan"]);

    let reported = artifact(&scan, &modules).unwrap()["last_modified_ms"]
        .as_i64()
        .unwrap();
    assert!(
        (reported - expected).abs() < 60_000,
        "{reported} vs {expected}"
    );
    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("40 days ago"), "table was:\n{text}");
}

#[test]
fn an_age_filter_narrows_the_list_to_older_artifacts_only() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let old = sb.node_project(&dir.join("old"), 1);
    let recent = sb.node_project(&dir.join("recent"), 1);
    sb.write_file(&sb.default_npm_cache().join("_cacache/a"), 1);
    age(old.parent().unwrap(), 40);
    age(recent.parent().unwrap(), 5);

    let scan = sb.json(&["scan", "--older-than-days", "30"]);

    assert_eq!(artifact_paths(&scan), vec![old]);
    assert!(
        scan["reclaimables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "npm-cache")
    );
}

// Cleaning

#[test]
fn a_plain_clean_never_touches_project_artifacts_and_says_how_many_it_left() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);

    let result = sb.json(&["clean", "--yes"]);
    assert_eq!(result["not_selected"], 1);
    assert!(modules.exists());

    let out = sb
        .clch()
        .arg("clean")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("1 project artifact"), "plan was:\n{text}");
}

#[test]
fn naming_the_kind_deletes_every_one_permanently_after_a_preview() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    let b = sb.node_project(&dir.join("b"), 3);

    let plan = sb.json(&["clean", "node_modules"]);
    assert_eq!(plan["plan"].as_array().unwrap().len(), 2);
    assert!(a.exists() && b.exists());
    assert_eq!(plan["plan"][0]["cleanable"], true);
    assert!(
        plan["plan"][0]["action"]
            .as_str()
            .unwrap()
            .contains("delete")
    );

    let result = sb.json(&["clean", "node_modules", "--yes"]);
    assert!(!a.exists() && !b.exists());
    assert_eq!(result["freed_bytes"], blocks(5));
    assert!(dir.join("a/package.json").exists());
}

#[test]
fn naming_a_path_deletes_only_that_one() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    let b = sb.node_project(&dir.join("b"), 3);

    sb.clch()
        .arg("clean")
        .arg(&a)
        .arg("--yes")
        .assert()
        .success();

    assert!(!a.exists());
    assert!(b.exists());
}

#[test]
fn a_path_that_is_no_project_artifact_is_an_error_and_nothing_is_deleted() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);

    sb.clch()
        .arg("clean")
        .arg(dir.join("a"))
        .arg("--yes")
        .assert()
        .failure()
        .code(2);

    assert!(a.exists());
    assert!(dir.join("a/package.json").exists());
}

#[test]
fn an_age_filter_narrows_the_cleanup_and_selects_without_a_name() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let old = sb.node_project(&dir.join("old"), 1);
    let recent = sb.node_project(&dir.join("recent"), 1);
    age(old.parent().unwrap(), 40);

    sb.clch()
        .args(["clean", "--older-than-days", "30", "--yes"])
        .assert()
        .success();

    assert!(!old.exists());
    assert!(recent.exists());
}

#[cfg(unix)]
#[test]
fn deleting_does_not_follow_a_link_out_of_the_artifact() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    let precious = sb.root().join("precious");
    sb.write_file(&precious.join("keep"), 1);
    std::os::unix::fs::symlink(&precious, modules.join("link")).unwrap();

    sb.clch()
        .args(["clean", "node_modules", "--yes"])
        .assert()
        .success();

    assert!(!modules.exists());
    assert!(precious.join("keep").exists());
}

#[cfg(windows)]
#[test]
fn deleting_does_not_follow_a_junction_out_of_the_artifact() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    let precious = sb.root().join("precious");
    sb.write_file(&precious.join("keep"), 1);
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(modules.join("link"))
        .arg(&precious)
        .output()
        .unwrap();
    assert!(made.status.success());

    sb.clch()
        .args(["clean", "node_modules", "--yes"])
        .assert()
        .success();

    assert!(!modules.exists());
    assert!(precious.join("keep").exists());
}

// History

#[test]
fn artifacts_sharing_a_kind_keep_separate_histories() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    let b = sb.node_project(&dir.join("b"), 2);
    sb.json(&["scan"]);
    sb.write_file(&a.join("more"), 3);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact(&scan, &a).unwrap()["change_bytes"], blocks(3));
    assert_eq!(artifact(&scan, &b).unwrap()["change_bytes"], 0);
}

#[test]
fn a_deleted_artifact_is_reported_gone_once() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(&a).unwrap();

    let first = sb.json(&["scan"]);
    assert_eq!(first["gone"].as_array().unwrap().len(), 1);
    assert_eq!(Path::new(first["gone"][0]["path"].as_str().unwrap()), a);

    let second = sb.json(&["scan"]);
    assert_eq!(second["gone"].as_array().unwrap().len(), 0);
}

#[test]
fn an_artifact_that_was_gone_and_returns_is_new() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(&a).unwrap();
    sb.json(&["scan"]);
    sb.node_project(&dir.join("a"), 2);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact(&scan, &a).unwrap()["change_state"], "new");
}

#[test]
fn a_one_off_root_does_not_make_other_artifacts_gone() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);
    let one_off = sb.root().join("one-off");
    std::fs::create_dir_all(&one_off).unwrap();

    let out = sb
        .clch()
        .args(["scan", "--json", "--root"])
        .arg(&one_off)
        .output()
        .unwrap();
    let partial: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(partial["partial"], true);
    assert_eq!(partial["gone"].as_array().unwrap().len(), 0);

    let whole = sb.json(&["scan"]);
    assert_eq!(artifact(&whole, &a).unwrap()["change_bytes"], 0);
}

#[test]
fn an_age_filter_does_not_make_other_artifacts_gone() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let recent = sb.node_project(&dir.join("recent"), 2);
    sb.json(&["scan"]);

    let filtered = sb.json(&["scan", "--older-than-days", "30"]);
    assert_eq!(filtered["gone"].as_array().unwrap().len(), 0);
    assert_eq!(artifact_paths(&filtered), Vec::<PathBuf>::new());

    let whole = sb.json(&["scan"]);
    assert_eq!(artifact(&whole, &recent).unwrap()["change_bytes"], 0);
}

#[test]
fn a_root_that_went_away_keeps_its_history_for_when_it_returns() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);
    let parked = sb.root().join("unplugged");
    std::fs::rename(&dir, &parked).unwrap();

    let away = sb.json(&["scan"]);
    assert_eq!(away["gone"].as_array().unwrap().len(), 0);

    std::fs::rename(&parked, &dir).unwrap();
    sb.write_file(&a.join("more"), 1);
    let back = sb.json(&["scan"]);
    let found = artifact(&back, &a).unwrap();
    assert_eq!(found["change_state"], "known");
    assert_eq!(found["change_bytes"], blocks(1));
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_under_a_root_does_not_make_what_it_hides_gone() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let dir = roots(&sb);
    sb.node_project(&dir.join("locked/app"), 1);
    sb.json(&["scan"]);
    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();

    let scan = sb.json(&["scan"]);
    std::fs::set_permissions(dir.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(scan["gone"].as_array().unwrap().len(), 0);
    assert_eq!(scan["incomplete_roots"].as_array().unwrap().len(), 1);
}

#[test]
fn what_a_cleanup_deleted_is_not_reported_gone_afterwards() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);

    sb.clch()
        .args(["clean", "node_modules", "--yes"])
        .assert()
        .success();
    let scan = sb.json(&["scan"]);

    assert_eq!(scan["gone"].as_array().unwrap().len(), 0);
}

#[test]
fn a_node_modules_inside_a_tool_cache_belongs_to_the_cache() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let cache_root = dir.join("npm-cache");
    sb.node_project(&cache_root.join("_npx/abc"), 1);
    sb.fake_npm_reporting_cache(&cache_root);
    let project = sb.node_project(&dir.join("app"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifact_paths(&scan), vec![project]);
}

// Decided by the maintainer: age, tracked folders, and selecting by name

#[test]
fn a_recently_edited_project_keeps_an_old_node_modules() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    age(dir.join("app").as_path(), 90);
    sb.write_file(&dir.join("app/src/today.js"), 1);

    let listed = sb.json(&["scan", "--older-than-days", "30"]);
    assert_eq!(artifact_paths(&listed), Vec::<PathBuf>::new());

    sb.clch()
        .args(["clean", "--older-than-days", "30", "--yes"])
        .assert()
        .success();
    assert!(modules.exists());
}

#[test]
fn another_artifact_or_a_hidden_folder_does_not_make_a_project_recent() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    sb.node_project(&dir.join("app/lib"), 1);
    age(dir.join("app").as_path(), 90);
    sb.write_file(&dir.join("app/.git/index"), 1);
    sb.write_file(&dir.join("app/lib/node_modules/fresh"), 1);

    let listed = sb.json(&["scan", "--older-than-days", "30"]);

    assert_eq!(artifact_paths(&listed), vec![modules]);
}

/// A fake `git` saying it tracks a file in whatever `ls-files` is asked about.
fn git_tracking_everything(sb: &Sandbox) {
    sb.fake_tool(
        "git",
        "case \"$*\" in *ls-files*) echo node_modules/dep/index.js ;; esac",
        "echo %* | findstr ls-files >nul && echo node_modules/dep/index.js",
    );
}

#[test]
fn a_node_modules_git_tracks_is_skipped_with_the_reason() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("vendored"), 1);
    git_tracking_everything(&sb);

    let plan = sb.json(&["clean", "node_modules"]);
    assert_eq!(plan["plan"][0]["cleanable"], false);
    assert!(plan["plan"][0]["reason"].as_str().unwrap().contains("git"));

    let result = sb.json(&["clean", "node_modules", "--yes"]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(modules.exists());
}

#[test]
fn real_git_tracking_a_node_modules_protects_it_and_an_untracked_one_goes() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let tracked = sb.node_project(&dir.join("vendored"), 1);
    let untracked = sb.node_project(&dir.join("plain"), 1);
    let real_path = std::env::var("PATH").unwrap();
    let git = |repo: &Path, args: &[&str]| {
        let done = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "{}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    git(&dir.join("vendored"), &["init", "-q"]);
    git(&dir.join("vendored"), &["add", "-A"]);
    git(&dir.join("plain"), &["init", "-q"]);

    let out = sb
        .clch()
        .env("PATH", &real_path)
        .args(["clean", "node_modules", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(tracked.exists());
    assert!(!untracked.exists());
}

#[test]
fn a_git_that_will_not_answer_protects_the_folder() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    sb.limit_tools_to(1);
    sb.fake_tool("git", "sleep 60", "ping -n 60 127.0.0.1 >nul");

    let result = sb.json(&["clean", "node_modules", "--yes"]);

    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(modules.exists());
}

#[test]
fn a_kind_name_that_is_also_a_folder_here_stops_and_says_how_to_choose() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let here = sb.node_project(&dir.join("here"), 1);
    let there = sb.node_project(&dir.join("there"), 1);

    let out = sb
        .clch()
        .args(["clean", "node_modules", "--yes"])
        .current_dir(dir.join("here"))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("./node_modules"));
    assert!(here.exists() && there.exists());

    sb.clch()
        .args(["clean", "./node_modules", "--yes"])
        .current_dir(dir.join("here"))
        .assert()
        .success();
    assert!(!here.exists());
    assert!(there.exists());
}

// Age filter limits

#[test]
fn an_age_filter_does_not_clean_tool_caches() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let old = sb.node_project(&dir.join("old"), 1);
    age(old.parent().unwrap(), 40);
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/a"), 1);
    sb.fake_npm_reporting_cache(&root);

    let result = sb.json(&["clean", "--older-than-days", "30", "--full", "--yes"]);

    assert_eq!(result["results"].as_array().unwrap().len(), 1);
    assert!(!sb.calls().iter().any(|c| c.contains("cache clean")));
    assert!(!old.exists());
}

#[test]
fn an_age_outside_a_sensible_range_is_refused() {
    let sb = Sandbox::new();
    for days in ["0", "36501", "9223372036854775807", "18446744073709551615"] {
        sb.clch()
            .args(["clean", "--yes", "--older-than-days", days])
            .assert()
            .failure()
            .code(2);
        sb.clch()
            .args(["scan", "--older-than-days", days])
            .assert()
            .failure()
            .code(2);
    }
}

#[cfg(unix)]
#[test]
fn a_project_that_cannot_be_fully_read_is_not_picked_by_age() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    sb.write_file(&dir.join("app/src/a.js"), 1);
    age(modules.parent().unwrap(), 40);
    let locked = dir.join("app/src");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    sb.clch()
        .args(["clean", "--older-than-days", "30", "--yes"])
        .assert()
        .success();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(modules.exists());
}

// The plan says what it does

#[test]
fn cleaning_without_scan_roots_says_why_no_artifact_was_found() {
    let sb = Sandbox::new();

    let plan = sb.json(&["clean", "node_modules"]);

    assert_eq!(plan["project_artifacts_skipped"], true);
    assert!(
        plan["plan"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("clch roots add"),
        "{plan}"
    );
}

#[test]
fn a_plan_of_only_project_artifacts_does_not_call_itself_a_cleanup_level() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    sb.node_project(&dir.join("a"), 2);
    sb.node_project(&dir.join("b"), 3);

    let out = sb
        .clch()
        .args(["clean", "node_modules"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(!text.contains("unused cleanup"), "plan was:\n{text}");

    let plan = sb.json(&["clean", "node_modules"]);
    assert_eq!(plan["expected_total_bytes"], blocks(5));
    assert!(plan["plan"][0]["last_modified_ms"].is_number(), "{plan}");
}

#[test]
fn an_empty_plan_does_not_invite_running_it() {
    let sb = Sandbox::new();

    let out = sb
        .clch()
        .arg("clean")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);

    assert!(!text.contains("--yes"), "plan was:\n{text}");
}

#[cfg(unix)]
#[test]
fn a_folder_name_cannot_write_control_codes_into_the_plan() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    sb.node_project(&dir.join("evil\u{1b}[2Jname\nfake row"), 1);

    let scan = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan = sb
        .clch()
        .args(["clean", "node_modules"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    for output in [scan, plan] {
        let text = String::from_utf8_lossy(&output);
        assert!(!text.contains('\u{1b}'), "{text:?}");
        assert!(
            !text.contains("fake row\n") && !text.contains("\nfake row"),
            "{text:?}"
        );
    }
}

// Deleting stays on one disk

#[cfg(target_os = "macos")]
#[test]
fn a_node_modules_with_another_disk_mounted_inside_is_not_deleted() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    let image = sb.root().join("other.dmg");
    let mount = modules.join("mnt");
    std::fs::create_dir_all(&mount).unwrap();
    let hdiutil = |args: &[&str]| {
        let done = std::process::Command::new("/usr/bin/hdiutil")
            .args(args)
            .output()
            .unwrap();
        assert!(
            done.status.success(),
            "hdiutil {args:?}: {}",
            String::from_utf8_lossy(&done.stderr)
        );
    };
    hdiutil(&[
        "create",
        "-quiet",
        "-size",
        "10m",
        "-fs",
        "HFS+",
        "-volname",
        "clchtest",
        image.to_str().unwrap(),
    ]);
    hdiutil(&[
        "attach",
        "-quiet",
        "-nobrowse",
        "-mountpoint",
        mount.to_str().unwrap(),
        image.to_str().unwrap(),
    ]);
    std::fs::write(mount.join("keep"), b"x").unwrap();

    let out = sb
        .clch()
        .args(["clean", "node_modules", "--yes", "--json"])
        .output()
        .unwrap();
    let survived = mount.join("keep").exists();
    hdiutil(&["detach", "-quiet", mount.to_str().unwrap()]);

    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["results"][0]["status"], "failed");
    assert!(
        result["results"][0]["output"]
            .as_str()
            .unwrap()
            .contains("another disk")
    );
    assert!(survived);
}

// Exclusions and path spellings

#[test]
fn a_root_spelled_with_dots_still_keeps_out_of_application_folders() {
    let sb = Sandbox::new();
    let kept = if cfg!(windows) { "AppData" } else { "Library" };
    sb.node_project(&sb.home().join(kept).join("Some App"), 1);
    let visible = sb.node_project(&sb.home().join("code/app"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json", "--root", ".."])
        .current_dir(sb.home().join("code"))
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let found: Vec<PathBuf> = artifact_paths(&scan)
        .iter()
        .map(|p| p.canonicalize().unwrap())
        .collect();
    assert_eq!(found, vec![visible.canonicalize().unwrap()]);
}

#[cfg(unix)]
#[test]
fn a_root_that_links_to_home_still_keeps_out_of_application_folders() {
    let sb = Sandbox::new();
    sb.node_project(&sb.home().join("Library/Some App"), 1);
    let link = sb.root().join("link-to-home");
    std::os::unix::fs::symlink(sb.home(), &link).unwrap();

    let out = sb
        .clch()
        .args(["scan", "--json", "--root"])
        .arg(&link)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(artifact_paths(&scan), Vec::<PathBuf>::new());
}

#[test]
fn a_root_inside_an_application_folder_is_not_searched_and_is_reported() {
    let sb = Sandbox::new();
    let kept = if cfg!(windows) { "AppData" } else { "Library" };
    let inside = sb.home().join(kept).join("Some App");
    sb.node_project(&inside, 1);

    let out = sb
        .clch()
        .args(["scan", "--json", "--root"])
        .arg(&inside)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(artifact_paths(&scan), Vec::<PathBuf>::new());
    assert_eq!(scan["unavailable_roots"].as_array().unwrap().len(), 1);
}

#[test]
fn a_root_that_is_itself_a_node_modules_lists_only_itself() {
    let sb = Sandbox::new();
    let modules = sb.node_project(&sb.root().join("app"), 1);
    sb.node_project(&modules.join("dep"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json", "--root"])
        .arg(&modules)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(artifact_paths(&scan), vec![modules]);
}

// Hand-written settings

#[test]
fn a_tilde_in_the_config_means_the_home_folder_and_can_be_removed() {
    let sb = Sandbox::new();
    let visible = sb.node_project(&sb.home().join("code/app"), 1);
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    std::fs::write(sb.config_file(), "scan_roots = ['~/code']\n").unwrap();

    let scan = sb.json(&["scan"]);
    assert_eq!(artifact_paths(&scan), vec![visible]);

    sb.clch()
        .args(["roots", "remove"])
        .arg(sb.home().join("code"))
        .assert()
        .success();
    assert_eq!(
        sb.json(&["roots", "list"])["scan_roots"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn a_relative_root_in_the_config_is_refused_rather_than_following_the_current_folder() {
    let sb = Sandbox::new();
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    std::fs::write(sb.config_file(), "scan_roots = ['.']\n").unwrap();

    let out = sb.clch().arg("scan").output().unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("absolute"));
}

#[test]
fn comments_inside_the_list_of_roots_survive_an_edit() {
    let sb = Sandbox::new();
    let (a, b) = (sb.root().join("a"), sb.root().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    std::fs::write(
        sb.config_file(),
        format!("scan_roots = [\n  # work\n  {},\n]\n", toml_string(&a)),
    )
    .unwrap();

    sb.clch().args(["roots", "add"]).arg(&b).assert().success();

    let written = std::fs::read_to_string(sb.config_file()).unwrap();
    assert!(written.contains("# work"), "{written}");
    assert_eq!(
        sb.json(&["roots", "list"])["scan_roots"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn adding_roots_at_the_same_time_loses_none() {
    let sb = Sandbox::new();
    let folders: Vec<PathBuf> = (0..24).map(|i| sb.root().join(format!("r{i}"))).collect();
    for f in &folders {
        std::fs::create_dir_all(f).unwrap();
    }

    std::thread::scope(|scope| {
        for f in &folders {
            let sb = &sb;
            scope.spawn(move || {
                sb.clch().args(["roots", "add"]).arg(f).assert().success();
            });
        }
    });

    assert_eq!(
        sb.json(&["roots", "list"])["scan_roots"]
            .as_array()
            .unwrap()
            .len(),
        24
    );
}

#[cfg(unix)]
#[test]
fn a_config_file_that_is_a_link_stays_one_and_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let real = sb.root().join("dotfiles/clch.toml");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, "").unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::create_dir_all(sb.config_file().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&real, sb.config_file()).unwrap();
    let dir = sb.root().join("code");
    std::fs::create_dir_all(&dir).unwrap();

    sb.clch()
        .args(["roots", "add"])
        .arg(&dir)
        .assert()
        .success();

    assert!(
        std::fs::symlink_metadata(sb.config_file())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(std::fs::read_to_string(&real).unwrap().contains("code"));
    assert_eq!(
        std::fs::metadata(&real).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

// History edge cases

#[test]
fn a_tool_cache_still_on_disk_is_not_gone_because_its_tool_is_away() {
    let sb = Sandbox::new();
    let custom = sb.root().join("custom");
    sb.write_file(&custom.join("_cacache/a"), 2);
    sb.fake_npm_reporting_cache(&custom);
    sb.json(&["scan"]);
    std::fs::remove_file(if cfg!(windows) {
        sb.bin().join("npm.cmd")
    } else {
        sb.bin().join("npm")
    })
    .unwrap();

    let away = sb.json(&["scan"]);
    assert_eq!(away["gone"].as_array().unwrap().len(), 0);

    sb.fake_npm_reporting_cache(&custom);
    let back = sb.json(&["scan"]);
    let npm = back["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "npm-cache")
        .unwrap();
    assert_eq!(npm["change_state"], "known");
}

#[test]
fn something_deleted_by_hand_before_a_cleanup_is_still_reported_gone_once() {
    let sb = Sandbox::new();
    let dir = roots(&sb);
    let a = sb.node_project(&dir.join("a"), 2);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(&a).unwrap();

    sb.clch().args(["clean", "--yes"]).assert().success();
    let first = sb.json(&["scan"]);
    assert_eq!(first["gone"].as_array().unwrap().len(), 1);

    let second = sb.json(&["scan"]);
    assert_eq!(second["gone"].as_array().unwrap().len(), 0);
}

#[cfg(unix)]
#[test]
fn asking_git_does_not_run_what_a_cloned_repositorys_config_names() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let dir = roots(&sb);
    let modules = sb.node_project(&dir.join("app"), 1);
    let ran = sb.root().join("ran");
    let script = sb.root().join("evil.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let git = |args: &[&str]| {
        let done = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.join("app"))
            .output()
            .unwrap();
        assert!(done.status.success());
    };
    git(&["init", "-q"]);
    git(&["config", "core.fsmonitor", script.to_str().unwrap()]);

    sb.clch()
        .env("PATH", std::env::var("PATH").unwrap())
        .args(["clean", "node_modules"])
        .assert()
        .success();

    assert!(modules.exists());
    assert!(!ran.exists(), "git ran the repository's fsmonitor");
}

#[test]
fn a_folder_ending_in_app_is_not_searched() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let inside = sb.node_project(&root.join("Tool.app/Contents/Resources/app"), 2);
    let outside = sb.node_project(&root.join("project"), 1);
    let root = root.display().to_string();

    let scan = sb.json(&["scan", "--root", &root]);

    let paths: Vec<&str> = scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == "node_modules")
        .map(|r| r["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [outside.to_str().unwrap()]);
    assert!(inside.exists());
}

#[test]
fn a_folder_merely_containing_app_in_its_name_is_searched() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    sb.node_project(&root.join("my.application"), 1);
    sb.node_project(&root.join("app"), 1);
    let root = root.display().to_string();

    let scan = sb.json(&["scan", "--root", &root]);

    let found = scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == "node_modules")
        .count();
    assert_eq!(found, 2);
}

#[test]
fn a_scan_root_named_like_a_bundle_is_searched_as_it_was_named() {
    let sb = Sandbox::new();
    let root = sb.root().join("Mine.app");
    sb.node_project(&root.join("project"), 1);
    let root = root.display().to_string();

    let scan = sb.json(&["scan", "--root", &root]);

    assert!(scan.to_string().contains("node_modules"), "{scan}");
}
