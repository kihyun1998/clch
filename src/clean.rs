//! Cleaning: a plan built from a scan, and carrying it out.

use std::collections::BTreeSet;

use serde::Serialize;

use crate::catalog::{self, ToolCacheKind};
use crate::env::Env;
use crate::scan::{self, Reclaimable, Scan};

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
    #[serde(skip)]
    command: Option<(&'static str, &'static [&'static str])>,
    #[serde(skip)]
    path: Option<std::path::PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct Plan {
    pub level: Level,
    pub plan: Vec<PlanEntry>,
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
    pub level: Level,
    pub results: Vec<CleanResult>,
    pub disks: Vec<DiskFreed>,
    pub freed_bytes: u64,
    /// False when a measurement behind `freed_bytes` was a lower bound.
    pub complete: bool,
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

/// An id that names no kind of reclaimable.
#[derive(Debug)]
pub struct UnknownId(pub String);

/// The plan for cleaning `ids` (every reclaimable in `scan` when empty).
pub fn plan(env: &Env, scan: &Scan, ids: &[String], level: Level) -> Result<Plan, UnknownId> {
    if let Some(unknown) = ids.iter().find(|id| catalog::tool_cache(id).is_none()) {
        return Err(UnknownId(unknown.clone()));
    }
    let mut ids = ids.to_vec();
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    let chosen = |r: &&Reclaimable| ids.is_empty() || ids.contains(&r.id);
    let mut entries: Vec<PlanEntry> = scan
        .reclaimables
        .iter()
        .filter(chosen)
        .map(|r| entry(env, r, level))
        .collect();
    for id in &ids {
        if !scan.reclaimables.iter().any(|r| &r.id == id) {
            let kind = catalog::tool_cache(id).expect("checked above");
            entries.push(not_cleanable(
                kind.id,
                kind.name,
                "not found on this machine".into(),
            ));
        }
    }
    Ok(Plan {
        level,
        plan: entries,
    })
}

fn entry(env: &Env, r: &Reclaimable, level: Level) -> PlanEntry {
    let kind: &ToolCacheKind = catalog::tool_cache(&r.id).expect("scan ids come from the catalog");
    let cleanup = &kind.cleanup;
    let args = match level {
        Level::Unused => cleanup.unused,
        Level::Full => cleanup.full,
    };
    let Some(args) = args else {
        let reason = match level {
            Level::Unused => format!(
                "{} offers no cleanup of only unused data here; use --full to remove all of it",
                cleanup.tool
            ),
            Level::Full => format!("{} offers no full cleanup here", cleanup.tool),
        };
        return not_cleanable(kind.id, kind.name, reason);
    };
    if env.find_tool(cleanup.tool).is_none() {
        return not_cleanable(
            kind.id,
            kind.name,
            format!("{} is not on PATH", cleanup.tool),
        );
    }
    if let Some(min) = cleanup.min_version {
        let version = env.tool_value(cleanup.tool, &["--version"]);
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
        Level::Full => Some(r.size_bytes),
        Level::Unused => None,
    };
    PlanEntry {
        id: kind.id.to_string(),
        name: kind.name.to_string(),
        cleanable: true,
        action: Some(format!("{} {}", cleanup.tool, args.join(" "))),
        expected_bytes,
        complete: r.complete,
        reason: None,
        command: Some((cleanup.tool, args)),
        path: Some(r.path.clone()),
    }
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
        command: None,
        path: None,
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
pub fn execute(env: &Env, plan: Plan, before: &Scan) -> (Outcome, Scan) {
    let attempted: Vec<_> = plan
        .plan
        .into_iter()
        .map(|entry| {
            let run = entry
                .command
                .map(|(tool, args)| env.run_to_completion(tool, args));
            (entry, run)
        })
        .collect();
    let after = scan::run(env);

    let cleaned_disks: BTreeSet<&str> = attempted
        .iter()
        .filter(|(_, run)| run.is_some())
        .filter_map(|(entry, _)| reclaimable_at(before, entry)?.disk.as_deref())
        .collect();
    let disks: Vec<DiskFreed> = before
        .disks
        .iter()
        .filter(|d| cleaned_disks.contains(d.disk.as_str()))
        .map(|d| {
            let later = after.disks.iter().find(|a| a.disk == d.disk);
            DiskFreed {
                disk: d.disk.clone(),
                freed_bytes: d
                    .reclaimable_bytes
                    .saturating_sub(later.map_or(0, |a| a.reclaimable_bytes)),
                complete: d.complete && later.is_none_or(|a| a.complete),
            }
        })
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
            let freed_bytes = match was {
                Some(was) if attempted => was
                    .size_bytes
                    .saturating_sub(now.map_or(0, |n| n.size_bytes)),
                _ => 0,
            };
            CleanResult {
                id: entry.id,
                name: entry.name,
                status: match &run {
                    None => Status::Skipped,
                    Some(Ok(())) => Status::Cleaned,
                    Some(Err(_)) => Status::Failed,
                },
                freed_bytes,
                complete: !attempted
                    || (was.is_none_or(|w| w.complete) && now.is_none_or(|n| n.complete)),
                action: entry.action,
                reason: entry.reason,
                output: run.and_then(Result::err),
            }
        })
        .collect();

    let freed_bytes = disks.iter().map(|d| d.freed_bytes).sum();
    let complete = disks.iter().all(|d| d.complete);
    let outcome = Outcome {
        level: plan.level,
        results,
        disks,
        freed_bytes,
        complete,
    };
    (outcome, after)
}

fn reclaimable_at<'a>(scan: &'a Scan, entry: &PlanEntry) -> Option<&'a Reclaimable> {
    let path = entry.path.as_ref()?;
    scan.reclaimables
        .iter()
        .find(|r| r.id == entry.id && &r.path == path)
}

#[cfg(test)]
mod tests {
    use super::version_at_least;

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
