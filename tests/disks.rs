mod common;

use common::{BLOCK, Sandbox, blocks, reclaimable};
use std::path::Path;

/// The disk `path` is on, and its free space, as the operating system's own
/// tools report them.
fn os_disk(path: &Path) -> (String, Option<i64>) {
    if cfg!(windows) {
        (path.display().to_string()[..3].to_string(), None)
    } else {
        let out = std::process::Command::new("/bin/df")
            .arg("-kP")
            .arg(path)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let last = text.lines().last().unwrap().to_string();
        let fields: Vec<&str> = last.split_whitespace().collect();
        let free = fields[3].parse::<i64>().unwrap() * 1024;
        (fields[5..].join(" "), Some(free))
    }
}

fn disk_entry<'a>(scan: &'a serde_json::Value, disk: &str) -> &'a serde_json::Value {
    scan["disks"]
        .as_array()
        .expect("disks is an array")
        .iter()
        .find(|d| d["disk"] == disk)
        .unwrap_or_else(|| panic!("{disk} missing from {}", scan["disks"]))
}

#[test]
fn each_reclaimable_shows_the_disk_it_lives_on() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("a"), 2);
    let (disk, _) = os_disk(&cache);

    let scan = sb.json(&["scan"]);
    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["disk"],
        disk.as_str()
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
    assert!(text.contains(&disk), "table was:\n{text}");
}

#[test]
fn each_disk_shows_its_free_space_and_reclaimable_total() {
    let sb = Sandbox::new();
    let root = sb.default_npm_cache();
    sb.write_file(&root.join("_cacache/a"), 2);
    sb.write_file(&root.join("_npx/k/a"), 3);
    let (disk, free) = os_disk(&root);

    let scan = sb.json(&["scan"]);

    let entry = disk_entry(&scan, &disk);
    assert_eq!(entry["reclaimable_bytes"], blocks(5));
    let reported = entry["free_bytes"].as_i64().unwrap();
    assert!(reported > 0, "{entry}");
    if let Some(free) = free {
        let margin = 1024 * 1024 * 1024;
        assert!((reported - free).abs() < margin, "{reported} vs df {free}");
    }

    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("RECLAIMABLE"), "table was:\n{text}");
    assert!(text.contains("FREE"), "table was:\n{text}");
}

#[cfg(unix)]
#[test]
fn a_sparse_file_counts_what_it_occupies_not_its_length() {
    use std::os::unix::fs::MetadataExt;

    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    std::fs::create_dir_all(&cache).unwrap();
    let file = std::fs::File::create(cache.join("sparse")).unwrap();
    file.set_len(100 * 1024 * 1024).unwrap();
    drop(file);
    let occupied = std::fs::metadata(cache.join("sparse")).unwrap().blocks() as i64 * 512;
    assert!(
        occupied < 1024 * 1024,
        "the fixture is not sparse here: {occupied}"
    );

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        occupied
    );
}

#[test]
fn a_small_file_counts_a_whole_block() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("small"), vec![b'x'; BLOCK + 1]).unwrap();

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        blocks(2)
    );
}

#[test]
fn a_file_hard_linked_from_two_reclaimables_is_counted_once_in_the_total() {
    let sb = Sandbox::new();
    let root = sb.default_npm_cache();
    sb.write_file(&root.join("_cacache/shared"), 4);
    sb.write_file(&root.join("_cacache/own"), 1);
    std::fs::create_dir_all(root.join("_npx/k")).unwrap();
    std::fs::hard_link(root.join("_cacache/shared"), root.join("_npx/k/shared")).unwrap();
    let (disk, _) = os_disk(&root);

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        blocks(5)
    );
    assert_eq!(
        reclaimable(&scan, "npx-cache").unwrap()["size_bytes"],
        blocks(4)
    );
    assert_eq!(disk_entry(&scan, &disk)["reclaimable_bytes"], blocks(5));
}

#[test]
fn a_file_hard_linked_twice_inside_a_reclaimable_is_counted_once() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("one"), 3);
    std::fs::hard_link(cache.join("one"), cache.join("two")).unwrap();

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        blocks(3)
    );
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_inside_a_reclaimable_is_not_followed() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    let elsewhere = sb.root().join("elsewhere");
    sb.write_file(&cache.join("own"), 1);
    sb.write_file(&elsewhere.join("big"), 50);
    std::os::unix::fs::symlink(&elsewhere, cache.join("to-dir")).unwrap();
    std::os::unix::fs::symlink(elsewhere.join("big"), cache.join("to-file")).unwrap();

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        blocks(1)
    );
}

#[cfg(unix)]
#[test]
fn a_folder_that_cannot_be_read_makes_the_size_a_lower_bound_not_a_change() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    let locked = cache.join("locked");
    sb.write_file(&cache.join("own"), 1);
    sb.write_file(&locked.join("a"), 2);
    sb.json(&["scan"]);

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let during = sb.json(&["scan"]);
    let table = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let after = sb.json(&["scan"]);

    let total = &during["disks"][0];
    assert_eq!(total["complete"], false, "{total}");
    let text = String::from_utf8_lossy(&table);
    let disk_rows = text.split("RECLAIMABLE").nth(1).unwrap();
    assert!(disk_rows.contains("at least"), "table was:\n{text}");

    let npm = reclaimable(&during, "npm-cache").unwrap();
    assert_eq!(npm["complete"], false);
    assert_eq!(npm["size_bytes"], blocks(1));
    assert_eq!(npm["change_state"], "unknown");
    assert!(npm["change_bytes"].is_null(), "{npm}");
    let text = String::from_utf8_lossy(&table);
    assert!(text.contains("at least"), "table was:\n{text}");

    let npm = reclaimable(&after, "npm-cache").unwrap();
    assert_eq!(npm["complete"], true);
    assert_eq!(npm["change_state"], "unknown");
    assert!(npm["change_bytes"].is_null(), "{npm}");
}

#[test]
fn a_cleanup_reports_what_it_freed_on_each_disk() {
    let sb = Sandbox::new();
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/a"), 3);
    let dir = root.join("_cacache");
    sb.fake_tool(
        "npm",
        &format!(
            "case \"$*\" in 'config get cache') echo '{}' ;; 'cache clean --force') rm -rf '{}' ;; esac",
            root.display(),
            dir.display()
        ),
        &format!(
            "set \"ARGS=%*\"\r\nif \"%ARGS%\"==\"config get cache\" goto config\r\nif \"%ARGS%\"==\"cache clean --force\" goto clean\r\nexit /b 0\r\n:config\r\necho {}\r\nexit /b 0\r\n:clean\r\nrmdir /s /q \"{}\"\r\nexit /b 0",
            root.display(),
            dir.display()
        ),
    );
    let (disk, _) = os_disk(&root);

    let result = sb.json(&["clean", "npm-cache", "--full", "--yes"]);

    let entry = disk_entry(&result, &disk);
    assert_eq!(entry["freed_bytes"], blocks(3));
}

/// A fake npm whose `cache clean --force` removes `_cacache` under `root`
/// and whose `cache npx rm --force` removes `_npx`.
fn npm_that_cleans(sb: &Sandbox, root: &Path) {
    let (cache, npx) = (root.join("_cacache"), root.join("_npx"));
    sb.fake_tool(
        "npm",
        &format!(
            "case \"$*\" in 'config get cache') echo '{}' ;; '--version') echo 11.6.2 ;; 'cache clean --force') rm -rf '{}' ;; 'cache npx rm --force') rm -rf '{}' ;; esac",
            root.display(),
            cache.display(),
            npx.display()
        ),
        &format!(
            "set \"ARGS=%*\"\r\nif \"%ARGS%\"==\"config get cache\" goto config\r\nif \"%ARGS%\"==\"--version\" goto version\r\nif \"%ARGS%\"==\"cache clean --force\" goto clean\r\nif \"%ARGS%\"==\"cache npx rm --force\" goto npx\r\nexit /b 0\r\n:config\r\necho {}\r\nexit /b 0\r\n:version\r\necho 11.6.2\r\nexit /b 0\r\n:clean\r\nrmdir /s /q \"{}\"\r\nexit /b 0\r\n:npx\r\nrmdir /s /q \"{}\"\r\nexit /b 0",
            root.display(),
            cache.display(),
            npx.display()
        ),
    );
}

fn shared_file_machine() -> (Sandbox, std::path::PathBuf) {
    let sb = Sandbox::new();
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/shared"), 4);
    std::fs::create_dir_all(root.join("_npx/k")).unwrap();
    std::fs::hard_link(root.join("_cacache/shared"), root.join("_npx/k/shared")).unwrap();
    npm_that_cleans(&sb, &root);
    (sb, root)
}

#[test]
fn cleaning_two_reclaimables_that_share_a_file_frees_it_once() {
    let (sb, root) = shared_file_machine();
    let (disk, _) = os_disk(&root);

    let result = sb.json(&["clean", "--full", "--yes"]);

    assert_eq!(disk_entry(&result, &disk)["freed_bytes"], blocks(4));
    assert_eq!(result["freed_bytes"], blocks(4));
}

#[test]
fn a_file_still_held_by_another_reclaimable_is_not_freed() {
    let (sb, root) = shared_file_machine();
    let (disk, _) = os_disk(&root);

    let result = sb.json(&["clean", "npm-cache", "--full", "--yes"]);

    assert_eq!(disk_entry(&result, &disk)["freed_bytes"], 0);
    assert_eq!(result["freed_bytes"], 0);
    assert!(root.join("_npx/k/shared").exists());
}

#[cfg(unix)]
#[test]
fn a_cleanup_of_something_partly_unreadable_says_its_figures_are_not_exact() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/own"), 1);
    sb.write_file(&root.join("_cacache/locked/a"), 2);
    npm_that_cleans(&sb, &root);
    let locked = root.join("_cacache/locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();

    let plan = sb.json(&["clean", "npm-cache", "--full"]);
    let table = sb
        .clch()
        .args(["clean", "npm-cache", "--full"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = sb
        .clch()
        .args(["clean", "npm-cache", "--full", "--yes", "--json"])
        .output()
        .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    if locked.exists() {
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    assert_eq!(plan["plan"][0]["complete"], false);
    let text = String::from_utf8_lossy(&table);
    assert!(text.contains("at least"), "plan was:\n{text}");
    assert_eq!(result["results"][0]["complete"], false);
    assert_eq!(result["disks"][0]["complete"], false);
    assert_eq!(result["complete"], false);
}

#[cfg(unix)]
#[test]
fn what_was_gone_after_a_lower_bound_says_so() {
    use std::os::unix::fs::PermissionsExt;

    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("own"), 1);
    sb.write_file(&cache.join("locked/a"), 2);
    std::fs::set_permissions(cache.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
    sb.json(&["scan"]);
    std::fs::set_permissions(cache.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::remove_dir_all(&cache).unwrap();

    let table = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8_lossy(&table);
    assert!(text.contains("at least -"), "table was:\n{text}");
}

#[cfg(target_os = "macos")]
#[test]
fn another_disk_mounted_inside_a_reclaimable_is_not_counted() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("own"), 2);
    let image = sb.root().join("other.dmg");
    let mount = cache.join("mnt");
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
    std::fs::write(mount.join("big"), vec![b'x'; 2 * 1024 * 1024]).unwrap();

    let scan = sb.json(&["scan"]);
    hdiutil(&["detach", "-quiet", mount.to_str().unwrap()]);

    let npm = reclaimable(&scan, "npm-cache").unwrap();
    assert_eq!(npm["size_bytes"], blocks(2));
    assert_eq!(npm["complete"], true);
}

#[cfg(windows)]
#[test]
fn a_junction_inside_a_reclaimable_is_not_followed() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    let elsewhere = sb.root().join("elsewhere");
    sb.write_file(&cache.join("own"), 1);
    sb.write_file(&elsewhere.join("big"), 50);
    let junction = cache.join("junction");
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&elsewhere)
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    assert!(elsewhere.join("big").exists());

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["size_bytes"],
        blocks(1)
    );
}
