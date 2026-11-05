//! Cleaning: a plan built from a scan, and carrying it out.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::catalog::{self, Deletable, Kind, ProjectArtifactKind, ToolCacheKind};
use crate::discover;
use crate::env::{Env, ToolAnswer};
use crate::scan::{self, Reclaimable, Scan, Scope, ToolNote, UNKNOWN_DISK};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Remove only what nothing currently uses.
    Unused,
    /// Remove everything, in use or not.
    Full,
}

#[derive(Debug, Serialize)]
pub struct PlanEntry {
    pub id: String,
    pub name: String,
    pub cleanable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The space the cleanup will free, where it can be known beforehand: for
    /// a full cleanup, the reclaimable's size.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_bytes: Option<u64>,
    /// False when part of the reclaimable could not be read, making
    /// `expected_bytes` a lower bound.
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What a person should know before this runs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    /// When this project artifact's project last changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified_ms: Option<i64>,
    #[serde(skip)]
    method: Option<Method>,
}

/// How a plan entry is carried out.
#[derive(Debug, Clone)]
enum Method {
    /// The tool, its arguments, and the folder to run it in.
    Native(&'static str, &'static [&'static str], Option<PathBuf>),
    Direct(&'static ProjectArtifactKind),
    /// Delete what the tool's docs allow of a tool cache.
    DeleteCache(Deletable),
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub level: Level,
    pub plan: Vec<PlanEntry>,
    /// Project artifacts left out because nothing selected them.
    pub not_selected: usize,
    /// The most this plan can free, adding up what can be known beforehand.
    pub expected_total_bytes: u64,
    /// True when no scan roots are set, so no project artifact was looked for.
    pub project_artifacts_skipped: bool,
    /// Tools on `PATH` whose numbers are missing from the scan behind this, and
    /// why.
    pub tool_notes: Vec<ToolNote>,
}

/// What must still be so for a plan to be the one a person saw.
pub type PlanSignature = Vec<(String, Option<PathBuf>, Option<String>, bool, Option<i64>)>;

impl Plan {
    /// Each entry's identity, action and project age.
    pub fn signature(&self) -> PlanSignature {
        self.plan
            .iter()
            .map(|e| {
                (
                    e.id.clone(),
                    e.path.clone(),
                    e.action.clone(),
                    e.cleanable,
                    e.last_modified_ms,
                )
            })
            .collect()
    }

    /// Whether anything here is a tool cache, the only thing the cleanup level
    /// changes.
    pub fn uses_level(&self) -> bool {
        self.plan
            .iter()
            .any(|e| !matches!(e.method, Some(Method::Direct(_))))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Cleaned,
    /// Not attempted; `reason` says why.
    Skipped,
    /// Attempted and the tool reported failure; `output` holds what it printed.
    Failed,
}

#[derive(Debug, Serialize)]
pub struct CleanResult {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub status: Status,
    /// How much this reclaimable shrank. A file another reclaimable also holds
    /// counts here though it stays on disk; `DiskFreed` and `Outcome` do not
    /// count it.
    pub freed_bytes: u64,
    /// False when a measurement behind `freed_bytes` was a lower bound.
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Outcome {
    /// What a cleanup was attempted on, by id and path.
    #[serde(skip)]
    pub attempted: Vec<(String, PathBuf)>,
    pub level: Level,
    /// Project artifacts left out because nothing selected them.
    pub not_selected: usize,
    pub results: Vec<CleanResult>,
    pub disks: Vec<DiskFreed>,
    pub freed_bytes: u64,
    /// False when a measurement behind `freed_bytes` was a lower bound.
    pub complete: bool,
    /// What a person must be told because of what was cleaned.
    pub notes: Vec<String>,
    /// Tools on `PATH` whose numbers are missing from a scan behind this, and why.
    pub tool_notes: Vec<ToolNote>,
}

/// What a cleanup freed on one disk.
#[derive(Debug, Serialize)]
pub struct DiskFreed {
    pub disk: String,
    pub freed_bytes: u64,
    /// False when a measurement behind `freed_bytes` was a lower bound.
    pub complete: bool,
}

impl Outcome {
    pub fn any_failed(&self) -> bool {
        self.results.iter().any(|r| r.status == Status::Failed)
    }
}

/// A name in a selection that cannot be acted on.
#[derive(Debug)]
pub enum BadName {
    /// Neither a kind of reclaimable nor the path of one.
    Unknown(String),
    /// A kind of project artifact, and also a folder where clch was started.
    Ambiguous(String),
}

/// What to clean. With no `names`, every tool cache; with `older_than_days` and
/// no `names`, the project artifacts of that age instead. With `exact`, those
/// rows of the scan and nothing else.
#[derive(Debug, Default)]
pub struct Selection {
    /// Kinds of reclaimable by id, and project artifacts by path.
    pub names: Vec<String>,
    pub older_than_days: Option<u64>,
    /// Rows of the scan, by id and path.
    pub exact: Vec<(String, PathBuf)>,
}

/// The plan for cleaning what `selection` picks out of `scan`.
pub fn plan(env: &Env, scan: &Scan, selection: &Selection, level: Level) -> Result<Plan, BadName> {
    let mut seen = HashSet::new();
    let names: Vec<&String> = selection.names.iter().filter(|n| seen.insert(*n)).collect();
    let is_kind = |name: &str| {
        catalog::tool_cache(name).is_some() || catalog::project_artifact(name).is_some()
    };
    let path_of = |name: &str| env.absolute(Path::new(name));
    let names_path =
        |name: &str, path: &Path| path.is_absolute() && same_folder(&path_of(name), path);
    for name in &names {
        let is_path = scan.reclaimables.iter().any(|r| names_path(name, &r.path));
        if !is_kind(name) && !is_path {
            return Err(BadName::Unknown((*name).clone()));
        }
        if let Some(kind) = catalog::project_artifact(name)
            && (path_of(name).is_dir()
                || discover::inside_project_of(kind, &env.absolute(Path::new("."))))
        {
            return Err(BadName::Ambiguous((*name).clone()));
        }
    }
    let chosen = |r: &Reclaimable| {
        if !selection.exact.is_empty() {
            return selection
                .exact
                .iter()
                .any(|(id, path)| *id == r.id && *path == r.path);
        }
        let named = if names.is_empty() {
            match selection.older_than_days {
                Some(_) => r.kind == Kind::ProjectArtifact,
                None => r.kind == Kind::ToolCache && r.size_bytes > 0,
            }
        } else {
            names
                .iter()
                .any(|n| n.as_str() == r.id || names_path(n, &r.path))
        };
        named
            && selection
                .older_than_days
                .is_none_or(|days| scan.passes_age(r, days))
    };
    let mut entries: Vec<PlanEntry> = scan
        .reclaimables
        .iter()
        .filter(|r| chosen(r))
        .map(|r| entry(env, r, level))
        .collect();
    let not_selected = scan
        .reclaimables
        .iter()
        .filter(|r| r.kind == Kind::ProjectArtifact && !chosen(r))
        .count();
    for name in names.iter().filter(|n| is_kind(n)) {
        if !scan.reclaimables.iter().any(|r| &r.id == *name) {
            let kind_name = catalog::tool_cache(name)
                .map(|k| k.name)
                .or(catalog::project_artifact(name).map(|k| k.name))
                .unwrap_or(name.as_str());
            let silent = catalog::tool_cache(name)
                .and_then(|k| scan.note_for(k.cleanup.tool).map(|note| (k, note)));
            let reason = if let Some((kind, note)) = silent {
                format!("{}: {}", kind.cleanup.tool, note.reason)
            } else if catalog::project_artifact(name).is_some() && scan.project_artifacts_skipped {
                "no scan roots are set; add one with `clch roots add <folder>`".to_string()
            } else {
                "not found on this machine".to_string()
            };
            entries.push(not_cleanable(name, kind_name, reason));
        }
    }
    Ok(Plan {
        level,
        expected_total_bytes: entries.iter().filter_map(|e| e.expected_bytes).sum(),
        plan: entries,
        not_selected,
        project_artifacts_skipped: scan.project_artifacts_skipped,
        tool_notes: scan.tool_notes.clone(),
    })
}

fn entry(env: &Env, r: &Reclaimable, level: Level) -> PlanEntry {
    if let Some(kind) = catalog::project_artifact(&r.id) {
        return match tracked_by_git(env, &r.path) {
            Ok(()) => PlanEntry {
                id: r.id.clone(),
                name: r.name.clone(),
                cleanable: true,
                action: Some("delete permanently".to_string()),
                expected_bytes: Some(r.size_bytes),
                complete: r.complete,
                reason: None,
                warning: artifact_warning(kind, &r.path),
                path: Some(r.path.clone()),
                last_modified_ms: r.last_modified_ms,
                method: Some(Method::Direct(kind)),
            },
            Err(reason) => PlanEntry {
                path: Some(r.path.clone()),
                last_modified_ms: r.last_modified_ms,
                ..not_cleanable(&r.id, &r.name, reason)
            },
        };
    }
    let kind: &ToolCacheKind = catalog::tool_cache(&r.id).expect("scan ids come from the catalog");
    let cleanup = &kind.cleanup;
    let args = match level {
        Level::Unused => cleanup.unused,
        Level::Full => cleanup.full,
    };
    let tool = std::iter::once(cleanup.tool)
        .chain(cleanup.also_named.iter().copied())
        .find(|name| env.find_tool(name).is_some());
    let no_command = args.is_none() || tool.is_none();
    if let (true, Level::Full, Some(deletable)) = (no_command, level, cleanup.deletable) {
        if let Err(reason) = what_to_delete(env, &r.path, deletable) {
            return PlanEntry {
                path: Some(r.path.clone()),
                ..not_cleanable(kind.id, kind.name, reason)
            };
        }
        return PlanEntry {
            id: kind.id.to_string(),
            name: kind.name.to_string(),
            cleanable: true,
            action: Some("delete permanently".to_string()),
            expected_bytes: Some(r.size_bytes),
            complete: r.complete,
            reason: None,
            warning: kind.delete_warning.map(str::to_string),
            method: Some(Method::DeleteCache(deletable)),
            path: Some(r.path.clone()),
            last_modified_ms: None,
        };
    }
    let Some(args) = args else {
        let reason = match level {
            _ if cleanup.unused.is_none()
                && cleanup.full.is_none()
                && cleanup.deletable.is_none() =>
            {
                format!(
                    "{} has no command for this, and its docs do not allow deleting it",
                    cleanup.tool
                )
            }
            Level::Unused => format!(
                "{} offers no cleanup of only unused data here; use --full to remove all of it",
                cleanup.tool
            ),
            Level::Full => format!("{} offers no full cleanup here", cleanup.tool),
        };
        return not_cleanable(kind.id, kind.name, reason);
    };
    let Some(tool) = tool else {
        let reason = if cleanup.deletable.is_some() {
            format!(
                "{} is not on PATH to remove only unused data; use --full to delete all of it",
                cleanup.tool
            )
        } else {
            format!(
                "{} is not on PATH, and its docs do not allow deleting this directly",
                cleanup.tool
            )
        };
        return not_cleanable(kind.id, kind.name, reason);
    };
    if let Some(below) = cleanup.below_version {
        let version = env.tool_value(tool, &["--version"]);
        if version
            .as_deref()
            .is_none_or(|v| version_at_least(v, below))
        {
            let this = version.map_or("its version could not be read".to_string(), |v| {
                format!("this is {v}")
            });
            let reason = format!("needs a {} older than {below}; {this}", cleanup.tool);
            return not_cleanable(kind.id, kind.name, reason);
        }
    }
    if let Some(min) = cleanup.min_version {
        let version = env.tool_value(tool, &["--version"]);
        match version.as_deref().map(|v| version_at_least(v, min)) {
            Some(true) => {}
            Some(false) => {
                let reason = format!(
                    "needs {} {min} or later; this is {}",
                    cleanup.tool,
                    version.unwrap_or_default()
                );
                return not_cleanable(kind.id, kind.name, reason);
            }
            None => {
                let reason = format!(
                    "could not read the {} version; {min} or later is needed",
                    cleanup.tool
                );
                return not_cleanable(kind.id, kind.name, reason);
            }
        }
    }
    let expected_bytes = match level {
        Level::Full if cleanup.full != cleanup.unused => Some(r.size_bytes),
        _ => None,
    };
    PlanEntry {
        id: kind.id.to_string(),
        name: kind.name.to_string(),
        cleanable: true,
        action: Some(format!("{tool} {}", args.join(" "))),
        expected_bytes,
        complete: r.complete,
        reason: None,
        warning: None,
        method: Some(Method::Native(tool, args, r.run_from.clone())),
        path: Some(r.path.clone()),
        last_modified_ms: None,
    }
}

/// What a person should know before this project artifact goes.
fn artifact_warning(kind: &ProjectArtifactKind, path: &Path) -> Option<String> {
    let workspace = path.parent()?.join("pnpm-workspace.yaml");
    (kind.id == "node_modules" && workspace.is_file()).then(|| {
        format!(
            "{} is a pnpm workspace root: its members break until you reinstall.",
            path.parent().unwrap_or(path).display()
        )
    })
}

fn not_cleanable(id: &str, name: &str, reason: String) -> PlanEntry {
    PlanEntry {
        id: id.to_string(),
        name: name.to_string(),
        cleanable: false,
        action: None,
        expected_bytes: None,
        complete: true,
        reason: Some(reason),
        warning: None,
        method: None,
        path: None,
        last_modified_ms: None,
    }
}

/// Whether dotted version `have` is `min` or later, comparing numeric parts;
/// anything after the first non-numeric part is ignored.
fn version_at_least(have: &str, min: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split('.')
            .map_while(|p| {
                let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            })
            .collect()
    };
    let (have, min) = (parts(have), parts(min));
    if have.is_empty() {
        return false;
    }
    for i in 0..have.len().max(min.len()) {
        let (h, m) = (
            have.get(i).copied().unwrap_or(0),
            min.get(i).copied().unwrap_or(0),
        );
        if h != m {
            return h > m;
        }
    }
    true
}

/// Carries out every cleanable entry of `plan`, each independently, and
/// measures the result against `before`, the scan the plan was built from. The
/// scan taken afterwards is returned as well.
pub fn execute(env: &Env, plan: Plan, before: &Scan, scope: &Scope) -> (Outcome, Scan) {
    let attempted: Vec<_> = plan
        .plan
        .into_iter()
        .map(|entry| {
            if let (Some(_), Some(action)) = (&entry.method, &entry.action) {
                eprintln!("{}: {action}", entry.name);
            }
            let run = entry.method.as_ref().map(|method| match method {
                Method::Native(tool, args, dir) => {
                    env.run_to_completion(tool, args, dir.as_deref())
                }
                Method::Direct(kind) => delete_artifact(entry.path.as_deref(), kind),
                Method::DeleteCache(deletable) => {
                    delete_cache(env, entry.path.as_deref(), deletable)
                }
            });
            (entry, run)
        })
        .collect();
    let after = scan::run(env, scope);

    let silent_after = |r: &Reclaimable| {
        catalog::tool_cache(&r.id).is_some_and(|k| after.note_for(k.cleanup.tool).is_some())
    };
    let cleaned_disks: BTreeSet<&str> = attempted
        .iter()
        .filter(|(_, run)| run.is_some())
        .filter_map(|(entry, _)| {
            let was = reclaimable_at(before, entry)?;
            Some(was.disk.as_deref().unwrap_or(UNKNOWN_DISK))
        })
        .collect();
    let disks: Vec<DiskFreed> = before
        .disks
        .iter()
        .filter(|d| cleaned_disks.contains(d.disk.as_str()))
        .map(|d| {
            let later = after.disks.iter().find(|a| a.disk == d.disk);
            let unseen: u64 = before
                .reclaimables
                .iter()
                .filter(|r| r.disk.as_deref().unwrap_or(UNKNOWN_DISK) == d.disk && silent_after(r))
                .map(|r| r.size_bytes)
                .sum();
            DiskFreed {
                disk: d.disk.clone(),
                freed_bytes: d
                    .reclaimable_bytes
                    .saturating_sub(later.map_or(0, |a| a.reclaimable_bytes))
                    .saturating_sub(unseen),
                complete: d.complete && later.is_none_or(|a| a.complete) && unseen == 0,
            }
        })
        .collect();

    let mut notes: Vec<String> = Vec::new();
    for (entry, _) in attempted
        .iter()
        .filter(|(_, run)| matches!(run, Some(Ok(()))))
    {
        let note = catalog::tool_cache(&entry.id)
            .and_then(|k| k.windows_note)
            .filter(|_| cfg!(windows));
        if let Some(note) = note
            && !notes.iter().any(|n| n == note)
        {
            notes.push(note.to_string());
        }
    }

    let attempted_paths = attempted
        .iter()
        .filter(|(_, run)| run.is_some())
        .filter_map(|(entry, _)| Some((entry.id.clone(), entry.path.clone()?)))
        .collect();
    let results = attempted
        .into_iter()
        .map(|(entry, run)| {
            let was = reclaimable_at(before, &entry);
            let now = was.and_then(|w| {
                after
                    .reclaimables
                    .iter()
                    .find(|a| a.id == w.id && a.path == w.path)
            });
            let attempted = run.is_some();
            let partly_deleted = matches!(entry.method, Some(Method::Direct(_)))
                && was.is_some_and(|w| now.is_none_or(|n| n.size_bytes < w.size_bytes));
            let unseen = was.is_some_and(&silent_after);
            let freed_bytes = match was {
                Some(_) if unseen => 0,
                Some(was) if attempted => was
                    .size_bytes
                    .saturating_sub(now.map_or(0, |n| n.size_bytes)),
                _ => 0,
            };
            CleanResult {
                id: entry.id,
                name: entry.name,
                path: entry.path,
                status: match &run {
                    None => Status::Skipped,
                    Some(Ok(())) => Status::Cleaned,
                    Some(Err(_)) => Status::Failed,
                },
                freed_bytes,
                complete: !attempted
                    || (!unseen
                        && was.is_none_or(|w| w.complete)
                        && now.is_none_or(|n| n.complete)),
                action: entry.action,
                reason: entry.reason,
                output: run.and_then(Result::err).map(|output| {
                    if partly_deleted {
                        format!(
                            "{output}. Part of it was deleted: reinstall or rebuild this project."
                        )
                    } else {
                        output
                    }
                }),
            }
        })
        .collect();

    let freed_bytes = disks.iter().map(|d| d.freed_bytes).sum();
    let complete = disks.iter().all(|d| d.complete);
    let mut tool_notes = plan.tool_notes;
    for note in &after.tool_notes {
        if !tool_notes.contains(note) {
            tool_notes.push(note.clone());
        }
    }
    let outcome = Outcome {
        attempted: attempted_paths,
        level: plan.level,
        not_selected: plan.not_selected,
        results,
        disks,
        freed_bytes,
        complete,
        notes,
        tool_notes,
    };
    (outcome, after)
}

/// Whether two paths lead to the same place, however they are spelled.
fn same_folder(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Checks that git tracks nothing in the folder at `path`. Without git, or
/// outside a repository, nothing is tracked. A git that will not answer, or that
/// fails inside a repository, counts against deleting. The repository's own `core.fsmonitor` is turned off for the
/// question.
fn tracked_by_git(env: &Env, path: &Path) -> Result<(), String> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    let (Some(name), Some(parent)) = (name, path.parent()) else {
        return Ok(());
    };
    match env.ask_tool(
        "git",
        &["-c", "core.fsmonitor=false", "ls-files", "--", &name],
        Some(parent),
    ) {
        ToolAnswer::Absent => Ok(()),
        ToolAnswer::Failed if !discover::inside_repository(parent) => Ok(()),
        ToolAnswer::Failed => Err("git could not say whether it tracks this folder".into()),
        ToolAnswer::TimedOut => Err("git did not answer whether it tracks this folder".into()),
        ToolAnswer::Printed(files) if files.trim().is_empty() => Ok(()),
        ToolAnswer::Printed(_) => Err("git tracks files in this folder".into()),
    }
}

/// Permanently deletes what `deletable` allows of the tool cache at `path`.
fn delete_cache(env: &Env, path: Option<&Path>, deletable: &Deletable) -> Result<(), String> {
    let path = path.ok_or("no path to delete")?;
    for folder in what_to_delete(env, path, *deletable)? {
        std::fs::remove_dir_all(folder).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The folders to delete of the tool cache at `path`, once it has been checked
/// to be one. `Err` says why it will not be touched.
fn what_to_delete(env: &Env, path: &Path, deletable: Deletable) -> Result<Vec<PathBuf>, String> {
    let shown = path.display();
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(format!("{shown} is not a folder"));
    }
    if env.holds_home(path) {
        return Err(format!("{shown} holds the home folder"));
    }
    let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(path)
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let folders = match deletable {
        Deletable::FolderHolding { holds, needs } => {
            let missing = needs
                .iter()
                .find(|need| !entries.iter().any(|e| e.file_name() == **need));
            if let Some(missing) = missing {
                return Err(format!(
                    "{shown} does not look like this cache: it has no {missing}"
                ));
            }
            let stray = entries.iter().find(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                !holds.contains(&name.as_ref()) && !IGNORED_ENTRIES.contains(&name.as_ref())
            });
            if let Some(stray) = stray {
                return Err(format!(
                    "{shown} does not look like this cache: it holds {}",
                    stray.file_name().to_string_lossy()
                ));
            }
            vec![path.to_path_buf()]
        }
        Deletable::VersionedStores => {
            let stores: Vec<PathBuf> = entries
                .iter()
                .filter(|entry| {
                    let name = entry.file_name();
                    let versioned = name
                        .to_string_lossy()
                        .strip_prefix('v')
                        .is_some_and(|n| n.parse::<u32>().is_ok());
                    versioned && entry.path().join("files").is_dir()
                })
                .map(|entry| entry.path())
                .collect();
            if stores.is_empty() {
                return Err(format!(
                    "{shown} does not look like this cache: no store in it"
                ));
            }
            stores
        }
        Deletable::FolderNamed(name) => {
            if path.file_name().is_none_or(|n| n != name) {
                return Err(format!(
                    "{shown} does not look like this cache: it is not called {name}"
                ));
            }
            tracked_by_git(env, path)?;
            vec![path.to_path_buf()]
        }
    };
    if let Some(mounted) = folders.iter().find(|f| discover::holds_another_disk(f)) {
        return Err(format!(
            "another disk is mounted inside {}",
            mounted.display()
        ));
    }
    Ok(folders)
}

/// Files an operating system leaves in any folder it has shown.
const IGNORED_ENTRIES: [&str; 2] = [".DS_Store", "Thumbs.db"];

/// Permanently deletes a project artifact, once more checking that it is one.
/// Links inside it are removed, not followed.
fn delete_artifact(path: Option<&Path>, kind: &ProjectArtifactKind) -> Result<(), String> {
    let path = path.ok_or("no path to delete")?;
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() {
        return Err(format!("{} is no longer a folder", path.display()));
    }
    if !discover::is_artifact(kind, path) {
        return Err(format!(
            "{} no longer looks like a {}",
            path.display(),
            kind.name
        ));
    }
    if discover::holds_another_disk(path) {
        return Err(format!("another disk is mounted inside {}", path.display()));
    }
    std::fs::remove_dir_all(path).map_err(|e| e.to_string())
}

fn reclaimable_at<'a>(scan: &'a Scan, entry: &PlanEntry) -> Option<&'a Reclaimable> {
    let path = entry.path.as_ref()?;
    scan.reclaimables
        .iter()
        .find(|r| r.id == entry.id && &r.path == path)
}

#[cfg(test)]
mod tests {
    use super::{catalog, delete_artifact, version_at_least};

    #[test]
    fn a_folder_that_lost_its_evidence_since_the_plan_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
        let target = dir.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let kind = catalog::project_artifact("target").unwrap();

        assert!(delete_artifact(Some(&target), kind).is_err());
        assert!(target.exists());
    }

    #[test]
    fn a_folder_that_lost_its_marker_since_the_plan_is_not_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let modules = dir.path().join("node_modules");
        std::fs::create_dir(&modules).unwrap();
        let kind = catalog::project_artifact("node_modules").unwrap();

        let result = delete_artifact(Some(&modules), kind);

        assert!(result.unwrap_err().contains("no longer looks like"));
        assert!(modules.exists());
    }

    #[test]
    fn versions_compare_by_numeric_parts() {
        assert!(version_at_least("11.2.0", "11.2.0"));
        assert!(version_at_least("11.10.0", "11.2.0"));
        assert!(version_at_least("12.0.0", "11.2.0"));
        assert!(version_at_least("v11.6.2", "11.2.0"));
        assert!(version_at_least("11.2.0-pre.1", "11.2.0"));
        assert!(!version_at_least("11.1.9", "11.2.0"));
        assert!(!version_at_least("10.9.2", "11.2.0"));
        assert!(!version_at_least("not a version", "11.2.0"));
    }
}
