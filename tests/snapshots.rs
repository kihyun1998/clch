mod common;

use common::{Sandbox, blocks, reclaimable};

fn npm_cache_with(sb: &Sandbox, bytes: usize) {
    let cache = sb.default_npm_cache().join("_cacache");
    let _ = std::fs::remove_dir_all(&cache);
    sb.write_file(&cache.join("content-v2/a"), bytes);
}

fn snapshot_count(sb: &Sandbox) -> i64 {
    let db =
        rusqlite::Connection::open(sb.data_dir().join("clch.db")).expect("open snapshot store");
    db.query_row("SELECT count(*) FROM snapshot", [], |r| r.get(0))
        .expect("count snapshots")
}

#[test]
fn the_first_scan_has_nothing_to_compare_with() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 1000);

    let scan = sb.json(&["scan"]);

    let npm = reclaimable(&scan, "npm-cache").unwrap();
    assert_eq!(
        npm.get("change_bytes"),
        Some(&serde_json::Value::Null),
        "{npm}"
    );
}

#[test]
fn a_scan_shows_the_change_since_the_previous_snapshot() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 1000);
    sb.json(&["scan"]);

    npm_cache_with(&sb, 1500);
    let grown = sb.json(&["scan"]);
    npm_cache_with(&sb, 400);
    let shrunk = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&grown, "npm-cache").unwrap()["change_bytes"],
        blocks(500)
    );
    assert_eq!(
        reclaimable(&shrunk, "npm-cache").unwrap()["change_bytes"],
        blocks(-1100)
    );
}

#[test]
fn the_table_shows_the_change() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 1000);
    sb.clch().arg("scan").assert().success();
    npm_cache_with(&sb, 3000);

    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);

    assert!(text.contains("+8.2 MB"), "table was:\n{text}");
}

#[test]
fn snapshots_live_in_the_application_data_folder() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);

    sb.json(&["scan"]);

    assert!(
        sb.data_dir().join("clch.db").is_file(),
        "no store under {}",
        sb.data_dir().display()
    );
}

#[test]
fn a_cleanup_records_a_snapshot_after_it_finishes() {
    let sb = Sandbox::new();
    let root = sb.root().join("npm-root");
    sb.write_file(&root.join("_cacache/content-v2/kept"), 1000);
    sb.write_file(&root.join("_cacache/content-v2/sha512/00/orphan"), 200);
    let orphan = root.join("_cacache/content-v2/sha512/00/orphan");
    sb.fake_tool(
        "npm",
        &format!(
            "case \"$*\" in 'config get cache') echo '{}' ;; 'cache verify') rm -f '{}' ;; esac",
            root.display(),
            orphan.display()
        ),
        &format!(
            "set \"ARGS=%*\"\r\nif \"%ARGS%\"==\"config get cache\" goto config\r\nif \"%ARGS%\"==\"cache verify\" goto verify\r\nexit /b 0\r\n:config\r\necho {}\r\nexit /b 0\r\n:verify\r\ndel /q \"{}\"\r\nexit /b 0",
            root.display(),
            orphan.display().to_string().replace('/', "\\")
        ),
    );
    sb.json(&["scan"]);

    sb.clch()
        .args(["clean", "npm-cache", "--yes"])
        .assert()
        .success();
    let after = sb.json(&["scan"]);

    assert_eq!(snapshot_count(&sb), 3);
    assert_eq!(
        reclaimable(&after, "npm-cache").unwrap()["change_bytes"],
        blocks(0)
    );
}

#[test]
fn a_preview_records_nothing() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);
    sb.json(&["scan"]);

    sb.clch().arg("clean").assert().success();

    assert_eq!(snapshot_count(&sb), 1);
}

#[test]
fn every_snapshot_is_kept() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);

    for _ in 0..5 {
        sb.json(&["scan"]);
    }

    assert_eq!(snapshot_count(&sb), 5);
}

#[test]
fn scans_at_the_same_time_keep_the_history_whole() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);
    sb.json(&["scan"]);

    run_together(&sb, 8);

    assert_eq!(snapshot_count(&sb), 9);
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    let check: String = db
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
}

fn run_together(sb: &Sandbox, count: usize) {
    let children: Vec<_> = (0..count)
        .map(|_| {
            let mut cmd = std::process::Command::new(assert_cmd::cargo::cargo_bin("clch"));
            cmd.args(["scan", "--json"])
                .env_clear()
                .env("PATH", sb.bin())
                .env("HOME", sb.home())
                .env("USERPROFILE", sb.home())
                .env("LOCALAPPDATA", sb.local_app_data())
                .env("APPDATA", sb.app_data())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped());
            for keep in [
                "SystemRoot",
                "SYSTEMROOT",
                "ComSpec",
                "PATHEXT",
                "TEMP",
                "TMP",
                "TMPDIR",
            ] {
                if let Ok(v) = std::env::var(keep) {
                    cmd.env(keep, v);
                }
            }
            cmd.spawn().expect("start clch")
        })
        .collect();
    for child in children {
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn scans_racing_to_create_the_store_all_record_a_snapshot() {
    for _ in 0..25 {
        let sb = Sandbox::new();
        npm_cache_with(&sb, 10);
        run_together(&sb, 2);
        assert_eq!(snapshot_count(&sb), 2);
    }
    for _ in 0..5 {
        let sb = Sandbox::new();
        npm_cache_with(&sb, 10);
        run_together(&sb, 8);
        assert_eq!(snapshot_count(&sb), 8);
    }
}

#[test]
fn a_scan_with_history_says_so() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);

    assert_eq!(sb.json(&["scan"])["history_available"], true);
}

#[test]
fn an_unusable_store_is_not_reported_as_new() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);
    std::fs::create_dir_all(sb.data_dir().join("clch.db")).unwrap();

    let out = sb.clch().args(["scan", "--json"]).output().unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning"));
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(scan["history_available"], false);

    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("unknown"), "table was:\n{text}");
    assert!(!text.contains("new"), "table was:\n{text}");
}

#[test]
fn a_store_from_a_newer_clch_is_left_alone() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);
    sb.json(&["scan"]);
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    db.pragma_update(None, "user_version", 9999).unwrap();
    drop(db);

    let out = sb.clch().args(["scan", "--json"]).output().unwrap();

    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("newer"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(scan["history_available"], false);
    assert_eq!(snapshot_count(&sb), 1);
}

#[test]
fn a_reclaimable_gone_since_the_last_snapshot_is_reported_once() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 5000);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(sb.default_npm_cache().join("_cacache")).unwrap();

    let out = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("npm cache"), "table was:\n{text}");
    assert!(text.contains("-20.5 MB"), "table was:\n{text}");

    let later = sb.json(&["scan"]);
    assert_eq!(later["gone"].as_array().unwrap().len(), 0);
}

#[test]
fn the_json_lists_what_is_gone_with_its_last_size() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 5000);
    sb.json(&["scan"]);
    std::fs::remove_dir_all(sb.default_npm_cache().join("_cacache")).unwrap();

    let scan = sb.json(&["scan"]);

    let gone = scan["gone"].as_array().unwrap();
    assert_eq!(gone.len(), 1);
    assert_eq!(gone[0]["id"], "npm-cache");
    assert_eq!(gone[0]["previous_bytes"], blocks(5000));
}

#[test]
fn a_tool_cache_that_moved_keeps_its_history() {
    let sb = Sandbox::new();
    let first = sb.root().join("first");
    let second = sb.root().join("second");
    sb.write_file(&first.join("_cacache/a"), 1000);
    sb.write_file(&second.join("_cacache/a"), 1400);
    sb.fake_npm_reporting_cache(&first);
    sb.json(&["scan"]);

    sb.fake_npm_reporting_cache(&second);
    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").unwrap()["change_bytes"],
        blocks(400)
    );
    assert_eq!(scan["gone"].as_array().unwrap().len(), 0);
}

#[cfg(windows)]
#[test]
fn without_localappdata_snapshots_go_to_its_default_location() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 10);

    sb.clch()
        .args(["scan", "--json"])
        .env_remove("LOCALAPPDATA")
        .assert()
        .success();

    assert!(
        sb.home()
            .join("AppData")
            .join("Local")
            .join("clch")
            .join("clch.db")
            .is_file()
    );
}

#[test]
fn a_store_from_before_disks_were_recorded_is_upgraded_and_compared() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 3);
    std::fs::create_dir_all(sb.data_dir()).unwrap();
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE snapshot (id INTEGER PRIMARY KEY AUTOINCREMENT, taken_at_ms INTEGER NOT NULL);
         CREATE TABLE reclaimable_size (
             snapshot_id INTEGER NOT NULL REFERENCES snapshot(id),
             reclaimable_id TEXT NOT NULL,
             kind TEXT NOT NULL,
             path TEXT NOT NULL,
             size_bytes INTEGER NOT NULL,
             PRIMARY KEY (snapshot_id, reclaimable_id, path));
         INSERT INTO snapshot (taken_at_ms) VALUES (1);
         INSERT INTO reclaimable_size VALUES (1, 'npm-cache', 'tool-cache', 'x', 4096);
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(db);

    let scan = sb.json(&["scan"]);

    let npm = reclaimable(&scan, "npm-cache").unwrap();
    assert_eq!(npm["change_state"], "unknown", "{npm}");
    assert!(npm["change_bytes"].is_null(), "{npm}");
    assert_eq!(snapshot_count(&sb), 2);

    let next = sb.json(&["scan"]);
    assert_eq!(
        reclaimable(&next, "npm-cache").unwrap()["change_state"],
        "known"
    );
    assert_eq!(snapshot_count(&sb), 3);
}

#[test]
fn a_snapshot_keeps_each_disks_totals() {
    let sb = Sandbox::new();
    npm_cache_with(&sb, 3);

    let scan = sb.json(&["scan"]);

    let disk = &scan["disks"][0];
    let db = rusqlite::Connection::open(sb.data_dir().join("clch.db")).unwrap();
    let (name, free, reclaimable, complete): (String, Option<i64>, i64, bool) = db
        .query_row(
            "SELECT disk, free_bytes, reclaimable_bytes, complete FROM disk_total",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(name, disk["disk"].as_str().unwrap());
    assert_eq!(free, disk["free_bytes"].as_i64());
    assert_eq!(reclaimable, blocks(3));
    assert!(complete);
}
