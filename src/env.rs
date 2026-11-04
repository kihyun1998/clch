//! The machine clch runs on: per-user folders and the tools on `PATH`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a tool may take to answer before clch gives up on it.
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

type ToolCall = (String, Vec<String>);

/// Per-user folders and tool lookup, read once from the process environment.
pub struct Env {
    home: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    path: Option<OsString>,
    cwd: PathBuf,
    answers: RefCell<HashMap<ToolCall, Option<String>>>,
}

impl Env {
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
        let home = if cfg!(windows) {
            var("USERPROFILE")
        } else {
            var("HOME")
        };
        Env {
            home: home.map(PathBuf::from),
            local_app_data: var("LOCALAPPDATA").map(PathBuf::from),
            path: var("PATH"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            answers: RefCell::new(HashMap::new()),
        }
    }

    pub fn home(&self) -> Option<&PathBuf> {
        self.home.as_ref()
    }

    /// `%LOCALAPPDATA%` on Windows.
    pub fn local_app_data(&self) -> Option<&PathBuf> {
        self.local_app_data.as_ref()
    }

    /// The full path of `tool` on `PATH`, honouring `PATHEXT` on Windows.
    pub fn find_tool(&self, tool: &str) -> Option<PathBuf> {
        which::which_in(tool, self.path.as_ref(), &self.cwd).ok()
    }

    /// The single line `tool args` prints, run from the home folder.
    ///
    /// `None` when the tool is absent, fails, times out, or prints anything
    /// other than exactly one non-empty line. Each distinct call runs once per
    /// `Env`.
    pub fn tool_value(&self, tool: &str, args: &[&str]) -> Option<String> {
        let key = (
            tool.to_string(),
            args.iter().map(|a| a.to_string()).collect(),
        );
        if let Some(answer) = self.answers.borrow().get(&key) {
            return answer.clone();
        }
        let answer = self.run_tool(tool, args).and_then(|out| single_line(&out));
        self.answers.borrow_mut().insert(key, answer.clone());
        answer
    }

    fn run_tool(&self, tool: &str, args: &[&str]) -> Option<String> {
        let program = self.find_tool(tool)?;
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(home) = self.home.as_ref().filter(|h| h.is_dir()) {
            command.current_dir(home);
        }
        let mut child = command.spawn().ok()?;
        let deadline = Instant::now() + TOOL_TIMEOUT;
        let status = loop {
            match child.try_wait().ok()? {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        if !status.success() {
            return None;
        }
        let mut out = String::new();
        child.stdout.take()?.read_to_string(&mut out).ok()?;
        Some(out)
    }
}

fn single_line(output: &str) -> Option<String> {
    let mut lines = output.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    lines.next().is_none().then(|| first.to_string())
}
