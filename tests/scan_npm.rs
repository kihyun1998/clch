mod common;

use common::{Sandbox, blocks, reclaimable};
use std::path::Path;

fn same_path(reported: &serde_json::Value, expected: &Path) -> bool {
    let reported = Path::new(reported.as_str().expect("path is a string"));
    reported == expected
}

#[test]
fn finds_the_npm_cache_at_the_default_location_when_npm_is_absent() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("content-v2/a"), 1000);
    sb.write_file(&cache.join("index-v5/b"), 234);

    let scan = sb.json(&["scan"]);

    let npm = reclaimable(&scan, "npm-cache").expect("npm cache listed");
    assert_eq!(npm["kind"], "tool-cache");
    assert_eq!(npm["size_bytes"], blocks(1234));
    assert!(same_path(&npm["path"], &cache), "path was {}", npm["path"]);
}

#[test]
fn follows_the_cache_location_npm_reports() {
    let sb = Sandbox::new();
    let custom = sb.root().join("custom-npm-cache");
    sb.write_file(&custom.join("_cacache/content-v2/a"), 500);
    sb.write_file(&sb.default_npm_cache().join("_cacache/content-v2/a"), 9);
    sb.fake_npm_reporting_cache(&custom);

    let scan = sb.json(&["scan"]);

    let npm = reclaimable(&scan, "npm-cache").expect("npm cache listed");
    assert_eq!(npm["size_bytes"], blocks(500));
    assert!(
        same_path(&npm["path"], &custom.join("_cacache")),
        "path was {}",
        npm["path"]
    );
}

#[test]
fn lists_the_npx_cache_as_its_own_reclaimable() {
    let sb = Sandbox::new();
    let root = sb.default_npm_cache();
    sb.write_file(&root.join("_cacache/content-v2/a"), 10);
    sb.write_file(&root.join("_npx/0123abcd/node_modules/x/index.js"), 700);

    let scan = sb.json(&["scan"]);

    let npx = reclaimable(&scan, "npx-cache").expect("npx cache listed");
    assert_eq!(npx["kind"], "tool-cache");
    assert_eq!(npx["size_bytes"], blocks(700));
    assert!(
        same_path(&npx["path"], &root.join("_npx")),
        "path was {}",
        npx["path"]
    );
    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm cache listed")["size_bytes"],
        blocks(10)
    );
}

#[test]
fn a_machine_without_npm_caches_scans_cleanly() {
    let sb = Sandbox::new();

    let scan = sb.json(&["scan"]);

    assert_eq!(scan["reclaimables"].as_array().expect("array").len(), 0);
    let text = sb
        .clch()
        .arg("scan")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8_lossy(&text).contains("No reclaimables found"));
}

#[test]
fn the_table_shows_what_the_json_carries() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("content-v2/a"), 366);

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
    assert!(text.contains("1.5 MB"), "table was:\n{text}");
    assert!(
        text.contains(&cache.display().to_string()),
        "table was:\n{text}"
    );
}

#[test]
fn npm_is_asked_from_home_so_a_project_npmrc_does_not_redirect_it() {
    let sb = Sandbox::new();
    let home_answer = sb.home().join("from-home");
    let project_answer = sb.root().join("from-project");
    sb.write_file(&home_answer.join("_cacache/a"), 40);
    sb.write_file(&project_answer.join("_cacache/a"), 7);
    sb.write_file(&sb.home().join("here-is-home"), 1);
    let project = sb.root().join("project");
    std::fs::create_dir_all(&project).unwrap();
    sb.fake_tool(
        "npm",
        &format!(
            "if [ -f here-is-home ]; then echo '{}'; else echo '{}'; fi",
            home_answer.display(),
            project_answer.display()
        ),
        &format!(
            "if exist here-is-home (echo {}) else (echo {})",
            home_answer.display(),
            project_answer.display()
        ),
    );

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .current_dir(&project)
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let npm = reclaimable(&scan, "npm-cache").expect("npm cache listed");
    assert_eq!(npm["size_bytes"], blocks(40));
}

#[test]
fn npm_is_asked_once_per_scan() {
    let sb = Sandbox::new();
    let root = sb.root().join("reported");
    sb.write_file(&root.join("_cacache/a"), 1);
    sb.write_file(&root.join("_npx/k/a"), 1);
    sb.fake_npm_reporting_cache(&root);

    let scan = sb.json(&["scan"]);

    assert!(reclaimable(&scan, "npm-cache").is_some());
    assert!(reclaimable(&scan, "npx-cache").is_some());
    let asked: Vec<_> = sb
        .calls()
        .into_iter()
        .filter(|c| c.contains("config get cache"))
        .collect();
    assert_eq!(asked.len(), 1, "calls: {:?}", sb.calls());
}

#[test]
fn the_npx_cache_follows_the_location_npm_reports() {
    let sb = Sandbox::new();
    let custom = sb.root().join("custom");
    sb.write_file(&custom.join("_npx/k/a"), 321);
    sb.write_file(&sb.default_npm_cache().join("_npx/k/a"), 5);
    sb.fake_npm_reporting_cache(&custom);

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npx-cache").expect("npx listed")["size_bytes"],
        blocks(321)
    );
}

#[test]
fn a_failing_npm_falls_back_to_the_default_location() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/a"), 77);
    sb.fake_tool(
        "npm",
        "echo /nowhere; exit 1",
        "echo C:\\nowhere\r\nexit /b 1",
    );

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm listed")["size_bytes"],
        blocks(77)
    );
}

#[test]
fn npm_output_that_is_not_one_line_falls_back_to_the_default_location() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/a"), 66);
    sb.fake_tool(
        "npm",
        "echo 'npm notice: a new version'; echo /nowhere",
        "echo npm notice: a new version\r\necho C:\\nowhere",
    );

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm listed")["size_bytes"],
        blocks(66)
    );
}

#[test]
fn a_hung_npm_is_abandoned_and_the_default_location_used() {
    let sb = Sandbox::new();
    sb.write_file(&sb.default_npm_cache().join("_cacache/a"), 55);
    sb.limit_tools_to(1);
    sb.fake_tool("npm", "sleep 60", "ping -n 60 127.0.0.1 >nul");

    let started = std::time::Instant::now();
    let scan = sb.json(&["scan"]);

    assert!(
        started.elapsed().as_secs() < 30,
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm listed")["size_bytes"],
        blocks(55)
    );
}

#[cfg(windows)]
#[test]
fn without_localappdata_npm_defaults_to_the_home_folder_on_windows() {
    let sb = Sandbox::new();
    let cache = sb.home().join("npm-cache").join("_cacache");
    sb.write_file(&cache.join("a"), 88);

    let out = sb
        .clch()
        .args(["scan", "--json"])
        .env_remove("LOCALAPPDATA")
        .output()
        .unwrap();
    let scan: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let npm = reclaimable(&scan, "npm-cache").expect("npm listed");
    assert_eq!(npm["size_bytes"], blocks(88));
    assert!(same_path(&npm["path"], &cache), "path was {}", npm["path"]);
}

#[cfg(unix)]
#[test]
fn a_cache_folder_that_is_a_symlink_is_sized_by_what_it_points_to() {
    let sb = Sandbox::new();
    let real = sb.root().join("elsewhere");
    sb.write_file(&real.join("a"), 444);
    std::fs::create_dir_all(sb.default_npm_cache()).unwrap();
    std::os::unix::fs::symlink(&real, sb.default_npm_cache().join("_cacache")).unwrap();

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "npm-cache").expect("npm listed")["size_bytes"],
        blocks(444)
    );
}

#[cfg(unix)]
#[test]
fn an_abandoned_tool_leaves_no_process_behind() {
    let sb = Sandbox::new();
    let pid_file = sb.root().join("child.pid");
    sb.limit_tools_to(5);
    sb.fake_tool(
        "npm",
        &format!("sleep 60 & echo $! > '{}'; wait", pid_file.display()),
        "",
    );

    sb.json(&["scan"]);

    let pid = std::fs::read_to_string(&pid_file).expect("fake npm started its child");
    let alive = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .status()
        .expect("run kill -0")
        .success();
    assert!(!alive, "the tool's child {pid} is still running");
}

#[test]
fn the_limit_on_a_tools_answer_can_be_set() {
    let sb = Sandbox::new();
    let custom = sb.root().join("custom");
    sb.write_file(&custom.join("_cacache/entry"), 2);
    sb.fake_tool(
        "npm",
        &format!("sleep 2; echo '{}'", custom.display()),
        &format!("ping -n 3 127.0.0.1 >nul\r\necho {}", custom.display()),
    );
    let reported = custom.join("_cacache").display().to_string();

    sb.limit_tools_to(1);
    let short = sb.json(&["scan"]);
    assert!(
        !short.to_string().contains(&reported),
        "waited past the limit: {short}"
    );

    sb.limit_tools_to(60);
    let long = sb.json(&["scan"]);
    assert_eq!(reclaimable(&long, "npm-cache").unwrap()["path"], reported);
}
