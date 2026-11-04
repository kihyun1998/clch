//! Seam 1: the `clch` binary run as a subprocess in a sandbox.
//!
//! A sandbox holds a fake home, the per-user data folders, and a `bin` folder
//! that is the only entry on clch's `PATH`.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

pub struct Sandbox {
    root: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("create sandbox");
        let sandbox = Sandbox { root };
        for dir in [
            sandbox.home(),
            sandbox.local_app_data(),
            sandbox.app_data(),
            sandbox.bin(),
        ] {
            fs::create_dir_all(dir).expect("create sandbox dir");
        }
        sandbox
    }

    pub fn root(&self) -> &Path {
        self.root.path()
    }

    pub fn home(&self) -> PathBuf {
        self.root().join("home")
    }

    pub fn local_app_data(&self) -> PathBuf {
        self.root().join("localappdata")
    }

    pub fn app_data(&self) -> PathBuf {
        self.root().join("appdata")
    }

    pub fn bin(&self) -> PathBuf {
        self.root().join("bin")
    }

    /// The npm cache folder npm uses when nothing configures it.
    pub fn default_npm_cache(&self) -> PathBuf {
        if cfg!(windows) {
            self.local_app_data().join("npm-cache")
        } else {
            self.home().join(".npm")
        }
    }

    /// Writes `bytes` bytes into a new file at `path`, creating its parents.
    pub fn write_file(&self, path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parent");
        fs::write(path, vec![b'x'; bytes]).expect("write file");
    }

    /// The file every fake tool appends `<name> <args>` to when it runs.
    pub fn calls_log(&self) -> PathBuf {
        self.root().join("calls.log")
    }

    /// Each fake-tool invocation so far, as `<name> <args>`.
    pub fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.calls_log())
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }

    /// Puts a fake executable named `name` on the sandbox `PATH`.
    ///
    /// `unix` is the body of a POSIX shell script and `windows` the body of a
    /// batch file; both receive the arguments clch passes, run with the
    /// system's own tools on their `PATH`, and log the call to `calls_log`.
    pub fn fake_tool(&self, name: &str, unix: &str, windows: &str) {
        let log = self.calls_log();
        let log = log.display();
        if cfg!(windows) {
            let path = self.bin().join(format!("{name}.cmd"));
            let body = format!(
                "@echo off\r\nset \"PATH=%SystemRoot%\\System32;%PATH%\"\r\n\
                 echo {name} %*>>\"{log}\"\r\n{windows}\r\n"
            );
            fs::write(path, body).expect("write fake tool");
        } else {
            let path = self.bin().join(name);
            let body = format!(
                "#!/bin/sh\nPATH=/usr/bin:/bin:$PATH\necho \"{name} $*\" >> '{log}'\n{unix}\n"
            );
            fs::write(&path, body).expect("write fake tool");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                    .expect("make fake tool executable");
            }
        }
    }

    /// A fake `npm` whose `npm config get cache` prints `cache_dir`.
    pub fn fake_npm_reporting_cache(&self, cache_dir: &Path) {
        let dir = cache_dir.display();
        self.fake_tool(
            "npm",
            &format!("if [ \"$1 $2 $3\" = \"config get cache\" ]; then echo '{dir}'; fi"),
            &format!("if \"%~1 %~2 %~3\"==\"config get cache\" echo {dir}"),
        );
    }

    /// `clch` with an environment that sees only the sandbox.
    pub fn clch(&self) -> Command {
        let mut cmd = Command::cargo_bin("clch").expect("clch binary");
        cmd.env_clear()
            .env("PATH", self.bin())
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("LOCALAPPDATA", self.local_app_data())
            .env("APPDATA", self.app_data())
            .current_dir(self.root());
        for keep in [
            "SystemRoot",
            "SYSTEMROOT",
            "ComSpec",
            "PATHEXT",
            "TEMP",
            "TMP",
            "TMPDIR",
        ] {
            if let Ok(value) = std::env::var(keep) {
                cmd.env(keep, value);
            }
        }
        cmd
    }

    /// Runs `clch <args> --json` and parses its output.
    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self
            .clch()
            .args(args)
            .arg("--json")
            .output()
            .expect("run clch");
        assert!(
            output.status.success(),
            "clch {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("clch printed JSON")
    }
}

/// The reclaimable with `id` in a scan's JSON output.
pub fn reclaimable<'a>(scan: &'a serde_json::Value, id: &str) -> Option<&'a serde_json::Value> {
    scan["reclaimables"]
        .as_array()
        .expect("reclaimables is an array")
        .iter()
        .find(|r| r["id"] == id)
}
