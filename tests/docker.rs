mod common;

use common::{Reply, Sandbox, reclaimable, says, tool};
use std::fs;
use std::path::{Path, PathBuf};

const REPORT: &str = "system df --format json";
const VERBOSE: &str = "system df --verbose --format json";
const ENGINE: &str = "info --format {{.OperatingSystem}}";
const ENDPOINT: &str = "context inspect --format {{.Endpoints.docker.Host}}";

const BUILD_UNUSED: &str = "builder prune --force";
const BUILD_FULL: &str = "builder prune --all --force";
const IMAGE_UNUSED: &str = "image prune --force";
const IMAGE_FULL: &str = "image prune --all --force";

/// What `docker system df --format json` prints, as Docker 28.5 words it, with
/// figures for images and volumes that must not be taken for anything else.
fn report(build_cache: &str) -> String {
    format!(
        "{{\"Active\":\"3\",\"Reclaimable\":\"9.9GB (37%)\",\"Size\":\"5.839GB\",\"TotalCount\":\"3\",\"Type\":\"Images\"}}\n\
         {{\"Active\":\"8\",\"Reclaimable\":\"20.48kB (0%)\",\"Size\":\"224.9MB\",\"TotalCount\":\"10\",\"Type\":\"Containers\"}}\n\
         {{\"Active\":\"5\",\"Reclaimable\":\"7GB (90%)\",\"Size\":\"8GB\",\"TotalCount\":\"5\",\"Type\":\"Local Volumes\"}}\n\
         {{\"Active\":\"0\",\"Reclaimable\":\"{build_cache}\",\"Size\":\"9GB\",\"TotalCount\":\"10\",\"Type\":\"Build Cache\"}}\n"
    )
}

/// What `docker system df --verbose --format json` prints: one image in use, and
/// unused ones holding these sizes alone.
fn verbose_report(unused: &[&str]) -> String {
    let mut images = vec![
        "{\"Containers\":\"2\",\"Repository\":\"used\",\"SharedSize\":\"0B\",\"Size\":\"3.19GB\",\"UniqueSize\":\"3.186GB\"}"
            .to_string(),
    ];
    for size in unused {
        images.push(format!(
            "{{\"Containers\":\"0\",\"Repository\":\"old\",\"SharedSize\":\"1GB\",\"Size\":\"4GB\",\"UniqueSize\":\"{size}\"}}"
        ));
    }
    format!(
        "{{\"Images\":[{}],\"Containers\":[],\"Volumes\":[],\"BuildCache\":[]}}\n",
        images.join(",")
    )
}

/// The report files a fake `docker` shows, and the ones its prunes bring in.
struct Reports {
    now: PathBuf,
    verbose_now: PathBuf,
    after: PathBuf,
    verbose_after: PathBuf,
}

fn reports(sb: &Sandbox, unused: &[&str], build_cache: &str) -> Reports {
    let files = Reports {
        now: sb.root().join("df.json"),
        verbose_now: sb.root().join("df-v.json"),
        after: sb.root().join("df-after.json"),
        verbose_after: sb.root().join("df-v-after.json"),
    };
    fs::write(&files.now, report(build_cache)).unwrap();
    fs::write(&files.verbose_now, verbose_report(unused)).unwrap();
    fs::write(&files.after, report("0B")).unwrap();
    fs::write(&files.verbose_after, verbose_report(&[])).unwrap();
    files
}

fn shows(path: &Path) -> Reply {
    Reply {
        unix: format!("cat '{}'", path.display()),
        windows: format!("type \"{}\"", path.display()),
    }
}

fn empties(files: &Reports) -> Reply {
    Reply {
        unix: format!(
            "cp '{}' '{}'; cp '{}' '{}'",
            files.after.display(),
            files.now.display(),
            files.verbose_after.display(),
            files.verbose_now.display()
        ),
        windows: format!(
            "copy /y \"{}\" \"{}\" >nul\r\ncopy /y \"{}\" \"{}\" >nul",
            files.after.display(),
            files.now.display(),
            files.verbose_after.display(),
            files.verbose_now.display()
        ),
    }
}

fn fails(message: &str) -> Reply {
    Reply {
        unix: format!("echo {message} >&2; exit 1"),
        windows: format!("echo {message} 1>&2\r\nexit /b 1"),
    }
}

fn local_endpoint() -> Reply {
    says("unix:///var/run/docker.sock")
}

/// A fake `docker` on a local engine called `engine`, reporting unused images of
/// 2.18 GB and 1.5 GB of build cache, which its prunes empty.
fn docker(sb: &Sandbox, engine: &str) -> Reports {
    let files = reports(sb, &["2.18GB", "0B"], "1.5GB");
    let prune = || empties(&files);
    tool(
        sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, shows(&files.now)),
            (VERBOSE, shows(&files.verbose_now)),
            (ENGINE, says(engine)),
            (BUILD_UNUSED, prune()),
            (BUILD_FULL, prune()),
            (IMAGE_UNUSED, prune()),
            (IMAGE_FULL, prune()),
        ],
    );
    files
}

/// A fake `docker` whose engine is local and whose daemon does not answer.
fn stopped_docker(sb: &Sandbox) {
    tool(
        sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, fails("Cannot connect to the Docker daemon")),
            (VERBOSE, fails("Cannot connect to the Docker daemon")),
        ],
    );
}

fn desktop_folder(sb: &Sandbox) -> PathBuf {
    if cfg!(windows) {
        sb.local_app_data().join("Docker").join("wsl")
    } else {
        sb.home()
            .join("Library/Containers/com.docker.docker/Data/vms/0/data")
    }
}

fn calls_of_docker_that_clean(sb: &Sandbox) -> Vec<String> {
    sb.calls()
        .into_iter()
        .filter(|c| c.contains("prune") || c.contains(" rm"))
        .collect()
}

fn docker_rows(scan: &serde_json::Value) -> Vec<String> {
    scan["reclaimables"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["id"].as_str().unwrap().starts_with("docker"))
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_build_cache_and_unused_images_show_the_sizes_docker_reports() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let scan = sb.json(&["scan"]);

    let images = reclaimable(&scan, "docker-images").expect("images listed");
    assert_eq!(images["size_bytes"], 2_180_000_000u64);
    assert_eq!(images["kind"], "tool-cache");
    assert_eq!(images["complete"], true);
    let build = reclaimable(&scan, "docker-build-cache").expect("build cache listed");
    assert_eq!(build["size_bytes"], 1_500_000_000u64);
}

#[test]
fn only_images_no_container_uses_count_as_unused() {
    let sb = Sandbox::new();
    let files = reports(&sb, &[], "0B");
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, shows(&files.now)),
            (VERBOSE, shows(&files.verbose_now)),
        ],
    );

    let scan = sb.json(&["scan"]);

    assert_eq!(
        reclaimable(&scan, "docker-images").unwrap()["size_bytes"],
        0
    );
}

#[test]
fn volumes_and_containers_are_never_listed_or_cleaned() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let scan = sb.json(&["scan"]);
    assert_eq!(docker_rows(&scan), ["docker-build-cache", "docker-images"]);

    sb.json(&["clean", "--full", "--yes"]);
    let calls = calls_of_docker_that_clean(&sb);
    assert!(!calls.is_empty());
    for call in calls {
        assert!(
            call == format!("docker {BUILD_FULL}") || call == format!("docker {IMAGE_FULL}"),
            "{call}"
        );
    }
}

#[test]
fn an_unused_cleanup_runs_dockers_plain_prunes_and_shows_no_figure_beforehand() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let plan = sb.json(&["clean"]);
    let actions: Vec<_> = plan["plan"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["action"].as_str().unwrap().to_string(),
                e["expected_bytes"].clone(),
            )
        })
        .collect();
    assert!(actions.contains(&(format!("docker {BUILD_UNUSED}"), serde_json::Value::Null)));
    assert!(actions.contains(&(format!("docker {IMAGE_UNUSED}"), serde_json::Value::Null)));
    assert!(
        calls_of_docker_that_clean(&sb).is_empty(),
        "a plan runs nothing"
    );

    sb.json(&["clean", "--yes"]);
    assert_eq!(
        calls_of_docker_that_clean(&sb),
        [
            format!("docker {BUILD_UNUSED}"),
            format!("docker {IMAGE_UNUSED}")
        ]
    );
}

#[test]
fn a_full_cleanup_is_asked_for_by_name_and_says_what_it_can_free() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let plan = sb.json(&["clean", "docker-images", "--full"]);
    assert_eq!(plan["plan"][0]["action"], format!("docker {IMAGE_FULL}"));
    assert_eq!(plan["plan"][0]["expected_bytes"], 2_180_000_000u64);
    assert_eq!(plan["plan"].as_array().unwrap().len(), 1);

    sb.json(&["clean", "docker-images", "--full", "--yes"]);
    assert_eq!(
        calls_of_docker_that_clean(&sb),
        [format!("docker {IMAGE_FULL}")]
    );
}

#[test]
fn what_a_cleanup_freed_counts_though_no_disk_can_be_named() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let scan = sb.json(&["scan"]);
    assert!(reclaimable(&scan, "docker-images").unwrap()["disk"].is_null());

    let outcome = sb.json(&["clean", "docker-images", "--full", "--yes"]);

    assert_eq!(outcome["results"][0]["freed_bytes"], 2_180_000_000u64);
    assert_eq!(outcome["freed_bytes"], 3_680_000_000u64);
    assert_eq!(outcome["disks"][0]["disk"], "unknown");
}

#[test]
fn docker_desktops_figures_count_on_the_disk_holding_its_disk_image() {
    let sb = Sandbox::new();
    docker(&sb, "Docker Desktop");
    let folder = desktop_folder(&sb);
    fs::create_dir_all(&folder).unwrap();

    let scan = sb.json(&["scan"]);

    let images = reclaimable(&scan, "docker-images").unwrap();
    assert_eq!(images["path"], folder.display().to_string());
    let disk = images["disk"].as_str().expect("disk named").to_string();
    let total = scan["disks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["disk"] == disk.as_str())
        .unwrap();
    assert_eq!(total["reclaimable_bytes"], 3_680_000_000u64);
}

#[test]
fn another_engine_is_not_taken_to_be_on_docker_desktops_disk() {
    let sb = Sandbox::new();
    docker(&sb, "Ubuntu 24.04");
    fs::create_dir_all(desktop_folder(&sb)).unwrap();

    let scan = sb.json(&["scan"]);

    let images = reclaimable(&scan, "docker-images").unwrap();
    assert!(images["disk"].is_null());
    assert_eq!(images["path"], "docker engine");
}

#[test]
fn a_stopped_daemon_is_reported_and_the_scan_still_succeeds() {
    let sb = Sandbox::new();
    stopped_docker(&sb);

    let scan = sb.json(&["scan"]);

    assert!(docker_rows(&scan).is_empty());
    assert_eq!(scan["tool_notes"].as_array().unwrap().len(), 1);
    assert_eq!(scan["tool_notes"][0]["tool"], "docker");
    assert!(
        scan["tool_notes"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("daemon")
    );
    assert!(printed(&sb, &["scan"]).contains("daemon"));
}

/// What `clch <args>` prints, having succeeded.
fn printed(sb: &Sandbox, args: &[&str]) -> String {
    let output = sb.clch().args(args).output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn a_stopped_daemon_is_the_reason_given_for_not_cleaning() {
    let sb = Sandbox::new();
    stopped_docker(&sb);

    let plan = sb.json(&["clean", "docker-images", "--yes"]);

    assert_eq!(plan["results"][0]["status"], "skipped");
    let reason = plan["results"][0]["reason"].as_str().unwrap();
    assert!(reason.contains("daemon"), "{reason}");
    assert!(calls_of_docker_that_clean(&sb).is_empty());

    let plan = sb.json(&["clean"]);
    assert_eq!(plan["tool_notes"][0]["tool"], "docker");
}

#[test]
fn a_stopped_daemon_does_not_lose_dockers_history() {
    let sb = Sandbox::new();
    let files = reports(&sb, &["2GB"], "1GB");
    let running = |sb: &Sandbox| {
        tool(
            sb,
            "docker",
            vec![
                (ENDPOINT, local_endpoint()),
                (REPORT, shows(&files.now)),
                (VERBOSE, shows(&files.verbose_now)),
            ],
        )
    };
    running(&sb);
    sb.json(&["scan"]);

    stopped_docker(&sb);
    let stopped = sb.json(&["scan"]);
    assert_eq!(stopped["gone"].as_array().unwrap().len(), 0, "{stopped}");

    fs::write(&files.verbose_now, verbose_report(&["3GB"])).unwrap();
    running(&sb);
    let back = sb.json(&["scan"]);
    let images = reclaimable(&back, "docker-images").unwrap();
    assert_eq!(images["change_state"], "known");
    assert_eq!(images["change_bytes"], 1_000_000_000i64);
}

#[test]
fn a_report_that_cannot_be_read_is_said_so_and_not_taken_as_zero() {
    let sb = Sandbox::new();
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, says("something-else")),
            (VERBOSE, says("something-else")),
        ],
    );

    let scan = sb.json(&["scan"]);

    assert!(docker_rows(&scan).is_empty());
    assert!(
        scan["tool_notes"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("could not be read")
    );
}

#[test]
fn an_engine_on_another_machine_is_neither_listed_nor_cleaned() {
    let sb = Sandbox::new();
    let files = reports(&sb, &["2GB"], "1GB");
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, says("ssh://deploy@prod")),
            (REPORT, shows(&files.now)),
            (VERBOSE, shows(&files.verbose_now)),
        ],
    );

    let scan = sb.json(&["scan"]);
    assert!(docker_rows(&scan).is_empty());
    let reason = scan["tool_notes"][0]["reason"].as_str().unwrap();
    assert!(reason.contains("ssh://deploy@prod"), "{reason}");

    let named = sb.json(&["clean", "docker-images", "--full", "--yes"]);
    assert_eq!(named["results"][0]["status"], "skipped");
    assert!(calls_of_docker_that_clean(&sb).is_empty());
}

#[test]
fn an_engine_that_cannot_be_placed_is_left_alone() {
    let sb = Sandbox::new();
    tool(&sb, "docker", vec![(ENDPOINT, fails("context not found"))]);

    let scan = sb.json(&["scan"]);

    assert!(docker_rows(&scan).is_empty());
    assert_eq!(scan["tool_notes"][0]["tool"], "docker");
    let reason = scan["tool_notes"][0]["reason"].as_str().unwrap();
    assert!(reason.contains("where its engine is"), "{reason}");
}

#[test]
fn a_folder_named_like_the_placeholder_path_means_nothing() {
    let sb = Sandbox::new();
    docker(&sb, "Ubuntu 24.04");
    fs::create_dir(sb.root().join("docker engine")).unwrap();

    let scan = sb.json(&["scan"]);
    assert!(reclaimable(&scan, "docker-images").unwrap()["disk"].is_null());

    let output = sb.clch().args(["clean", "docker engine"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(calls_of_docker_that_clean(&sb).is_empty());
}

#[test]
fn a_daemon_gone_after_the_cleanup_frees_nothing_on_paper() {
    let sb = Sandbox::new();
    let files = reports(&sb, &["2GB"], "1GB");
    let gone = sb.root().join("gone");
    let answer_until_gone = |file: &Path| Reply {
        unix: format!(
            "if [ -e '{}' ]; then echo stopped >&2; exit 1; fi; cat '{}'",
            gone.display(),
            file.display()
        ),
        windows: format!(
            "if exist \"{}\" exit /b 1\r\ntype \"{}\"",
            gone.display(),
            file.display()
        ),
    };
    let stop_and_fail = Reply {
        unix: format!("touch '{}'; echo boom >&2; exit 1", gone.display()),
        windows: format!(
            "echo.>\"{}\"\r\necho boom 1>&2\r\nexit /b 1",
            gone.display()
        ),
    };
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, answer_until_gone(&files.now)),
            (VERBOSE, answer_until_gone(&files.verbose_now)),
            (IMAGE_FULL, stop_and_fail),
        ],
    );

    let output = sb
        .clch()
        .args(["clean", "docker-images", "--full", "--yes", "--json"])
        .output()
        .unwrap();

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outcome["results"][0]["status"], "failed");
    assert_eq!(outcome["results"][0]["freed_bytes"], 0);
    assert_eq!(outcome["results"][0]["complete"], false);
    assert_eq!(outcome["freed_bytes"], 0);
    assert_eq!(outcome["complete"], false);
    assert_eq!(outcome["tool_notes"][0]["tool"], "docker");
}

#[test]
fn a_cleanup_of_everything_says_why_docker_is_missing() {
    let sb = Sandbox::new();
    stopped_docker(&sb);

    let outcome = sb.json(&["clean", "--yes"]);
    assert_eq!(outcome["tool_notes"][0]["tool"], "docker");

    assert!(printed(&sb, &["clean", "--yes"]).contains("daemon"));
}

#[test]
fn without_docker_there_is_nothing_to_say() {
    let sb = Sandbox::new();

    let scan = sb.json(&["scan"]);

    assert!(docker_rows(&scan).is_empty());
    assert_eq!(scan["tool_notes"].as_array().unwrap().len(), 0);
}

#[test]
fn a_failed_prune_carries_dockers_output_and_stops_nothing_else() {
    let sb = Sandbox::new();
    let files = reports(&sb, &["2GB"], "1GB");
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, shows(&files.now)),
            (VERBOSE, shows(&files.verbose_now)),
            (BUILD_UNUSED, fails("prune-exploded")),
            (IMAGE_UNUSED, empties(&files)),
        ],
    );

    let output = sb
        .clch()
        .args(["clean", "--yes", "--json"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let result = |id: &str| {
        outcome["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(result("docker-build-cache")["status"], "failed");
    assert!(
        result("docker-build-cache")["output"]
            .as_str()
            .unwrap()
            .contains("prune-exploded")
    );
    assert_eq!(result("docker-images")["status"], "cleaned");
    assert_eq!(result("docker-images")["freed_bytes"], 2_000_000_000u64);
}

#[test]
fn a_docker_cleanup_on_windows_explains_how_to_compact_the_wsl_disk() {
    let sb = Sandbox::new();
    docker(&sb, "Docker Desktop");

    let outcome = sb.json(&["clean", "--yes"]);
    let notes = outcome["notes"].as_array().unwrap();
    if cfg!(windows) {
        assert_eq!(notes.len(), 1, "{outcome}");
        let text = notes[0].as_str().unwrap();
        for step in ["wsl --shutdown", "compact vdisk", "select vdisk", "sparse"] {
            assert!(text.contains(step), "{step} in {text}");
        }
    } else {
        assert!(notes.is_empty(), "{outcome}");
    }
}

#[test]
fn no_compaction_guidance_when_docker_was_not_cleaned() {
    let sb = Sandbox::new();
    let cache = sb.root().join("custom");
    sb.write_file(&cache.join("_cacache/entry"), 1);
    sb.fake_npm_reporting_cache(&cache);
    docker(&sb, "Docker Desktop");

    let outcome = sb.json(&["clean", "npm-cache", "--yes"]);

    assert!(outcome["notes"].as_array().unwrap().is_empty());
}

#[test]
fn a_stopped_daemon_is_said_in_the_plan_table() {
    let sb = Sandbox::new();
    stopped_docker(&sb);

    assert!(printed(&sb, &["clean"]).contains("daemon"));
}

#[test]
fn a_prune_that_failed_gives_no_compaction_guidance() {
    let sb = Sandbox::new();
    let files = reports(&sb, &["2GB"], "0B");
    tool(
        &sb,
        "docker",
        vec![
            (ENDPOINT, local_endpoint()),
            (REPORT, shows(&files.now)),
            (VERBOSE, shows(&files.verbose_now)),
            (IMAGE_UNUSED, fails("prune-exploded")),
        ],
    );

    let output = sb
        .clch()
        .args(["clean", "docker-images", "--yes", "--json"])
        .output()
        .unwrap();

    let outcome: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(outcome["notes"].as_array().unwrap().is_empty());
}

#[test]
fn a_full_image_cleanup_warns_about_images_nothing_can_download_again() {
    let sb = Sandbox::new();
    docker(&sb, "Other Engine");

    let full = sb.json(&["clean", "docker-images", "--full"]);
    let warning = full["plan"][0]["warning"].as_str().unwrap();
    assert!(warning.contains("docker commit"), "{warning}");

    let unused = sb.json(&["clean", "docker-images"]);
    assert!(unused["plan"][0].get("warning").is_none(), "{unused}");
    let build = sb.json(&["clean", "docker-build-cache", "--full"]);
    assert!(build["plan"][0].get("warning").is_none(), "{build}");
}
