mod common;

use common::{Sandbox, blocks, prints, removes, says, tool};
use std::path::{Path, PathBuf};

fn found<'a>(scan: &'a serde_json::Value, id: &str) -> Vec<&'a serde_json::Value> {
    scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"] == id)
        .collect()
}

fn add_root(sb: &Sandbox, dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    sb.clch().args(["roots", "add"]).arg(dir).assert().success();
}

fn calls_with(sb: &Sandbox, text: &str) -> Vec<String> {
    sb.calls()
        .into_iter()
        .filter(|c| c.contains(text))
        .collect()
}

// pnpm

/// A sandbox whose pnpm answers from the home folder, and from `run_on_other`
/// with the store there, which a fake tells apart by a file only that has.
struct Stores {
    sb: Sandbox,
    home_store: PathBuf,
}

fn pnpm_at_home() -> Stores {
    let sb = Sandbox::new();
    let home_store = sb.home().join("Library/pnpm/store");
    sb.write_file(&home_store.join("v10/files/kept"), 2);
    sb.write_file(&home_store.join("v10/files/orphan"), 1);
    let versioned = home_store.join("v10");
    let orphan = home_store.join("v10/files/orphan");
    tool(
        &sb,
        "pnpm",
        vec![
            ("store path", prints(&versioned)),
            ("store prune", removes(&orphan)),
        ],
    );
    Stores { sb, home_store }
}

#[test]
fn the_pnpm_store_is_where_pnpm_says_and_counts_every_version() {
    let t = pnpm_at_home();
    t.sb.write_file(&t.home_store.join("v3/files/left-behind"), 4);

    let scan = t.sb.json(&["scan"]);

    let stores = found(&scan, "pnpm-store");
    assert_eq!(stores.len(), 1, "{scan}");
    assert_eq!(Path::new(stores[0]["path"].as_str().unwrap()), t.home_store);
    assert_eq!(stores[0]["size_bytes"], blocks(7));
    assert_eq!(stores[0]["kind"], "tool-cache");
}

#[test]
fn an_upgrade_of_pnpm_keeps_the_stores_history() {
    let t = pnpm_at_home();
    t.sb.json(&["scan"]);
    let newer = t.home_store.join("v11");
    t.sb.write_file(&newer.join("files/new"), 2);
    tool(&t.sb, "pnpm", vec![("store path", prints(&newer))]);

    let scan = t.sb.json(&["scan"]);

    let stores = found(&scan, "pnpm-store");
    assert_eq!(stores[0]["change_bytes"], blocks(2));
    assert_eq!(scan["gone"].as_array().unwrap().len(), 0);
}

#[test]
fn pnpm_never_runs_inside_a_scan_root() {
    let sb = Sandbox::new();
    let root = sb.root().join("cloned");
    sb.write_file(&root.join(".pnpmfile.cjs"), 1);
    let store = sb.home().join("Library/pnpm/store");
    sb.write_file(&store.join("v10/a"), 1);
    let visited = sb.root().join("visited.log");
    let log = visited.display();
    sb.fake_tool(
        "pnpm",
        &format!("pwd >> '{log}'; echo '{}'", store.join("v10").display()),
        &format!("cd >> \"{log}\"\r\necho {}", store.join("v10").display()),
    );
    add_root(&sb, &root);

    sb.json(&["scan"]);
    sb.clch()
        .args(["clean", "pnpm-store", "--yes"])
        .assert()
        .success();

    let folders = std::fs::read_to_string(&visited).unwrap();
    assert!(!folders.is_empty());
    assert!(!folders.contains("cloned"), "{folders}");
}

#[test]
fn unused_cleanup_prunes_the_store() {
    let t = pnpm_at_home();

    let result = t.sb.json(&["clean", "pnpm-store", "--yes"]);

    assert_eq!(calls_with(&t.sb, "store prune").len(), 1);
    assert!(!t.home_store.join("v10/files/orphan").exists());
    assert!(t.home_store.join("v10/files/kept").exists());
    assert_eq!(result["freed_bytes"], blocks(1));
}

#[test]
fn pnpm_full_cleanup_deletes_every_versions_store_and_nothing_beside_them() {
    let t = pnpm_at_home();
    t.sb.write_file(&t.home_store.join("v3/files/left-behind"), 4);
    t.sb.write_file(&t.home_store.join("notes.txt"), 1);

    let plan = t.sb.json(&["clean", "pnpm-store", "--full"]);
    assert!(
        plan["plan"][0]["action"]
            .as_str()
            .unwrap()
            .contains("delete"),
        "{plan}"
    );

    let result = t.sb.json(&["clean", "pnpm-store", "--full", "--yes"]);
    assert_eq!(result["results"][0]["status"], "cleaned", "{result}");
    assert_eq!(result["freed_bytes"], blocks(7));
    assert!(!t.home_store.join("v10").exists());
    assert!(!t.home_store.join("v3").exists());
    assert!(t.home_store.join("notes.txt").exists());
    assert_eq!(calls_with(&t.sb, "store prune").len(), 0);
}

#[test]
fn a_store_dir_pointing_at_a_users_folder_loses_nothing() {
    let sb = Sandbox::new();
    let docs = sb.home().join("docs");
    sb.write_file(&docs.join("precious"), 2);
    sb.write_file(&docs.join("v2/thesis"), 2);
    tool(&sb, "pnpm", vec![("store path", prints(&docs.join("v10")))]);

    let plan = sb.json(&["clean", "pnpm-store", "--full"]);
    assert_eq!(plan["plan"][0]["cleanable"], false, "{plan}");

    let result = sb.json(&["clean", "pnpm-store", "--full", "--yes"]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(docs.join("precious").exists());
    assert!(docs.join("v2/thesis").exists());
}

#[test]
fn a_tool_answer_naming_the_home_folder_cannot_delete_it() {
    let sb = Sandbox::new();
    sb.write_file(&sb.home().join("v10/files/a"), 1);
    sb.write_file(&sb.home().join("keep"), 1);
    tool(
        &sb,
        "pnpm",
        vec![("store path", prints(&sb.home().join("v10")))],
    );

    let result = sb.json(&["clean", "pnpm-store", "--full", "--yes"]);

    assert_eq!(result["results"][0]["status"], "skipped", "{result}");
    assert!(sb.home().join("v10/files/a").exists());
    assert!(sb.home().join("keep").exists());
}

#[test]
fn an_empty_cache_stays_out_of_a_plain_plan_but_not_when_named() {
    let t = pnpm_at_home();
    std::fs::remove_dir_all(t.home_store.join("v10")).unwrap();
    std::fs::create_dir_all(t.home_store.join("v10")).unwrap();

    let plain = t.sb.json(&["clean"]);
    let named = t.sb.json(&["clean", "pnpm-store"]);

    assert_eq!(plain["plan"].as_array().unwrap().len(), 0, "{plain}");
    assert_eq!(named["plan"].as_array().unwrap().len(), 1, "{named}");
}

#[test]
fn a_pnpm_store_inside_a_scan_root_is_not_searched_for_projects() {
    let sb = Sandbox::new();
    let root = sb.root().join("code");
    let store = root.join("pnpm-store/v10");
    sb.node_project(&store.join("pkg"), 1);
    tool(&sb, "pnpm", vec![("store path", prints(&store))]);
    add_root(&sb, &root);

    let scan = sb.json(&["scan"]);

    assert_eq!(found(&scan, "node_modules").len(), 0, "{scan}");
    assert_eq!(found(&scan, "pnpm-store").len(), 1);
}

fn pnpm_default(sb: &Sandbox) -> PathBuf {
    if cfg!(windows) {
        sb.home().join("AppData/Local/pnpm/store")
    } else {
        sb.home().join("Library/pnpm/store")
    }
}

#[test]
fn without_pnpm_the_default_store_is_found_and_not_cleaned() {
    let sb = Sandbox::new();
    sb.write_file(&pnpm_default(&sb).join("v10/files/a"), 2);

    let plan = sb.json(&["clean", "pnpm-store"]);
    let scan = sb.json(&["scan"]);

    assert_eq!(found(&scan, "pnpm-store")[0]["size_bytes"], blocks(2));
    assert_eq!(plan["plan"][0]["cleanable"], false);
    assert!(
        plan["plan"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("--full"),
        "{plan}"
    );

    let full = sb.json(&["clean", "pnpm-store", "--full", "--yes"]);
    assert_eq!(full["results"][0]["status"], "cleaned");
    assert!(!pnpm_default(&sb).join("v10").exists());
}

#[test]
fn a_tool_answer_that_is_no_absolute_path_is_not_believed() {
    let sb = Sandbox::new();
    sb.write_file(&pnpm_default(&sb).join("v10/files/a"), 2);
    tool(&sb, "pnpm", vec![("store path", says("undefined"))]);
    std::fs::create_dir_all(sb.home().join("undefined")).unwrap();

    let scan = sb.json(&["scan"]);

    let stores = found(&scan, "pnpm-store");
    assert_eq!(stores.len(), 1, "{scan}");
    assert_eq!(
        Path::new(stores[0]["path"].as_str().unwrap()),
        pnpm_default(&sb)
    );
}

#[test]
fn pnpm_home_sets_the_default_store() {
    let sb = Sandbox::new();
    let home = sb.root().join("custom-pnpm");
    sb.write_file(&home.join("store/v10/files/a"), 2);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("PNPM_HOME", &home)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let stores = found(&scan, "pnpm-store");
    assert_eq!(
        Path::new(stores[0]["path"].as_str().unwrap()),
        home.join("store")
    );
}

#[cfg(unix)]
#[test]
fn xdg_folders_set_the_default_locations_as_the_tools_themselves_do() {
    let sb = Sandbox::new();
    let (data, cache) = (sb.root().join("xdg-data"), sb.root().join("xdg-cache"));
    sb.write_file(&data.join("pnpm/store/v10/a"), 1);
    sb.write_file(&data.join("yarn/berry/cache/a"), 1);
    sb.write_file(&cache.join("yarn/v6/a"), 1);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let at = |id: &str| PathBuf::from(found(&scan, id)[0]["path"].as_str().unwrap());
    assert_eq!(at("pnpm-store"), data.join("pnpm/store"));
    assert_eq!(at("yarn-berry-cache"), data.join("yarn/berry/cache"));
    assert_eq!(at("yarn-cache"), cache.join("yarn/v6"));
}

/// A real second disk, mounted for the length of a test.
#[cfg(target_os = "macos")]
struct OtherDisk {
    mount: PathBuf,
}

#[cfg(target_os = "macos")]
impl OtherDisk {
    fn new(sb: &Sandbox) -> OtherDisk {
        let image = sb.root().join("other.dmg");
        let mount = sb.root().join("other-disk");
        std::fs::create_dir_all(&mount).unwrap();
        let run = |args: &[&str]| {
            let done = std::process::Command::new("/usr/bin/hdiutil")
                .args(args)
                .output()
                .unwrap();
            assert!(
                done.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&done.stderr)
            );
        };
        run(&[
            "create",
            "-quiet",
            "-size",
            "20m",
            "-fs",
            "HFS+",
            "-volname",
            "clchstore",
            image.to_str().unwrap(),
        ]);
        run(&[
            "attach",
            "-quiet",
            "-nobrowse",
            "-mountpoint",
            mount.to_str().unwrap(),
            image.to_str().unwrap(),
        ]);
        OtherDisk { mount }
    }
}

#[cfg(target_os = "macos")]
impl Drop for OtherDisk {
    fn drop(&mut self) {
        let _ = std::process::Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount)
            .output();
    }
}

/// A home store, and a store near the root of another disk, with a pnpm that
/// answers for the disk it runs on.
#[cfg(target_os = "macos")]
fn two_disks() -> (Stores, OtherDisk, PathBuf) {
    let sb = Sandbox::new();
    let home_store = sb.home().join("Library/pnpm/store");
    sb.write_file(&home_store.join("v10/files/kept"), 2);
    sb.write_file(&home_store.join("v10/files/orphan"), 1);
    let disk = OtherDisk::new(&sb);
    let other_store = disk.mount.canonicalize().unwrap().join(".pnpm-store");
    sb.write_file(&other_store.join("v10/files/kept"), 3);
    sb.write_file(&other_store.join("v10/files/orphan"), 4);
    sb.write_file(&other_store.join("on-other-disk"), 1);
    sb.fake_tool(
        "pnpm",
        &format!(
            "if [ -f on-other-disk ]; then S='{}'; else S='{}'; fi
case \"$*\" in
  'store path') echo \"$S/v10\" ;;
  'store prune') rm -f \"$S/v10/files/orphan\" ;;
esac",
            other_store.display(),
            home_store.display()
        ),
        "",
    );
    std::fs::create_dir_all(disk.mount.join("code")).unwrap();
    (Stores { sb, home_store }, disk, other_store)
}

#[cfg(target_os = "macos")]
#[test]
fn a_scan_root_on_another_disk_brings_in_that_disks_store() {
    let (t, disk, other_store) = two_disks();

    let before = t.sb.json(&["scan"]);
    assert_eq!(
        found(&before, "pnpm-store").len(),
        1,
        "no root reaches it yet"
    );

    add_root(&t.sb, &disk.mount.join("code"));
    let scan = t.sb.json(&["scan"]);

    let stores = found(&scan, "pnpm-store");
    assert_eq!(stores.len(), 2, "{scan}");
    let other = stores
        .iter()
        .find(|s| Path::new(s["path"].as_str().unwrap()) == other_store)
        .expect("the other disk's store");
    assert_eq!(
        other["disk"],
        disk.mount.canonicalize().unwrap().to_str().unwrap()
    );
}

#[cfg(target_os = "macos")]
#[test]
fn each_store_is_pruned_from_its_own_disk_and_keeps_its_own_history() {
    let (t, disk, other_store) = two_disks();
    add_root(&t.sb, &disk.mount.join("code"));
    t.sb.json(&["scan"]);
    t.sb.write_file(&other_store.join("v10/files/more"), 2);

    let scan = t.sb.json(&["scan"]);
    let other = found(&scan, "pnpm-store")
        .into_iter()
        .find(|s| Path::new(s["path"].as_str().unwrap()) == other_store)
        .unwrap()
        .clone();
    assert_eq!(other["change_bytes"], blocks(2));

    t.sb.clch()
        .args(["clean", "pnpm-store", "--yes"])
        .assert()
        .success();

    assert_eq!(
        calls_with(&t.sb, "store prune").len(),
        2,
        "{:?}",
        t.sb.calls()
    );
    assert!(!t.home_store.join("v10/files/orphan").exists());
    assert!(!other_store.join("v10/files/orphan").exists());
    assert!(other_store.join("v10/files/kept").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn naming_a_stores_path_prunes_only_that_store() {
    let (t, disk, other_store) = two_disks();
    add_root(&t.sb, &disk.mount.join("code"));

    t.sb.clch()
        .arg("clean")
        .arg(&other_store)
        .arg("--yes")
        .assert()
        .success();

    assert!(!other_store.join("v10/files/orphan").exists());
    assert!(t.home_store.join("v10/files/orphan").exists());
}

// yarn

fn classic_default(sb: &Sandbox) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join("Yarn/Cache/v6")
    } else {
        sb.home().join("Library/Caches/Yarn/v6")
    }
}

fn berry_default(sb: &Sandbox) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join("Yarn/Berry/cache")
    } else {
        sb.home().join(".yarn/berry/cache")
    }
}

#[test]
fn yarn_classic_says_where_its_cache_is_and_cleans_it_only_in_full() {
    let sb = Sandbox::new();
    let cache = sb.root().join("yarn-custom");
    sb.write_file(&cache.join("pkg"), 2);
    tool(
        &sb,
        "yarn",
        vec![
            ("--version", says("1.22.22")),
            ("cache dir", prints(&cache)),
            ("cache clean", removes(&cache)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(
        Path::new(found(&scan, "yarn-cache")[0]["path"].as_str().unwrap()),
        cache
    );

    let unused = sb.json(&["clean", "yarn-cache", "--yes"]);
    assert_eq!(unused["results"][0]["status"], "skipped");
    assert!(cache.exists());

    let full = sb.json(&["clean", "yarn-cache", "--full", "--yes"]);
    assert_eq!(full["freed_bytes"], blocks(2));
    assert!(!cache.exists());
}

#[test]
fn yarn_berry_says_where_its_cache_is_and_cleans_it_with_all() {
    let sb = Sandbox::new();
    let cache = sb.root().join("berry-custom");
    sb.write_file(&cache.join("pkg-8.zip"), 3);
    tool(
        &sb,
        "yarn",
        vec![
            ("--version", says("4.18.1")),
            ("config get cacheFolder", prints(&cache)),
            ("cache clean --all", removes(&cache)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(
        Path::new(
            found(&scan, "yarn-berry-cache")[0]["path"]
                .as_str()
                .unwrap()
        ),
        cache
    );

    let full = sb.json(&["clean", "yarn-berry-cache", "--full", "--yes"]);
    assert_eq!(full["freed_bytes"], blocks(3));
    assert_eq!(calls_with(&sb, "cache clean").len(), 1);
    assert_eq!(calls_with(&sb, "cache clean --all").len(), 1);
}

#[test]
fn a_cache_left_by_the_other_yarn_is_found_and_not_cleaned_with_the_wrong_one() {
    let sb = Sandbox::new();
    sb.write_file(&classic_default(&sb).join("pkg"), 2);
    tool(
        &sb,
        "yarn",
        vec![
            ("--version", says("4.18.1")),
            ("config get cacheFolder", prints(&berry_default(&sb))),
            ("cache clean --all", removes(&berry_default(&sb))),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert_eq!(found(&scan, "yarn-cache").len(), 1, "{scan}");

    let result = sb.json(&["clean", "yarn-cache", "--full", "--yes"]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert!(
        result["results"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("older than 2.0.0"),
        "{result}"
    );
    assert_eq!(calls_with(&sb, "cache clean").len(), 0);
    assert!(classic_default(&sb).join("pkg").exists());
}

#[test]
fn without_yarn_both_default_caches_are_found_and_not_cleaned() {
    let sb = Sandbox::new();
    sb.write_file(&classic_default(&sb).join("pkg"), 2);
    sb.write_file(&berry_default(&sb).join("pkg-8.zip"), 3);

    let scan = sb.json(&["scan"]);
    assert_eq!(found(&scan, "yarn-cache")[0]["size_bytes"], blocks(2));
    assert_eq!(found(&scan, "yarn-berry-cache")[0]["size_bytes"], blocks(3));

    let result = sb.json(&["clean", "yarn-cache", "yarn-berry-cache", "--full", "--yes"]);
    assert_eq!(result["results"][0]["status"], "skipped");
    assert_eq!(result["results"][1]["status"], "skipped");
    assert!(classic_default(&sb).exists());
    assert!(berry_default(&sb).exists());
}

#[test]
fn berry_answering_undefined_falls_back_to_its_default() {
    let sb = Sandbox::new();
    sb.write_file(&berry_default(&sb).join("a-8.zip"), 1);
    tool(
        &sb,
        "yarn",
        vec![
            ("--version", says("4.18.1")),
            ("config get cacheFolder", says("undefined")),
        ],
    );

    let scan = sb.json(&["scan"]);

    let at = found(&scan, "yarn-berry-cache");
    assert_eq!(
        Path::new(at[0]["path"].as_str().unwrap()),
        berry_default(&sb)
    );
}

// corepack

fn fake_recording_network_setting(sb: &Sandbox, tool: &str) -> PathBuf {
    let seen = sb.root().join(format!("{tool}-saw"));
    sb.fake_tool(
        tool,
        &format!("echo \"$COREPACK_ENABLE_NETWORK\" >> '{0}'", seen.display()),
        &format!(">>\"{0}\" echo %COREPACK_ENABLE_NETWORK%", seen.display()),
    );
    seen
}

#[test]
fn a_scan_tells_corepack_shims_not_to_use_the_network() {
    let sb = Sandbox::new();
    for tool in ["pnpm", "yarn", "npm"] {
        let seen = fake_recording_network_setting(&sb, tool);
        sb.json(&["scan"]);
        let told = std::fs::read_to_string(&seen).unwrap();
        assert!(!told.trim().is_empty(), "{tool} was never asked");
        assert!(told.lines().all(|l| l.trim() == "0"), "{tool}: {told}");
    }
}

#[test]
fn a_shim_that_cannot_answer_leaves_a_clean_scan() {
    let sb = Sandbox::new();
    sb.fake_tool(
        "pnpm",
        "echo \"Network access disabled by the environment\" >&2; exit 1",
        "echo Network access disabled by the environment 1>&2\r\nexit /b 1",
    );

    let out = sb.clch().args(["scan", "--json"]).output().unwrap();

    assert!(out.status.success());
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(scan["reclaimables"].is_array());
}
