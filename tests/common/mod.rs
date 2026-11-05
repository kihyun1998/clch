//! Seam 1: the `clch` binary run as a subprocess in a sandbox.
//!
//! A sandbox holds a fake home, the per-user data folders, and a `bin` folder
//! that is the only entry on clch's `PATH`.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// The allocation unit of the filesystems the tests run on.
pub const BLOCK: usize = 4096;

/// The size in bytes of `count` blocks.
pub fn blocks(count: i64) -> i64 {
    count * BLOCK as i64
}

pub struct Sandbox {
    root: TempDir,
    tool_limit_secs: std::sync::atomic::AtomicU32,
}

impl Sandbox {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("create sandbox");
        let sandbox = Sandbox {
            root,
            tool_limit_secs: std::sync::atomic::AtomicU32::new(120),
        };
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

    /// How long clch waits for a tool's answer from now on.
    pub fn limit_tools_to(&self, seconds: u32) {
        self.tool_limit_secs
            .store(seconds, std::sync::atomic::Ordering::Relaxed);
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

    /// The folder clch keeps its own data in.
    pub fn data_dir(&self) -> PathBuf {
        if cfg!(windows) {
            self.local_app_data().join("clch")
        } else {
            self.home().join("Library/Application Support/clch")
        }
    }

    /// The file clch keeps its settings in.
    pub fn config_file(&self) -> PathBuf {
        if cfg!(windows) {
            self.app_data().join("clch").join("config.toml")
        } else {
            self.home()
                .join("Library/Application Support/clch/config.toml")
        }
    }

    /// Makes `dir` hold a `node_modules` of `count` blocks with its marker
    /// file beside it, and returns the `node_modules` folder.
    pub fn node_project(&self, dir: &Path, count: usize) -> PathBuf {
        self.write_file(&dir.join("package.json"), 1);
        let modules = dir.join("node_modules");
        self.write_file(&modules.join("dep/index.js"), count);
        modules
    }

    /// The npm cache folder npm uses when nothing configures it.
    pub fn default_npm_cache(&self) -> PathBuf {
        if cfg!(windows) {
            self.local_app_data().join("npm-cache")
        } else {
            self.home().join(".npm")
        }
    }

    /// Writes a new file of `count` filesystem blocks at `path`, creating its
    /// parents. Its length and its allocated size are both `blocks(count)`.
    pub fn write_file(&self, path: &Path, count: usize) {
        fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parent");
        fs::write(path, vec![b'x'; count * BLOCK]).expect("write file");
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
        Command::from_std(self.clch_process())
    }

    /// The same, as a plain process.
    pub fn clch_process(&self) -> std::process::Command {
        let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_clch"));
        cmd.env_clear()
            .env("PATH", self.bin())
            .env("HOME", self.home())
            .env("USERPROFILE", self.home())
            .env("LOCALAPPDATA", self.local_app_data())
            .env("APPDATA", self.app_data())
            .env(
                "CLCH_TOOL_TIMEOUT_SECS",
                self.tool_limit_secs
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .to_string(),
            )
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

/// What a fake tool does for one command line.
pub struct Reply {
    pub unix: String,
    pub windows: String,
}

pub fn prints(path: &Path) -> Reply {
    Reply {
        unix: format!("echo '{}'", path.display()),
        windows: format!("echo {}", path.display()),
    }
}

pub fn says(text: &str) -> Reply {
    Reply {
        unix: format!("echo {text}"),
        windows: format!("echo {text}"),
    }
}

pub fn removes(path: &Path) -> Reply {
    let batch = path.display().to_string().replace('/', "\\");
    Reply {
        unix: format!("rm -rf '{}'", path.display()),
        windows: format!(
            "if exist \"{batch}\\\" (rmdir /s /q \"{batch}\") else (del /q \"{batch}\")"
        ),
    }
}

/// A fake tool answering exactly these command lines, and nothing otherwise.
pub fn tool(sb: &Sandbox, name: &str, replies: Vec<(&str, Reply)>) {
    let mut unix = String::from("case \"$*\" in\n");
    let mut windows =
        String::from("set \"ARGS=%*\"\r\nif defined ARGS set \"ARGS=%ARGS:\"=%\"\r\n");
    let mut labels = String::new();
    for (i, (args, reply)) in replies.iter().enumerate() {
        unix.push_str(&format!("  '{args}') {} ;;\n", reply.unix));
        windows.push_str(&format!("if \"%ARGS%\"==\"{args}\" goto c{i}\r\n"));
        labels.push_str(&format!(":c{i}\r\n{}\r\nexit /b 0\r\n", reply.windows));
    }
    unix.push_str("esac");
    windows.push_str("exit /b 0\r\n");
    windows.push_str(&labels);
    sb.fake_tool(name, &unix, &windows);
}

/// Seam 2: `clch dashboard` running in a sandbox, stopped when dropped.
pub struct Dashboard {
    child: std::process::Child,
    pub url: String,
    pub port: u16,
    pub token: String,
}

/// An HTTP response from the dashboard.
pub struct Response {
    pub status: u16,
    pub headers: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).expect("response is JSON")
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

impl Dashboard {
    /// Starts `clch dashboard <args>` and reads the URL it prints.
    pub fn start(sb: &Sandbox, args: &[&str]) -> Dashboard {
        use std::io::BufRead;
        let mut child = sb
            .clch_process()
            .arg("dashboard")
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start dashboard");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut line = String::new();
        std::io::BufReader::new(stdout)
            .read_line(&mut line)
            .expect("read the URL");
        let url = line.trim().to_string();
        let rest = url
            .strip_prefix("http://127.0.0.1:")
            .unwrap_or_else(|| panic!("a URL on 127.0.0.1, not {url:?}"));
        let (port, token) = rest
            .split_once("/?token=")
            .unwrap_or_else(|| panic!("a URL carrying a token, not {url:?}"));
        let port = port.parse().expect("a port");
        let token = token.to_string();
        Dashboard {
            child,
            url,
            port,
            token,
        }
    }

    pub fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Sends `method path` with exactly these headers, plus `Host` when none is
    /// given.
    pub fn send(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Response {
        use std::io::{Read, Write};
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", self.port)).expect("connect to dashboard");
        let mut request = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
        if !headers.iter().any(|(name, _)| *name == "Host") {
            request.push_str(&format!("Host: {}\r\n", self.host()));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
        stream.write_all(request.as_bytes()).expect("send request");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("read response");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("a complete response");
        let headers = String::from_utf8_lossy(&raw[..split]).into_owned();
        let status = headers
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("a status");
        Response {
            status,
            headers,
            body: raw[split + 4..].to_vec(),
        }
    }

    /// A request carrying this run's token.
    pub fn api(&self, method: &str, path: &str, body: &str) -> Response {
        let token = self.token.clone();
        self.send(method, path, &[("X-Clch-Token", token.as_str())], body)
    }

    pub fn wait_for_exit(&mut self) -> Option<std::process::ExitStatus> {
        for _ in 0..200 {
            if let Some(status) = self.child.try_wait().expect("poll dashboard") {
                return Some(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        None
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for Dashboard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
