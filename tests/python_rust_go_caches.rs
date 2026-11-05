mod common;

use common::{Sandbox, blocks, prints, removes, tool};
use std::path::{Path, PathBuf};

fn only<'a>(scan: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    let found: Vec<_> = scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == id)
        .collect();
    assert_eq!(found.len(), 1, "{id} in {scan}");
    found[0]
}

fn path_of(value: &serde_json::Value) -> &Path {
    Path::new(value["path"].as_str().unwrap())
}

fn calls_with(sb: &Sandbox, text: &str) -> Vec<String> {
    sb.calls()
        .into_iter()
        .filter(|c| c.contains(text))
        .collect()
}

fn local(sb: &Sandbox, windows: &str, unix: &str) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join(windows)
    } else {
        sb.home().join(unix)
    }
}

/// A sandbox with a cache at a path only the tool knows.
fn custom(name: &str) -> (Sandbox, PathBuf) {
    let sb = Sandbox::new();
    let cache = sb.root().join(name);
    sb.write_file(&cache.join("entry"), 3);
    (sb, cache)
}

// pip

#[test]
fn pip_says_where_its_cache_is_and_purges_it_only_in_full() {
    let (sb, cache) = custom("pip-custom");
    tool(
        &sb,
        "pip",
        vec![
            ("cache dir", prints(&cache)),
            ("cache purge", removes(&cache)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "pip-cache")), cache);

    let unused = sb.json(&["clean", "pip-cache", "--yes"]);
    assert_eq!(unused["results"][0]["status"], "skipped");
    assert!(cache.exists());

    let full = sb.json(&["clean", "pip-cache", "--full", "--yes"]);
    assert_eq!(full["freed_bytes"], blocks(3));
    assert_eq!(calls_with(&sb, "cache purge").len(), 1);
}

#[test]
fn pip3_stands_in_where_there_is_no_pip() {
    let (sb, cache) = custom("pip-custom");
    tool(
        &sb,
        "pip3",
        vec![
            ("cache dir", prints(&cache)),
            ("cache purge", removes(&cache)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "pip-cache")), cache);

    sb.clch()
        .args(["clean", "pip-cache", "--full", "--yes"])
        .assert()
        .success();
    assert_eq!(calls_with(&sb, "pip3 cache purge").len(), 1);
    assert!(!cache.exists());
}

#[test]
fn without_pip_its_default_cache_is_found_and_not_cleanable() {
    let sb = Sandbox::new();
    let cache = local(&sb, "pip/Cache", "Library/Caches/pip");
    sb.write_file(&cache.join("wheels/a"), 2);

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "pip-cache")), cache);

    let result = sb.json(&["clean", "pip-cache", "--full", "--yes"]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(
        result["results"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("pip"),
        "{result}"
    );
    assert!(cache.exists());
}

// uv

#[test]
fn uv_says_where_its_cache_is_with_both_levels_of_cleanup() {
    let (sb, cache) = custom("uv-custom");
    let orphan = cache.join("orphan");
    sb.write_file(&orphan, 2);
    tool(
        &sb,
        "uv",
        vec![
            ("cache dir", prints(&cache)),
            ("cache prune", removes(&orphan)),
            ("cache clean", removes(&cache)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "uv-cache")), cache);

    let unused = sb.json(&["clean", "uv-cache", "--yes"]);
    assert_eq!(unused["freed_bytes"], blocks(2));
    assert!(cache.join("entry").exists());

    let full = sb.json(&["clean", "uv-cache", "--full", "--yes"]);
    assert_eq!(full["freed_bytes"], blocks(3));
    assert!(!cache.exists());
}

#[test]
fn without_uv_its_cache_stays_since_its_docs_forbid_touching_it() {
    let sb = Sandbox::new();
    let cache = local(&sb, "uv/cache", ".cache/uv");
    sb.write_file(&cache.join("a"), 2);

    let result = sb.json(&["clean", "uv-cache", "--full", "--yes"]);

    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(
        result["results"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("not on PATH"),
        "{result}"
    );
    assert!(cache.exists());
}

// Cargo, whose docs allow deleting its caches and which has no stable command

fn cargo_home(sb: &Sandbox) -> PathBuf {
    let home = sb.home().join(".cargo");
    sb.write_file(&home.join("registry/cache/index/a.crate"), 2);
    sb.write_file(&home.join("registry/src/index/a/lib.rs"), 3);
    sb.write_file(&home.join("git/db/repo/a"), 4);
    sb.write_file(&home.join("bin/cargo-tool"), 5);
    home
}

#[test]
fn cargos_registry_and_git_caches_are_listed_apart_from_its_binaries() {
    let sb = Sandbox::new();
    let home = cargo_home(&sb);

    let scan = sb.json(&["scan"]);

    let registry = only(&scan, "cargo-registry");
    assert_eq!(path_of(registry), home.join("registry"));
    assert_eq!(registry["size_bytes"], blocks(5));
    assert_eq!(only(&scan, "cargo-git")["size_bytes"], blocks(4));
}

#[test]
fn cargo_home_moves_them() {
    let sb = Sandbox::new();
    let custom = sb.root().join("cargo-custom");
    sb.write_file(&custom.join("registry/cache/a"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("CARGO_HOME", &custom)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(
        path_of(only(&scan, "cargo-registry")),
        custom.join("registry")
    );
}

#[test]
fn cargos_caches_are_deleted_only_in_full_and_with_cargo_there_or_not() {
    for with_cargo in [false, true] {
        let sb = Sandbox::new();
        let home = cargo_home(&sb);
        if with_cargo {
            tool(&sb, "cargo", vec![]);
        }

        let unused = sb.json(&["clean", "cargo-registry", "--yes"]);
        assert_eq!(unused["results"][0]["status"], "skipped");
        assert!(
            unused["results"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("--full"),
            "{unused}"
        );
        assert!(home.join("registry").exists());

        let full = sb.json(&["clean", "cargo-registry", "cargo-git", "--full", "--yes"]);
        assert_eq!(full["freed_bytes"], blocks(9));
        assert!(!home.join("registry").exists());
        assert!(!home.join("git").exists());
        assert!(home.join("bin/cargo-tool").exists());
        assert_eq!(sb.calls().len(), 0, "{:?}", sb.calls());
    }
}

#[cfg(unix)]
#[test]
fn a_cache_folder_that_is_a_link_is_not_deleted_directly() {
    let sb = Sandbox::new();
    let elsewhere = sb.root().join("elsewhere");
    sb.write_file(&elsewhere.join("keep"), 1);
    std::fs::create_dir_all(sb.home().join(".cargo")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, sb.home().join(".cargo/registry")).unwrap();

    let out = sb
        .clch()
        .args(["clean", "cargo-registry", "--full", "--yes", "--json"])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(elsewhere.join("keep").exists());
}

// Go

#[test]
fn go_says_where_both_its_caches_are_and_cleans_each_only_in_full() {
    let sb = Sandbox::new();
    let (build, modules) = (sb.root().join("go-build"), sb.root().join("go-mod"));
    sb.write_file(&build.join("aa/a"), 2);
    sb.write_file(&modules.join("cache/a"), 3);
    tool(
        &sb,
        "go",
        vec![
            ("env GOCACHE", prints(&build)),
            ("env GOMODCACHE", prints(&modules)),
            ("clean -cache", removes(&build)),
            ("clean -modcache", removes(&modules)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "go-build-cache")), build);
    assert_eq!(path_of(only(&scan, "go-module-cache")), modules);

    let unused = sb.json(&["clean", "go-build-cache", "--yes"]);
    assert_eq!(unused["results"][0]["status"], "skipped");
    assert!(build.exists());

    sb.clch()
        .args(["clean", "go-build-cache", "--full", "--yes"])
        .assert()
        .success();
    assert!(!build.exists() && modules.exists());
    sb.clch()
        .args(["clean", "go-module-cache", "--full", "--yes"])
        .assert()
        .success();
    assert!(!modules.exists());
}

#[test]
fn without_go_its_default_caches_are_found_and_not_cleanable() {
    let sb = Sandbox::new();
    let build = local(&sb, "go-build", "Library/Caches/go-build");
    let modules = sb.home().join("go/pkg/mod");
    sb.write_file(&build.join("aa/a"), 2);
    sb.write_file(&modules.join("cache/a"), 3);

    let scan = sb.json(&["scan"]);
    assert_eq!(path_of(only(&scan, "go-build-cache")), build);
    assert_eq!(path_of(only(&scan, "go-module-cache")), modules);

    let result = sb.json(&[
        "clean",
        "go-build-cache",
        "go-module-cache",
        "--full",
        "--yes",
    ]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert_eq!(result["results"][1]["status"], "skipped");
    assert!(build.exists() && modules.exists());
}

#[test]
fn gopath_and_cache_variables_set_the_defaults() {
    let sb = Sandbox::new();
    let (gopath, gocache) = (sb.root().join("gopath"), sb.root().join("gocache"));
    sb.write_file(&gopath.join("pkg/mod/a"), 1);
    sb.write_file(&gocache.join("a"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("GOPATH", &gopath)
        .env("GOCACHE", &gocache)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(
        path_of(only(&scan, "go-module-cache")),
        gopath.join("pkg/mod")
    );
    assert_eq!(path_of(only(&scan, "go-build-cache")), gocache);
}

#[test]
fn a_cargo_home_holding_other_things_loses_nothing() {
    let sb = Sandbox::new();
    let custom = sb.root().join("work");
    sb.write_file(&custom.join("git/myrepo/.git/config"), 1);
    sb.write_file(&custom.join("registry/cache/a"), 1);
    sb.write_file(&custom.join("registry/my-notes"), 1);

    let out = sb
        .clch()
        .args([
            "clean",
            "cargo-registry",
            "cargo-git",
            "--full",
            "--yes",
            "--json",
        ])
        .env("CARGO_HOME", &custom)
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(result["results"][0]["status"], "skipped", "{result}");
    assert_eq!(result["results"][1]["status"], "skipped", "{result}");
    assert!(
        result["results"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("my-notes"),
        "{result}"
    );
    assert!(custom.join("git/myrepo/.git/config").exists());
    assert!(custom.join("registry/my-notes").exists());
}

#[test]
fn a_relative_location_in_the_environment_is_not_believed() {
    let sb = Sandbox::new();
    sb.write_file(&sb.root().join("git/db/a"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("CARGO_HOME", ".")
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert!(
        scan["reclaimables"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["id"] != "cargo-git"),
        "{scan}"
    );
}

#[test]
fn go_is_told_not_to_download_a_toolchain() {
    let sb = Sandbox::new();
    let (good, bad) = (sb.root().join("local"), sb.root().join("downloaded"));
    sb.write_file(&good.join("a"), 1);
    sb.write_file(&bad.join("a"), 1);
    sb.fake_tool(
        "go",
        &format!(
            "if [ \"$GOTOOLCHAIN\" = local ]; then echo '{}'; else echo '{}'; fi",
            good.display(),
            bad.display()
        ),
        &format!(
            "if \"%GOTOOLCHAIN%\"==\"local\" (echo {}) else (echo {})",
            good.display(),
            bad.display()
        ),
    );

    let scan = sb.json(&["scan"]);

    assert_eq!(path_of(only(&scan, "go-build-cache")), good);
}

#[cfg(unix)]
#[test]
fn only_the_first_folder_of_a_gopath_list_holds_the_module_cache() {
    let sb = Sandbox::new();
    let first = sb.root().join("first");
    sb.write_file(&first.join("pkg/mod/a"), 1);
    let list = format!("{}:{}", first.display(), sb.root().join("second").display());

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("GOPATH", list)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(
        path_of(only(&scan, "go-module-cache")),
        first.join("pkg/mod")
    );
}

#[test]
fn a_pip_that_answers_nothing_is_still_the_pip_that_runs() {
    let sb = Sandbox::new();
    let cache = sb.root().join("from-pip3");
    sb.write_file(&cache.join("a"), 1);
    tool(&sb, "pip", vec![]);
    tool(&sb, "pip3", vec![("cache dir", prints(&cache))]);

    let scan = sb.json(&["scan"]);

    let listed = scan["reclaimables"].as_array().unwrap();
    assert!(
        listed
            .iter()
            .all(|r| r["id"] != "pip-cache" || path_of(r) != cache),
        "{scan}"
    );
}

#[test]
fn a_cleanup_says_what_it_is_running_while_it_runs() {
    let (sb, cache) = custom("uv-custom");
    tool(
        &sb,
        "uv",
        vec![
            ("cache dir", prints(&cache)),
            ("cache clean", removes(&cache)),
        ],
    );

    let out = sb
        .clch()
        .args(["clean", "uv-cache", "--full", "--yes"])
        .output()
        .unwrap();

    assert!(String::from_utf8_lossy(&out.stderr).contains("uv cache clean"));
}

#[test]
fn a_tool_that_fails_silently_still_gets_a_reason() {
    let (sb, cache) = custom("uv-custom");
    sb.fake_tool(
        "uv",
        &format!(
            "case \"$*\" in 'cache dir') echo '{}' ;; *) exit 3 ;; esac",
            cache.display()
        ),
        &format!(
            "if \"%~1 %~2\"==\"cache dir\" (echo {}\r\nexit /b 0)\r\nexit /b 3",
            cache.display()
        ),
    );

    let out = sb
        .clch()
        .args(["clean", "uv-cache", "--full", "--yes", "--json"])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(result["results"][0]["status"], "failed");
    assert!(
        result["results"][0]["output"]
            .as_str()
            .unwrap()
            .contains('3'),
        "{result}"
    );
}

#[test]
fn a_plan_of_only_tool_caches_being_deleted_does_not_say_project_artifacts() {
    let sb = Sandbox::new();
    cargo_home(&sb);

    let out = sb
        .clch()
        .args(["clean", "cargo-registry", "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);

    assert!(text.contains("full cleanup"), "plan was:\n{text}");
    assert!(
        !text.contains("delete project artifacts"),
        "plan was:\n{text}"
    );
}
