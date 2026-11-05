mod common;

use common::{Sandbox, blocks};
use std::path::{Path, PathBuf};

fn root(sb: &Sandbox) -> PathBuf {
    let dir = sb.root().join("code");
    std::fs::create_dir_all(&dir).unwrap();
    sb.clch()
        .args(["roots", "add"])
        .arg(&dir)
        .assert()
        .success();
    dir
}

fn artifacts(scan: &serde_json::Value) -> Vec<(String, PathBuf)> {
    let mut found: Vec<_> = scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "project-artifact")
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_string(),
                PathBuf::from(r["path"].as_str().unwrap()),
            )
        })
        .collect();
    found.sort();
    found
}

fn text_file(sb: &Sandbox, path: &Path, text: &str) {
    sb.write_file(path, 1);
    std::fs::write(path, text).unwrap();
}

/// A Rust project with a `target` of `count` blocks, as Cargo leaves it.
fn rust(sb: &Sandbox, dir: &Path, count: usize) -> PathBuf {
    text_file(sb, &dir.join("Cargo.toml"), "[package]\n");
    let target = dir.join("target");
    sb.write_file(&target.join("CACHEDIR.TAG"), 1);
    sb.write_file(&target.join("debug/app"), count - 1);
    target
}

/// A Python project with a `.venv` of `count` blocks.
fn python(sb: &Sandbox, dir: &Path, marker: &str, count: usize) -> PathBuf {
    sb.write_file(&dir.join(marker), 1);
    let venv = dir.join(".venv");
    sb.write_file(&venv.join("pyvenv.cfg"), 1);
    sb.write_file(&venv.join("lib/site-packages/x"), count - 1);
    venv
}

/// A Flutter project with its `.dart_tool` and `build`.
fn flutter(sb: &Sandbox, dir: &Path) -> (PathBuf, PathBuf) {
    text_file(
        sb,
        &dir.join("pubspec.yaml"),
        "name: app\ndependencies:\n  flutter:\n    sdk: flutter\n",
    );
    let tool = dir.join(".dart_tool");
    sb.write_file(&tool.join("package_config.json"), 2);
    let build = dir.join("build");
    sb.write_file(&build.join("app/output"), 3);
    (tool, build)
}

#[test]
fn each_kind_is_found_with_its_marker_and_its_own_evidence() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 4);
    let toml_venv = python(&sb, &dir.join("py1"), "pyproject.toml", 2);
    let req_venv = python(&sb, &dir.join("py2"), "requirements.txt", 2);
    let (tool, build) = flutter(&sb, &dir.join("app"));

    let scan = sb.json(&["scan"]);

    let mut expected = vec![
        ("target".to_string(), target),
        (".venv".to_string(), toml_venv),
        (".venv".to_string(), req_venv),
        (".dart_tool".to_string(), tool),
        ("build".to_string(), build),
    ];
    expected.sort();
    assert_eq!(artifacts(&scan), expected);
}

#[test]
fn a_folder_with_the_right_name_and_no_marker_is_not_listed() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    sb.write_file(&dir.join("a/target/CACHEDIR.TAG"), 1);
    sb.write_file(&dir.join("b/.venv/pyvenv.cfg"), 1);
    sb.write_file(&dir.join("c/.dart_tool/package_config.json"), 1);
    sb.write_file(&dir.join("d/build/out"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifacts(&scan), vec![]);
}

#[test]
fn a_marker_alone_does_not_make_a_folder_an_artifact() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    sb.write_file(&dir.join("a/Cargo.toml"), 1);
    sb.write_file(&dir.join("a/target/my-notes"), 1);
    sb.write_file(&dir.join("b/pyproject.toml"), 1);
    sb.write_file(&dir.join("b/.venv/not-a-venv"), 1);
    sb.write_file(&dir.join("c/pubspec.yaml"), 1);
    sb.write_file(&dir.join("c/.dart_tool/other"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifacts(&scan), vec![]);
}

#[test]
fn a_dart_package_that_is_not_flutter_keeps_its_build_folder() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    text_file(&sb, &dir.join("pkg/pubspec.yaml"), "name: pkg\n");
    sb.write_file(&dir.join("pkg/build/handmade"), 1);
    sb.write_file(&dir.join("pkg/.dart_tool/package_config.json"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(
        artifacts(&scan),
        vec![(".dart_tool".to_string(), dir.join("pkg/.dart_tool"))]
    );
}

#[test]
fn a_name_like_flutter_elsewhere_in_a_pubspec_does_not_count() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    text_file(
        &sb,
        &dir.join("pkg/pubspec.yaml"),
        "name: pkg\ndescription: |\n  flutter:\n  not for it\ndev_dependencies:\n  flutter:\n    path: ../flutter\n",
    );
    sb.write_file(&dir.join("pkg/build/handmade"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifacts(&scan), vec![]);
}

#[test]
fn a_build_or_target_without_a_marker_is_still_searched_for_projects() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let inside_build = rust(&sb, &dir.join("build/tool"), 2);
    let inside_target = rust(&sb, &dir.join("target/tool"), 2);

    let scan = sb.json(&["scan"]);

    let found: Vec<PathBuf> = artifacts(&scan).into_iter().map(|(_, p)| p).collect();
    assert!(found.contains(&inside_build), "{found:?}");
    assert!(found.contains(&inside_target), "{found:?}");
}

#[test]
fn nothing_inside_an_artifact_is_listed() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let (_, build) = flutter(&sb, &dir.join("app"));
    rust(&sb, &build.join("macos/inner"), 2);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifacts(&scan).len(), 2, "{scan}");
}

#[test]
fn each_kind_is_deleted_by_its_name_leaving_the_project() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 4);
    let venv = python(&sb, &dir.join("py"), "pyproject.toml", 2);
    let (tool, build) = flutter(&sb, &dir.join("app"));

    let result = sb.json(&["clean", "target", ".venv", ".dart_tool", "build", "--yes"]);

    assert_eq!(result["freed_bytes"], blocks(4 + 2 + 2 + 3));
    for gone in [&target, &venv, &tool, &build] {
        assert!(!gone.exists(), "{}", gone.display());
    }
    assert!(dir.join("rust/Cargo.toml").exists());
    assert!(dir.join("py/pyproject.toml").exists());
    assert!(dir.join("app/pubspec.yaml").exists());
}

#[test]
fn a_kind_name_that_is_also_a_folder_here_stops_for_every_kind() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 2);
    let other = rust(&sb, &dir.join("other"), 2);

    let out = sb
        .clch()
        .args(["clean", "target", "--yes"])
        .current_dir(dir.join("rust"))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(target.exists() && other.exists());
}

#[test]
fn evidence_gone_since_the_plan_stops_the_deletion() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 2);
    std::fs::remove_file(target.join("CACHEDIR.TAG")).unwrap();

    let out = sb
        .clch()
        .arg("clean")
        .arg(&target)
        .arg("--yes")
        .output()
        .unwrap();

    assert_eq!(
        out.status.code(),
        Some(2),
        "no longer an artifact, so not a known path"
    );
    assert!(target.exists());
}

#[test]
fn an_unmarked_build_folder_counts_toward_how_recent_a_project_is() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 2);
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(90 * 24 * 3600);
    fn age(dir: &Path, when: std::time::SystemTime) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                age(&path, when);
            } else {
                let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                file.set_modified(when).unwrap();
            }
        }
    }
    age(&dir.join("rust"), old);
    sb.write_file(&dir.join("rust/build/hand-written-today"), 1);

    let listed = sb.json(&["scan", "--older-than-days", "30"]);

    assert!(
        artifacts(&listed).iter().all(|(_, p)| p != &target),
        "{listed}"
    );
}

#[test]
fn a_repository_cloned_as_target_is_not_an_artifact() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 2);
    sb.write_file(&target.join(".git/HEAD"), 1);

    let scan = sb.json(&["scan"]);
    assert_eq!(artifacts(&scan), vec![]);

    sb.json(&["clean", "target", "--yes"]);
    assert!(target.join(".git/HEAD").exists());
}

#[test]
fn a_folder_with_the_name_of_the_evidence_is_not_evidence() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    sb.write_file(&dir.join("rust/Cargo.toml"), 1);
    sb.write_file(&dir.join("rust/target/CACHEDIR.TAG/inner"), 1);

    let scan = sb.json(&["scan"]);

    assert_eq!(artifacts(&scan), vec![]);
}

/// A fake git that fails on every question.
fn broken_git(sb: &Sandbox) {
    sb.fake_tool("git", "echo broken >&2; exit 128", "exit /b 128");
}

#[test]
fn git_failing_inside_a_repository_protects_the_folder() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("repo/crate"), 2);
    sb.write_file(&dir.join("repo/.git/index"), 1);
    broken_git(&sb);

    let result = sb.json(&["clean", "target", "--yes"]);

    assert_eq!(result["results"][0]["status"], "skipped", "{result}");
    assert!(
        result["results"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("git"),
        "{result}"
    );
    assert!(target.exists());
}

#[test]
fn git_failing_outside_any_repository_does_not_protect_the_folder() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("plain"), 2);
    broken_git(&sb);

    sb.clch()
        .args(["clean", "target", "--yes"])
        .assert()
        .success();

    assert!(!target.exists());
}

#[test]
fn a_kind_name_used_from_inside_a_project_of_that_kind_stops() {
    let sb = Sandbox::new();
    let dir = root(&sb);
    let root_target = rust(&sb, &dir.join("workspace"), 2);
    sb.write_file(&dir.join("workspace/member/Cargo.toml"), 1);
    let other = rust(&sb, &dir.join("other"), 2);

    let out = sb
        .clch()
        .args(["clean", "target", "--yes"])
        .current_dir(dir.join("workspace/member"))
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2));
    assert!(root_target.exists() && other.exists());
}

#[test]
fn old_artifacts_of_every_kind_are_picked_by_age_after_a_preview() {
    fn age(dir: &Path, when: std::time::SystemTime) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                age(&path, when);
            } else {
                let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
                file.set_modified(when).unwrap();
            }
        }
    }
    let sb = Sandbox::new();
    let dir = root(&sb);
    let target = rust(&sb, &dir.join("rust"), 2);
    let venv = python(&sb, &dir.join("py"), "pyproject.toml", 2);
    let (tool, build) = flutter(&sb, &dir.join("app"));
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(90 * 24 * 3600);
    age(&dir, old);
    let every = [&target, &venv, &tool, &build];

    let plan = sb.json(&["clean", "--older-than-days", "30"]);
    assert_eq!(plan["plan"].as_array().unwrap().len(), 4, "{plan}");
    assert!(every.iter().all(|a| a.exists()));

    sb.clch()
        .args(["clean", "--older-than-days", "30", "--yes"])
        .assert()
        .success();
    assert!(every.iter().all(|a| !a.exists()));
}
