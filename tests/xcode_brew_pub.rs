mod common;

use common::{Sandbox, blocks, prints, reclaimable, removes, tool};
use std::path::{Path, PathBuf};

fn result_of<'a>(outcome: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    outcome["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("{id} in {outcome}"))
}

// pub

fn default_pub_cache(sb: &Sandbox) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join("Pub").join("Cache")
    } else {
        sb.home().join(".pub-cache")
    }
}

#[test]
fn the_pub_cache_is_found_at_its_default_on_both_oses() {
    let sb = Sandbox::new();
    let cache = default_pub_cache(&sb);
    sb.write_file(&cache.join("hosted").join("package"), 3);

    let scan = sb.json(&["scan"]);

    let row = reclaimable(&scan, "pub-cache").expect("pub cache listed");
    assert_eq!(row["path"], cache.display().to_string());
    assert_eq!(row["size_bytes"], blocks(3));
}

#[test]
fn pub_cache_set_in_the_environment_is_where_the_cache_is() {
    let sb = Sandbox::new();
    let cache = sb.root().join("my-pub");
    sb.write_file(&cache.join("hosted").join("package"), 2);
    sb.write_file(&default_pub_cache(&sb).join("hosted").join("other"), 9);

    let output = sb
        .clch()
        .env("PUB_CACHE", &cache)
        .args(["scan", "--json"])
        .output()
        .unwrap();

    let scan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let row = reclaimable(&scan, "pub-cache").unwrap();
    assert_eq!(row["path"], cache.display().to_string());
    assert_eq!(row["size_bytes"], blocks(2));
}

#[test]
fn dart_prunes_for_an_unused_cleanup_and_clears_for_a_full_one() {
    let sb = Sandbox::new();
    let cache = default_pub_cache(&sb);
    sb.write_file(&cache.join("hosted").join("package"), 3);
    tool(
        &sb,
        "dart",
        vec![("pub cache clean --force", removes(&cache))],
    );

    sb.json(&["clean", "pub-cache", "--yes"]);
    assert_eq!(sb.calls(), ["dart pub cache gc --force"]);
    assert!(cache.exists());

    let full = sb.json(&["clean", "pub-cache", "--full", "--yes"]);
    assert_eq!(
        sb.calls(),
        ["dart pub cache gc --force", "dart pub cache clean --force"]
    );
    assert_eq!(full["freed_bytes"], blocks(3));
}

#[test]
fn without_dart_a_full_cleanup_deletes_the_cache_when_it_looks_like_one() {
    let sb = Sandbox::new();
    let cache = default_pub_cache(&sb);
    sb.write_file(&cache.join("hosted").join("package"), 3);
    sb.write_file(&cache.join("README.md"), 1);

    let unused = sb.json(&["clean", "pub-cache", "--yes"]);
    assert_eq!(result_of(&unused, "pub-cache")["status"], "skipped");
    assert!(cache.exists());

    sb.json(&["clean", "pub-cache", "--full", "--yes"]);
    assert!(!cache.exists());
}

#[test]
fn without_dart_a_folder_holding_anything_else_is_not_deleted() {
    let sb = Sandbox::new();
    let cache = sb.root().join("work");
    sb.write_file(&cache.join("hosted").join("package"), 3);
    sb.write_file(&cache.join("thesis.doc"), 1);

    let output = sb
        .clch()
        .env("PUB_CACHE", &cache)
        .args(["clean", "pub-cache", "--full", "--yes", "--json"])
        .output()
        .unwrap();

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let reason = result_of(&outcome, "pub-cache")["reason"].as_str().unwrap();
    assert!(reason.contains("thesis.doc"), "{reason}");
    assert!(cache.join("thesis.doc").exists());
}

#[test]
fn without_dart_a_folder_without_hosted_is_not_deleted() {
    let sb = Sandbox::new();
    let folder = sb.root().join("tools");
    sb.write_file(&folder.join("bin").join("deploy.sh"), 1);
    sb.write_file(&folder.join("README.md"), 1);

    let output = sb
        .clch()
        .env("PUB_CACHE", &folder)
        .args(["clean", "pub-cache", "--full", "--yes", "--json"])
        .output()
        .unwrap();

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result_of(&outcome, "pub-cache")["status"], "skipped");
    assert!(folder.join("bin").join("deploy.sh").exists());
}

// Homebrew

fn homebrew_files(sb: &Sandbox, cache: &Path) {
    sb.write_file(&cache.join("downloads").join("abc--foo-1.0.tar.gz"), 4);
    sb.write_file(&cache.join("api").join("formula.json"), 1);
    sb.write_file(&cache.join("Cask").join("bar.dmg"), 1);
    sb.write_file(&cache.join("notes.txt"), 1);
}

#[test]
fn the_homebrew_cache_is_where_brew_says_on_macos_only() {
    let sb = Sandbox::new();
    let cache = sb.root().join("brew-custom");
    homebrew_files(&sb, &cache);
    tool(&sb, "brew", vec![("--cache", prints(&cache))]);

    let scan = sb.json(&["scan"]);

    let row = reclaimable(&scan, "homebrew-cache");
    if cfg!(target_os = "macos") {
        assert_eq!(row.unwrap()["path"], cache.display().to_string());
    } else {
        assert!(row.is_none(), "{scan}");
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    fn plan_of<'a>(plan: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
        plan["plan"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .unwrap_or_else(|| panic!("{id} in {plan}"))
    }

    fn homebrew_cache(sb: &Sandbox) -> PathBuf {
        sb.home().join("Library/Caches/Homebrew")
    }

    #[test]
    fn the_homebrew_cache_has_a_default_and_a_setting() {
        let sb = Sandbox::new();
        let cache = homebrew_cache(&sb);
        homebrew_files(&sb, &cache);
        let set = sb.root().join("set");
        homebrew_files(&sb, &set);

        let scan = sb.json(&["scan"]);
        assert_eq!(
            reclaimable(&scan, "homebrew-cache").unwrap()["path"],
            cache.display().to_string()
        );

        let output = sb
            .clch()
            .env("HOMEBREW_CACHE", &set)
            .args(["scan", "--json"])
            .output()
            .unwrap();
        let scan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            reclaimable(&scan, "homebrew-cache").unwrap()["path"],
            set.display().to_string()
        );
    }

    #[test]
    fn an_unused_cleanup_goes_through_brew() {
        let sb = Sandbox::new();
        let cache = homebrew_cache(&sb);
        homebrew_files(&sb, &cache);
        tool(&sb, "brew", vec![("--cache", prints(&cache))]);

        sb.json(&["clean", "homebrew-cache", "--yes"]);

        assert!(sb.calls().contains(&"brew cleanup --scrub".to_string()));
        assert!(cache.join("downloads").exists());
    }

    #[test]
    fn a_full_cleanup_goes_through_brew_and_without_it_nothing_is_deleted() {
        let sb = Sandbox::new();
        let cache = homebrew_cache(&sb);
        homebrew_files(&sb, &cache);

        let outcome = sb.json(&["clean", "homebrew-cache", "--full", "--yes"]);
        assert_eq!(result_of(&outcome, "homebrew-cache")["status"], "skipped");
        assert!(cache.join("downloads").exists());

        tool(&sb, "brew", vec![("--cache", prints(&cache))]);
        sb.json(&["clean", "homebrew-cache", "--full", "--yes"]);
        assert!(sb.calls().contains(&"brew cleanup --prune=all".to_string()));
    }

    // Xcode

    fn developer(sb: &Sandbox, rest: &str) -> PathBuf {
        sb.home().join("Library/Developer").join(rest)
    }

    #[test]
    fn xcode_data_are_listed() {
        let sb = Sandbox::new();
        sb.write_file(&developer(&sb, "Xcode/DerivedData/App-abc/x"), 3);
        sb.write_file(&developer(&sb, "Xcode/iOS DeviceSupport/18.0/x"), 2);
        sb.write_file(&developer(&sb, "CoreSimulator/Devices/ABC/x"), 1);
        sb.write_file(&developer(&sb, "Xcode/Archives/2026/App.xcarchive/x"), 7);

        let scan = sb.json(&["scan"]);

        assert_eq!(
            reclaimable(&scan, "xcode-derived-data").unwrap()["size_bytes"],
            blocks(3)
        );
        assert_eq!(
            reclaimable(&scan, "xcode-device-support").unwrap()["size_bytes"],
            blocks(2)
        );
        assert_eq!(
            reclaimable(&scan, "xcode-simulators").unwrap()["size_bytes"],
            blocks(1)
        );
        let text = scan.to_string();
        assert!(!text.contains("Archives"), "archives are never offered");
    }

    #[test]
    fn derived_data_goes_only_in_a_full_cleanup() {
        let sb = Sandbox::new();
        let derived = developer(&sb, "Xcode/DerivedData");
        sb.write_file(&derived.join("App-abc").join("x"), 3);

        let unused = sb.json(&["clean", "xcode-derived-data", "--yes"]);
        assert_eq!(
            result_of(&unused, "xcode-derived-data")["status"],
            "skipped"
        );
        assert!(derived.exists());

        let plan = sb.json(&["clean", "xcode-derived-data", "--full"]);
        assert_eq!(
            plan_of(&plan, "xcode-derived-data")["expected_bytes"],
            blocks(3)
        );

        sb.json(&["clean", "xcode-derived-data", "--full", "--yes"]);
        assert!(!derived.exists());
    }

    #[test]
    fn a_custom_derived_data_location_is_found_and_deleted_only_under_that_name() {
        let sb = Sandbox::new();
        let custom = sb.root().join("Builds");
        sb.write_file(&custom.join("App-abc").join("x"), 3);
        tool(
            &sb,
            "defaults",
            vec![(
                "read com.apple.dt.Xcode IDECustomDerivedDataLocation",
                prints(&custom),
            )],
        );

        let scan = sb.json(&["scan"]);
        assert_eq!(
            reclaimable(&scan, "xcode-derived-data").unwrap()["path"],
            custom.display().to_string()
        );

        let outcome = sb.json(&["clean", "xcode-derived-data", "--full", "--yes"]);
        assert_eq!(
            result_of(&outcome, "xcode-derived-data")["status"],
            "skipped"
        );
        assert!(custom.exists());
    }

    #[test]
    fn a_derived_data_folder_git_tracks_is_not_deleted() {
        let sb = Sandbox::new();
        let custom = sb.root().join("repo").join("DerivedData");
        sb.write_file(&custom.join("source.swift"), 1);
        tool(
            &sb,
            "defaults",
            vec![(
                "read com.apple.dt.Xcode IDECustomDerivedDataLocation",
                prints(&custom),
            )],
        );
        tool(
            &sb,
            "git",
            vec![(
                "-c core.fsmonitor=false ls-files -- DerivedData",
                common::says("DerivedData/source.swift"),
            )],
        );

        let outcome = sb.json(&["clean", "xcode-derived-data", "--full", "--yes"]);

        assert_eq!(
            result_of(&outcome, "xcode-derived-data")["status"],
            "skipped"
        );
        assert!(custom.join("source.swift").exists());
    }

    #[test]
    fn simulators_lose_only_unavailable_devices_at_either_level() {
        let sb = Sandbox::new();
        sb.write_file(&developer(&sb, "CoreSimulator/Devices/ABC/x"), 2);
        tool(&sb, "xcrun", vec![]);

        let plan = sb.json(&["clean", "xcode-simulators", "--full"]);
        let entry = plan_of(&plan, "xcode-simulators");
        assert_eq!(entry["action"], "xcrun simctl delete unavailable");
        assert!(entry["expected_bytes"].is_null(), "{entry}");

        sb.json(&["clean", "xcode-simulators", "--yes"]);
        sb.json(&["clean", "xcode-simulators", "--full", "--yes"]);

        assert_eq!(
            sb.calls(),
            [
                "xcrun simctl delete unavailable",
                "xcrun simctl delete unavailable"
            ]
        );
    }

    #[test]
    fn device_support_has_no_way_to_be_cleaned_and_says_so() {
        let sb = Sandbox::new();
        let support = developer(&sb, "Xcode/iOS DeviceSupport");
        sb.write_file(&support.join("18.0").join("x"), 2);

        for args in [
            vec!["clean", "xcode-device-support", "--yes"],
            vec!["clean", "xcode-device-support", "--full", "--yes"],
        ] {
            let outcome = sb.json(&args);
            let row = result_of(&outcome, "xcode-device-support");
            assert_eq!(row["status"], "skipped");
            let reason = row["reason"].as_str().unwrap();
            assert!(reason.contains("docs do not"), "{reason}");
            assert!(!reason.contains("--full"), "{reason}");
        }
        assert!(support.exists());
    }
}

#[cfg(not(target_os = "macos"))]
#[test]
fn xcode_data_are_macos_only() {
    let sb = Sandbox::new();
    let dir = sb.home().join("Library/Developer/Xcode/DerivedData/App");
    sb.write_file(&dir.join("x"), 3);
    tool(
        &sb,
        "defaults",
        vec![(
            "read com.apple.dt.Xcode IDECustomDerivedDataLocation",
            prints(&dir),
        )],
    );

    let scan = sb.json(&["scan"]);

    assert!(reclaimable(&scan, "xcode-derived-data").is_none(), "{scan}");
}

fn warning_of(plan: &serde_json::Value, id: &str) -> Option<String> {
    let row = plan["plan"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)?;
    row["warning"].as_str().map(str::to_string)
}

#[test]
fn a_full_pub_cleanup_warns_about_activated_tools_and_an_unused_one_does_not() {
    let sb = Sandbox::new();
    sb.write_file(&default_pub_cache(&sb).join("hosted").join("package"), 1);
    tool(&sb, "dart", vec![]);

    let full = sb.json(&["clean", "pub-cache", "--full"]);
    assert!(
        warning_of(&full, "pub-cache")
            .unwrap()
            .contains("global activate")
    );

    let unused = sb.json(&["clean", "pub-cache"]);
    assert_eq!(warning_of(&unused, "pub-cache"), None);
}

#[cfg(target_os = "macos")]
#[test]
fn a_homebrew_cleanup_warns_about_old_formula_versions_at_either_level() {
    let sb = Sandbox::new();
    let cache = sb.home().join("Library/Caches/Homebrew");
    homebrew_files(&sb, &cache);
    tool(&sb, "brew", vec![("--cache", prints(&cache))]);

    for args in [
        vec!["clean", "homebrew-cache"],
        vec!["clean", "homebrew-cache", "--full"],
    ] {
        let plan = sb.json(&args);
        assert!(
            warning_of(&plan, "homebrew-cache")
                .unwrap()
                .contains("old versions"),
            "{plan}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn deleting_derived_data_warns_about_a_running_build() {
    let sb = Sandbox::new();
    sb.write_file(
        &sb.home().join("Library/Developer/Xcode/DerivedData/App/x"),
        1,
    );

    let plan = sb.json(&["clean", "xcode-derived-data", "--full"]);

    assert!(
        warning_of(&plan, "xcode-derived-data")
            .unwrap()
            .contains("build running now")
    );
}
