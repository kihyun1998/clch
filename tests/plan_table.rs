mod common;

use common::Sandbox;

fn workspace_root(sb: &Sandbox) -> (std::path::PathBuf, String) {
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("mono"), 2);
    sb.write_file(&root.join("mono").join("pnpm-workspace.yaml"), 1);
    (modules, root.display().to_string())
}

fn table(sb: &Sandbox, args: &[&str]) -> String {
    let out = sb.clch().args(args).output().unwrap();
    assert!(out.status.success(), "{:?}", out);
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn deleting_a_pnpm_workspace_roots_node_modules_warns_about_its_members() {
    let sb = Sandbox::new();
    let (modules, root) = workspace_root(&sb);
    let name = modules.to_str().unwrap();

    let plan = sb.json(&["clean", name, "--root", &root]);
    let warning = plan["plan"][0]["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("workspace"), "{plan}");

    let text = table(&sb, &["clean", name, "--root", &root]);
    assert!(
        text.contains("Warning:") && text.contains("workspace"),
        "{text}"
    );
    assert!(modules.exists());
}

#[test]
fn another_node_modules_has_no_such_warning() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("plain"), 2);
    let root = root.display().to_string();

    let plan = sb.json(&["clean", modules.to_str().unwrap(), "--root", &root]);

    assert!(plan["plan"][0].get("warning").is_none(), "{plan}");
}

#[test]
fn a_long_plan_has_its_total_first_and_its_rows_fit_80_columns() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    for i in 0..40 {
        sb.node_project(&root.join(format!("project{i}")), 1);
    }
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 1);
    sb.fake_npm_reporting_cache(&cache);
    let root = root.display().to_string();

    let text = table(
        &sb,
        &[
            "clean",
            "node_modules",
            "npm-cache",
            "--root",
            &root,
            "--full",
        ],
    );

    let lines: Vec<&str> = text.lines().collect();
    let total = lines
        .iter()
        .position(|l| l.contains("Frees at most"))
        .expect("a total");
    assert!(total < 5, "{text}");
    let first_row = lines
        .iter()
        .position(|l| l.contains("node_modules"))
        .unwrap();
    assert!(total < first_row, "{text}");
    for line in &lines {
        let is_path = line.trim_start().starts_with(sb.root().to_str().unwrap());
        assert!(is_path || line.len() <= 80, "too wide: {line}");
    }
}

#[test]
fn every_row_keeps_its_action_and_path() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("one"), 1);
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 1);
    sb.fake_npm_reporting_cache(&cache);
    let root = root.display().to_string();

    let text = table(
        &sb,
        &[
            "clean",
            "node_modules",
            "npm-cache",
            "--root",
            &root,
            "--full",
        ],
    );

    assert!(text.contains("delete permanently"), "{text}");
    assert!(text.contains("npm cache clean --force"), "{text}");
    assert!(text.contains(modules.to_str().unwrap()), "{text}");
    assert!(
        text.contains(cache.join("_cacache").to_str().unwrap()),
        "{text}"
    );
    assert!(text.contains("today"), "{text}");
}

#[test]
fn a_skipped_row_keeps_its_reason_within_the_columns() {
    let sb = Sandbox::new();
    sb.write_file(&sb.home().join(".cargo/registry/stray/file"), 1);

    let text = table(&sb, &["clean", "cargo-registry", "--full"]);

    assert!(text.contains("skipped:"), "{text}");
    assert!(text.contains("stray"), "{text}");
    let reason = text
        .lines()
        .find(|l| l.contains("skipped:"))
        .expect("the reason");
    assert!(reason.len() <= 80, "too wide: {reason}");
}

#[cfg(unix)]
#[test]
fn a_deletion_that_failed_partway_says_to_reinstall() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("app"), 1);
    for i in 0..200 {
        sb.write_file(&modules.join(format!("file{i}")), 1);
    }
    let locked = modules.join("locked");
    sb.write_file(&locked.join("kept"), 1);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let root = root.display().to_string();

    let out = sb
        .clch()
        .args([
            "clean",
            modules.to_str().unwrap(),
            "--root",
            &root,
            "--yes",
            "--json",
        ])
        .output()
        .unwrap();

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(out.status.code(), Some(1));
    let outcome: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = &outcome["results"][0];
    assert_eq!(row["status"], "failed");
    assert!(row["freed_bytes"].as_i64().unwrap() > 0, "{row}");
    let said = row["output"].as_str().unwrap();
    assert!(said.contains("reinstall"), "{said}");
}

#[cfg(unix)]
#[test]
fn a_deletion_that_removed_nothing_does_not_say_to_reinstall() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let modules = sb.node_project(&root.join("app"), 1);
    let locked = modules.join("dep");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    let root = root.display().to_string();

    let out = sb
        .clch()
        .args([
            "clean",
            modules.to_str().unwrap(),
            "--root",
            &root,
            "--yes",
            "--json",
        ])
        .output()
        .unwrap();

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let outcome: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let row = &outcome["results"][0];
    assert_eq!(row["status"], "failed");
    assert!(
        !row["output"].as_str().unwrap().contains("reinstall"),
        "{row}"
    );
}
