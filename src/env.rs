//! The machine clch runs on: per-user folders and the tools on `PATH`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::process::ToolProcess;

/// How long a tool may take to answer before clch gives up on it, unless
/// `CLCH_TOOL_TIMEOUT_SECS` says otherwise.
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

type ToolCall = (String, Vec<String>, Option<PathBuf>);

/// Per-user folders and tool lookup, read once from the process environment.
pub struct Env {
    home: Option<PathBuf>,
    local_app_data: Option<PathBuf>,
    app_data: Option<PathBuf>,
    path: Option<OsString>,
    cwd: PathBuf,
    answers: RefCell<HashMap<ToolCall, Option<String>>>,
    reports: RefCell<HashMap<ToolCall, ToolAnswer>>,
    progress: RefCell<Option<Progress>>,
    narration: RefCell<Option<NarrationSink>>,
    tool_timeout: Duration,
}

/// Takes one line saying what a scan is doing, to be kept.
pub type NarrationSink = Box<dyn Fn(&str)>;

/// Where word of a long run goes.
struct Progress {
    started: Instant,
    /// How long a run stays quiet before it says anything.
    quiet_for: Duration,
    last: Cell<Option<Instant>>,
    sink: ProgressSink,
}

/// Takes what a scan is doing, or `None` once it has finished.
pub type ProgressSink = Box<dyn Fn(Option<&str>)>;

/// The least time between two passing messages.
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

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
            app_data: var("APPDATA").map(PathBuf::from),
            path: var("PATH"),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            answers: RefCell::new(HashMap::new()),
            reports: RefCell::new(HashMap::new()),
            progress: RefCell::new(None),
            narration: RefCell::new(None),
            tool_timeout: std::env::var("CLCH_TOOL_TIMEOUT_SECS")
                .ok()
                .and_then(|secs| secs.parse().ok())
                .map_or(TOOL_TIMEOUT, Duration::from_secs),
        }
    }

    pub fn home(&self) -> Option<&PathBuf> {
        self.home.as_ref()
    }

    /// `%LOCALAPPDATA%` on Windows.
    pub fn local_app_data(&self) -> Option<&PathBuf> {
        self.local_app_data.as_ref()
    }

    /// The folder clch keeps its own data in: `~/Library/Application
    /// Support/clch` on macOS, `%LOCALAPPDATA%\\clch` on Windows, whose default
    /// `~\\AppData\\Local` stands in when the variable is not set.
    pub fn data_dir(&self) -> Option<PathBuf> {
        if cfg!(windows) {
            self.local_app_data
                .clone()
                .or_else(|| self.home.as_ref().map(|h| h.join("AppData").join("Local")))
                .map(|d| d.join("clch"))
        } else if cfg!(target_os = "macos") {
            self.home
                .as_ref()
                .map(|h| h.join("Library/Application Support/clch"))
        } else {
            self.home.as_ref().map(|h| h.join(".local/share/clch"))
        }
    }

    /// The folder clch keeps its settings in: the same folder as its data on
    /// macOS, `%APPDATA%\\clch` on Windows, whose default
    /// `~\\AppData\\Roaming` stands in when the variable is not set.
    pub fn config_dir(&self) -> Option<PathBuf> {
        if cfg!(windows) {
            self.app_data
                .clone()
                .or_else(|| {
                    self.home
                        .as_ref()
                        .map(|h| h.join("AppData").join("Roaming"))
                })
                .map(|d| d.join("clch"))
        } else {
            self.data_dir()
        }
    }

    /// Folders in the home folder that the operating system keeps for
    /// applications.
    pub fn application_folders(&self) -> Vec<PathBuf> {
        let name = if cfg!(windows) { "AppData" } else { "Library" };
        self.home.iter().map(|h| h.join(name)).collect()
    }

    /// `path` made absolute against the folder clch was started in, with `.` and
    /// `..` resolved by name.
    pub fn absolute(&self, path: &Path) -> PathBuf {
        let mut absolute = PathBuf::new();
        for part in self.cwd.join(path).components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    absolute.pop();
                }
                other => absolute.push(other),
            }
        }
        absolute
    }

    /// The full path of `tool` on `PATH`, honouring `PATHEXT` on Windows.
    pub fn find_tool(&self, tool: &str) -> Option<PathBuf> {
        which::which_in(tool, self.path.as_ref(), &self.cwd).ok()
    }

    /// The value of an environment variable, when it is set and not empty.
    pub fn var(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name).filter(|v| !v.is_empty())
    }

    /// Whether `path` is the home folder or holds it.
    pub fn holds_home(&self, path: &Path) -> bool {
        let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        self.home
            .as_deref()
            .is_some_and(|home| real(home).starts_with(real(path)))
    }

    /// The single line `tool args` prints, run from the home folder.
    pub fn tool_value(&self, tool: &str, args: &[&str]) -> Option<String> {
        self.tool_value_from(tool, args, self.home.as_deref())
    }

    /// The single line `tool args` prints, run from `dir`.
    ///
    /// `None` when the tool is absent, fails, times out, or prints anything
    /// other than exactly one non-empty line. Each distinct call runs once per
    /// `Env`.
    pub fn tool_value_from(&self, tool: &str, args: &[&str], dir: Option<&Path>) -> Option<String> {
        let key = (
            tool.to_string(),
            args.iter().map(|a| a.to_string()).collect(),
            dir.map(Path::to_path_buf),
        );
        if let Some(answer) = self.answers.borrow().get(&key) {
            return answer.clone();
        }
        let answer = match self.ask_tool(tool, args, dir) {
            ToolAnswer::Printed(out) => single_line(&out),
            _ => None,
        };
        self.answers.borrow_mut().insert(key, answer.clone());
        answer
    }

    /// The absolute path `tool args` prints, run from `dir`; anything else it
    /// prints is not believed.
    pub fn tool_path(&self, tool: &str, args: &[&str], dir: Option<&Path>) -> Option<PathBuf> {
        self.tool_value_from(tool, args, dir)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    }

    /// Runs `tool args` until it finishes, with no time limit, from `dir` or else
    /// the home folder. `Err` carries what the tool printed, or why it could not
    /// start.
    pub fn run_to_completion(
        &self,
        tool: &str,
        args: &[&str],
        dir: Option<&Path>,
    ) -> Result<(), String> {
        let program = self
            .find_tool(tool)
            .ok_or_else(|| format!("{tool} is not on PATH"))?;
        let mut command = self.command(program, args);
        command
            .envs(settings_for(tool).iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            command.current_dir(dir);
        }
        let output = ToolProcess::spawn(&mut command, false)
            .and_then(ToolProcess::wait_with_output)
            .map_err(|e| format!("could not run {tool}: {e}"))?;
        if output.status.success() {
            return Ok(());
        }
        let mut printed = String::from_utf8_lossy(&output.stdout).into_owned();
        printed.push_str(&String::from_utf8_lossy(&output.stderr));
        let printed = printed.trim();
        Err(if printed.is_empty() {
            format!("{tool} printed nothing and {}", output.status)
        } else {
            printed.to_string()
        })
    }

    fn command(&self, program: PathBuf, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null());
        if let Some(home) = self.home.as_ref().filter(|h| h.is_dir()) {
            command.current_dir(home);
        }
        command
    }

    /// Sends word of what a scan is doing to `sink`, once `quiet_for` has passed.
    pub fn report_progress_to(&self, quiet_for: Duration, sink: ProgressSink) {
        *self.progress.borrow_mut() = Some(Progress {
            started: Instant::now(),
            quiet_for,
            last: Cell::new(None),
            sink,
        });
    }

    /// Sends each line a scan narrates to `sink`.
    pub fn narrate_to(&self, sink: NarrationSink) {
        *self.narration.borrow_mut() = Some(sink);
    }

    /// Says one line about the scan, where narration is on.
    pub fn narrate(&self, line: impl FnOnce() -> String) {
        if let Some(sink) = self.narration.borrow().as_ref() {
            sink(&line());
        }
    }

    /// Says what is starting now, which may take a while.
    pub fn progress_step(&self, message: impl FnOnce() -> String) {
        if let Some(progress) = self.progress.borrow().as_ref()
            && progress.started.elapsed() >= progress.quiet_for
        {
            progress.last.set(Some(Instant::now()));
            (progress.sink)(Some(&message()));
        }
    }

    /// Says what is passing by; most of these are dropped.
    pub fn progress_passing(&self, message: impl FnOnce() -> String) {
        if let Some(progress) = self.progress.borrow().as_ref()
            && progress.started.elapsed() >= progress.quiet_for
            && progress
                .last
                .get()
                .is_none_or(|t| t.elapsed() >= PROGRESS_EVERY)
        {
            progress.last.set(Some(Instant::now()));
            (progress.sink)(Some(&message()));
        }
    }

    /// Says the scan has finished.
    pub fn progress_done(&self) {
        if let Some(progress) = self.progress.borrow().as_ref()
            && progress.last.take().is_some()
        {
            (progress.sink)(None);
        }
    }

    /// What `tool args` reports about the present state of things, from the home
    /// folder. Each distinct call runs once until `forget_reports`.
    pub fn tool_report(&self, tool: &str, args: &[&str]) -> ToolAnswer {
        let key = (
            tool.to_string(),
            args.iter().map(|a| a.to_string()).collect(),
            None,
        );
        if let Some(answer) = self.reports.borrow().get(&key) {
            return answer.clone();
        }
        let answer = self.ask_tool(tool, args, self.home.as_deref());
        self.reports.borrow_mut().insert(key, answer.clone());
        answer
    }

    /// Makes the next `tool_report` ask again.
    pub fn forget_reports(&self) {
        self.reports.borrow_mut().clear();
    }

    /// Asks `tool args` a question from the folder `dir`, within the time limit.
    pub fn ask_tool(&self, tool: &str, args: &[&str], dir: Option<&Path>) -> ToolAnswer {
        let Some(program) = self.find_tool(tool) else {
            return ToolAnswer::Absent;
        };
        let mut command = Command::new(program);
        command
            .args(args)
            .envs(settings_for(tool).iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(dir) = dir.filter(|d| d.is_dir()) {
            command.current_dir(dir);
        }
        let Ok(mut process) = ToolProcess::spawn(&mut command, true) else {
            return ToolAnswer::Failed;
        };
        let stdout = process.child().stdout.take();
        let (printed, reading) = mpsc::channel();
        std::thread::spawn(move || {
            let mut out = String::new();
            if let Some(mut stdout) = stdout {
                let _ = stdout.read_to_string(&mut out);
            }
            let _ = printed.send(out);
        });
        let deadline = Instant::now() + self.tool_timeout;
        let status = loop {
            match process.child().try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    process.kill_tree();
                    return ToolAnswer::TimedOut;
                }
            }
        };
        let left = deadline.saturating_duration_since(Instant::now());
        match reading.recv_timeout(left) {
            Ok(out) if status.success() => ToolAnswer::Printed(out),
            Ok(_) => ToolAnswer::Failed,
            Err(_) => {
                process.kill_tree();
                ToolAnswer::TimedOut
            }
        }
    }
}

/// Environment variables a tool runs with, so that asking it where its cache is
/// does not make it download anything.
fn settings_for(tool: &str) -> &'static [(&'static str, &'static str)] {
    match tool {
        "go" => &[("GOTOOLCHAIN", "local")],
        "npm" | "pnpm" | "yarn" => &[("COREPACK_ENABLE_NETWORK", "0")],
        _ => &[],
    }
}

/// What came of asking a tool a question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolAnswer {
    Absent,
    /// It could not start, or it exited with a failure.
    Failed,
    TimedOut,
    Printed(String),
}

fn single_line(output: &str) -> Option<String> {
    let mut lines = output.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    lines.next().is_none().then(|| first.to_string())
}
