mod common;

use common::{Sandbox, blocks};
use std::path::{Path, PathBuf};

/// What a fake npm does for one command line.
struct Behaviour {
    unix: String,
    windows: String,
}

/// `path` as a batch file must spell it: `del` and `rmdir` read `/` as the
/// start of a switch.
fn batch_path(path: &Path) -> String {
    path.display().to_string().replace('/', "\\")
}

fn remove_dir(dir: &Path) -> Behaviour {
    Behaviour {
        unix: format!("rm -rf '{}'", dir.display()),
        windows: format!("rmdir /s /q \"{}\"", batch_path(dir)),
    }
}

fn remove_file(file: &Path) -> Behaviour {
    Behaviour {
        unix: format!("rm -f '{}'", file.display()),
        windows: format!("del /q \"{}\"", batch_path(file)),
    }
}

fn fail_with(message: &str) -> Behaviour {
    Behaviour {
        unix: format!("echo {message} >&2; exit 1"),
        windows: format!("echo {message} 1>&2\r\nexit /b 1"),
    }
}

/// A sandbox with npm and npx caches under a root a fake npm reports.
struct NpmMachine {
    sb: Sandbox,
    root: PathBuf,
}

impl NpmMachine {
    /// `_cacache` holds `kept` bytes npm's index refers to and `garbage`
    /// bytes of content nothing refers to, which `npm cache verify`
    /// collects; `_npx` holds `npx` bytes.
    fn new(kept: usize, garbage: usize, npx: usize) -> Self {
        let sb = Sandbox::new();
        let root = sb.root().join("npm-root");
        sb.write_file(&root.join("_cacache/content-v2/kept"), kept);
        sb.write_file(&root.join("_cacache/content-v2/sha512/00/orphan"), garbage);
        sb.write_file(&root.join("_npx/0abc/node_modules/x/index.js"), npx);
        NpmMachine { sb, root }
    }

    fn cacache(&self) -> PathBuf {
        self.root.join("_cacache")
    }

    fn npx(&self) -> PathBuf {
        self.root.join("_npx")
    }

    /// Installs a fake npm. Commands not overridden behave as real npm
    /// would on these folders.
    fn npm(&self, overrides: &[(&str, Behaviour)]) {
        self.npm_version("11.6.2", overrides);
    }

    fn npm_version(&self, version: &str, overrides: &[(&str, Behaviour)]) {
        let mut commands: Vec<(String, Behaviour)> = vec![
            (
                "--version".into(),
                Behaviour {
                    unix: format!("echo {version}"),
                    windows: format!("echo {version}"),
                },
            ),
            (
                "cache verify".into(),
                remove_file(&self.cacache().join("content-v2/sha512/00/orphan")),
            ),
            ("cache clean --force".into(), remove_dir(&self.cacache())),
            ("cache npx rm --force".into(), remove_dir(&self.npx())),
        ];
        for (args, behaviour) in overrides {
            commands.retain(|(a, _)| a != args);
            commands.push((
                args.to_string(),
                Behaviour {
                    unix: behaviour.unix.clone(),
                    windows: behaviour.windows.clone(),
                },
            ));
        }
        let root = self.root.display();
        let mut unix = format!("case \"$*\" in\n  'config get cache') echo '{root}' ;;\n");
        // Each command jumps to its own label and ends with a top-level
        // `exit /b`, whose status cmd.exe passes back.
        let mut windows =
            "set \"ARGS=%*\"\r\nif \"%ARGS%\"==\"config get cache\" goto config\r\n".to_string();
        let mut labels = format!(":config\r\necho {root}\r\nexit /b 0\r\n");
        for (i, (args, behaviour)) in commands.iter().enumerate() {
            unix.push_str(&format!("  '{args}') {} ;;\n", behaviour.unix));
            windows.push_str(&format!("if \"%ARGS%\"==\"{args}\" goto c{i}\r\n"));
            labels.push_str(&format!(":c{i}\r\n{}\r\nexit /b 0\r\n", behaviour.windows));
        }
        windows.push_str("exit /b 0\r\n");
        windows.push_str(&labels);
        unix.push_str("esac");
        self.sb.fake_tool("npm", &unix, &windows);
    }

    fn cleanup_calls(&self) -> Vec<String> {
        self.sb
            .calls()
            .into_iter()
            .filter(|c| !c.contains("config get cache") && !c.contains("--version"))
            .collect()
    }
}

fn entry<'a>(list: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    list.as_array()
        .expect("a list")
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} missing from {list}"))
}

#[test]
fn clean_without_yes_only_shows_the_plan() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let out =
        m.sb.clch()
            .arg("clean")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
    let text = String::from_utf8_lossy(&out);

    assert!(text.contains("npm cache verify"), "plan was:\n{text}");
    assert!(text.contains("--yes"), "plan was:\n{text}");
    assert!(
        m.cleanup_calls().is_empty(),
        "calls: {:?}",
        m.cleanup_calls()
    );
    assert!(m.cacache().join("content-v2/sha512/00/orphan").exists());
    assert!(m.npx().exists());
}

#[test]
fn the_plan_states_level_action_and_space_as_json() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let plan = m.sb.json(&["clean"]);

    assert_eq!(plan["level"], "unused");
    let npm = entry(&plan["plan"], "npm-cache");
    assert_eq!(npm["cleanable"], true);
    assert_eq!(npm["action"], "npm cache verify");
    assert!(npm.get("expected_bytes").is_none(), "{npm}");
    assert!(npm.get("up_to_bytes").is_none(), "{npm}");

    let full = m.sb.json(&["clean", "--full"]);
    assert_eq!(
        entry(&full["plan"], "npm-cache")["expected_bytes"],
        blocks(1200)
    );
    assert_eq!(
        entry(&full["plan"], "npx-cache")["expected_bytes"],
        blocks(300)
    );
    assert!(m.cleanup_calls().is_empty());
}

#[test]
fn yes_runs_unused_cleanup_through_npm_by_default() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    m.sb.clch()
        .args(["clean", "npm-cache", "--yes"])
        .assert()
        .success();

    assert_eq!(m.cleanup_calls(), vec!["npm cache verify"]);
    assert!(m.cacache().join("content-v2/kept").exists());
    assert!(!m.cacache().join("content-v2/sha512/00/orphan").exists());
}

#[test]
fn full_cleanup_runs_only_when_asked_for() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let result = m.sb.json(&["clean", "npm-cache", "--full", "--yes"]);

    assert_eq!(m.cleanup_calls(), vec!["npm cache clean --force"]);
    assert!(!m.cacache().exists());
    assert_eq!(
        entry(&result["results"], "npm-cache")["freed_bytes"],
        blocks(1200)
    );
}

#[test]
fn the_result_reports_the_space_actually_freed() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let result = m.sb.json(&["clean", "npm-cache", "--yes"]);

    let npm = entry(&result["results"], "npm-cache");
    assert_eq!(npm["status"], "cleaned");
    assert_eq!(npm["freed_bytes"], blocks(200));
    assert_eq!(result["freed_bytes"], blocks(200));
}

#[test]
fn only_the_chosen_reclaimables_are_cleaned() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    m.sb.clch()
        .args(["clean", "npx-cache", "--full", "--yes"])
        .assert()
        .success();

    assert_eq!(m.cleanup_calls(), vec!["npm cache npx rm --force"]);
    assert!(!m.npx().exists());
    assert!(m.cacache().join("content-v2/sha512/00/orphan").exists());
}

#[test]
fn the_npx_cache_has_no_unused_cleanup_and_says_why() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let result = m.sb.json(&["clean", "--yes"]);

    let npx = entry(&result["results"], "npx-cache");
    assert_eq!(npx["status"], "skipped");
    assert!(npx["reason"].as_str().unwrap().contains("--full"), "{npx}");
    assert_eq!(m.cleanup_calls(), vec!["npm cache verify"]);
    assert!(m.npx().exists());
}

#[test]
fn one_failure_does_not_stop_the_others_and_carries_the_tools_output() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[("cache clean --force", fail_with("EPERM-from-npm"))]);

    let out =
        m.sb.clch()
            .args(["clean", "--full", "--yes", "--json"])
            .output()
            .unwrap();
    let result: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(out.status.code(), Some(1));
    let npm = entry(&result["results"], "npm-cache");
    assert_eq!(npm["status"], "failed");
    assert!(
        npm["output"].as_str().unwrap().contains("EPERM-from-npm"),
        "{npm}"
    );
    assert_eq!(entry(&result["results"], "npx-cache")["status"], "cleaned");
    assert!(!m.npx().exists());
}

#[test]
fn without_npm_on_path_nothing_is_cleaned_and_the_reason_is_given() {
    let sb = Sandbox::new();
    let cache = sb.default_npm_cache().join("_cacache");
    sb.write_file(&cache.join("a"), 10);

    let plan = sb.json(&["clean"]);
    let npm = entry(&plan["plan"], "npm-cache");
    assert_eq!(npm["cleanable"], false);
    assert!(npm["reason"].as_str().unwrap().contains("npm"), "{npm}");

    sb.clch().args(["clean", "--yes"]).assert().success();
    assert!(cache.join("a").exists());
}

#[test]
fn an_unknown_reclaimable_is_an_error() {
    let m = NpmMachine::new(1, 1, 1);
    m.npm(&[]);

    m.sb.clch()
        .args(["clean", "no-such-thing", "--yes"])
        .assert()
        .failure()
        .code(2);
    assert!(m.cleanup_calls().is_empty());
}

#[test]
fn the_text_result_lists_each_reclaimable_and_the_total() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);

    let out =
        m.sb.clch()
            .args(["clean", "--full", "--yes"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
    let text = String::from_utf8_lossy(&out);

    assert!(text.contains("npm cache"), "result was:\n{text}");
    assert!(text.contains("npx cache"), "result was:\n{text}");
    assert!(text.contains("6.1 MB"), "total missing:\n{text}");
}

#[test]
fn the_npx_cache_needs_npm_11_2_and_older_npm_is_told_so() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm_version("10.9.2", &[]);

    let result = m.sb.json(&["clean", "npx-cache", "--full", "--yes"]);

    let npx = entry(&result["results"], "npx-cache");
    assert_eq!(npx["status"], "skipped");
    assert!(npx["reason"].as_str().unwrap().contains("11.2"), "{npx}");
    assert!(
        m.cleanup_calls().is_empty(),
        "calls: {:?}",
        m.cleanup_calls()
    );
    assert!(m.npx().exists());
}

#[test]
fn naming_a_reclaimable_twice_plans_it_once() {
    let m = NpmMachine::new(1000, 200, 300);
    m.npm(&[]);
    std::fs::remove_dir_all(m.npx()).unwrap();

    let plan =
        m.sb.json(&["clean", "npm-cache", "npm-cache", "npx-cache", "npx-cache"]);

    let ids: Vec<_> = plan["plan"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].clone())
        .collect();
    assert_eq!(ids, vec!["npm-cache", "npx-cache"]);
}

#[cfg(unix)]
#[test]
fn interrupting_a_cleanup_stops_the_tool_too() {
    use std::os::unix::process::CommandExt;

    let m = NpmMachine::new(1000, 200, 300);
    let pid_file = m.sb.root().join("verify.pid");
    let started = m.sb.root().join("verify.started");
    m.npm(&[(
        "cache verify",
        Behaviour {
            unix: format!(
                "echo $$ > '{}'; touch '{}'; exec sleep 60",
                pid_file.display(),
                started.display()
            ),
            windows: String::new(),
        },
    )]);

    let mut clch = std::process::Command::new(assert_cmd::cargo::cargo_bin("clch"));
    clch.args(["clean", "npm-cache", "--yes"])
        .env_clear()
        .env("PATH", m.sb.bin())
        .env("HOME", m.sb.home())
        .stdout(std::process::Stdio::null())
        .process_group(0);
    let mut child = clch.spawn().unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !started.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "npm cache verify never started"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    // Ctrl+C in a terminal signals the whole foreground process group.
    let pgid = child.id() as i32;
    std::process::Command::new("/bin/kill")
        .args(["-INT", &format!("-{pgid}")])
        .status()
        .unwrap();
    let _ = child.wait();
    std::thread::sleep(std::time::Duration::from_millis(300));

    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let alive = std::process::Command::new("/bin/kill")
        .args(["-0", pid.trim()])
        .status()
        .unwrap()
        .success();
    assert!(!alive, "npm's child {pid} kept running after Ctrl+C");
}
