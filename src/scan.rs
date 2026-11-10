//! A scan: every reclaimable present on this machine, with its size.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::catalog::{self, Kind, LocatedSize};
use crate::discover;
use crate::disks::{self, Disk};
use crate::env::Env;
use crate::size::{Measurement, Measurer};
use crate::snapshots::Gone;

#[derive(Debug, Serialize)]
pub struct Reclaimable {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
    /// The folder its tool must run in to act on it, when not the home folder.
    #[serde(skip)]
    pub run_from: Option<PathBuf>,
    /// True when its tool did not say where this is.
    #[serde(skip)]
    pub location_assumed: bool,
    pub size_bytes: u64,
    /// False when part of it could not be read, making `size_bytes` a lower
    /// bound.
    pub complete: bool,
    /// The disk it lives on, when the system can say.
    pub disk: Option<String>,
    /// When a file in this project artifact or the rest of its project last
    /// changed, in milliseconds since the Unix epoch.
    pub last_modified_ms: Option<i64>,
    /// False when part of the project could not be read, so its age is unknown.
    pub age_known: bool,
    /// Growth since the previous snapshot, when `change_state` is `Known`.
    pub change_bytes: Option<i64>,
    pub change_state: ChangeState,
}

/// What can be said about a reclaimable's change since the previous snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChangeState {
    /// `change_bytes` holds it.
    Known,
    /// The previous snapshot did not have this reclaimable.
    New,
    /// The store could not be used, or either measurement is a lower bound.
    Unknown,
}

/// One disk's free space and what the reclaimables on it occupy.
#[derive(Debug, Serialize)]
pub struct DiskTotal {
    /// Where it is mounted, or `UNKNOWN_DISK` when the system cannot say.
    pub disk: String,
    pub free_bytes: Option<u64>,
    pub reclaimable_bytes: u64,
    /// False when part of it could not be read, making `reclaimable_bytes` a
    /// lower bound.
    pub complete: bool,
}

/// The name of the total for reclaimables whose disk the system cannot name.
pub const UNKNOWN_DISK: &str = "unknown";

/// Adds a measured reclaimable to its disk's total.
fn add_to_total(
    totals: &mut BTreeMap<String, DiskTotal>,
    disk: Option<&Disk>,
    measured: &Measurement,
) {
    let name = disk.map_or(UNKNOWN_DISK, |d| d.name.as_str());
    let total = totals.entry(name.to_string()).or_insert(DiskTotal {
        disk: name.to_string(),
        free_bytes: disk.map(|d| d.free_bytes),
        reclaimable_bytes: 0,
        complete: true,
    });
    total.reclaimable_bytes += measured.unique_bytes;
    total.complete &= measured.complete;
}

/// What a scan knows of a reclaimable before measuring it.
#[derive(Default)]
struct Source {
    run_from: Option<PathBuf>,
    /// Its size as its tool reports it, instead of measuring.
    reported: Option<u64>,
    location_assumed: bool,
}

/// Which scan roots a scan searches for project artifacts.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    pub roots: Vec<PathBuf>,
    /// True when `roots` stand in for the configured ones for this run.
    pub one_off: bool,
}

/// Why a tool's numbers are missing from a scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolNote {
    pub tool: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Scan {
    pub reclaimables: Vec<Reclaimable>,
    /// Totals per disk, with each hard-linked file counted once.
    pub disks: Vec<DiskTotal>,
    /// What the previous snapshot had and this scan did not find.
    pub gone: Vec<Gone>,
    /// Whether this scan could be compared with the snapshot store.
    pub history_available: bool,
    pub scan_roots: Vec<PathBuf>,
    /// Scan roots that are not there, perhaps on a disk that is not attached.
    pub unavailable_roots: Vec<PathBuf>,
    /// Scan roots with a folder that could not be read, so what it holds is
    /// unknown.
    pub incomplete_roots: Vec<PathBuf>,
    /// True when no scan roots are set, so no project artifact was looked for.
    pub project_artifacts_skipped: bool,
    /// Tools on `PATH` whose numbers are missing from this scan, and why.
    pub tool_notes: Vec<ToolNote>,
    /// True when one-off scan roots replaced the configured ones.
    pub partial: bool,
    /// When the scan started, in milliseconds since the Unix epoch.
    #[serde(skip)]
    pub taken_at_ms: i64,
}

impl Scan {
    /// Why this scan has no numbers from `tool`, if it says.
    pub fn note_for(&self, tool: &str) -> Option<&ToolNote> {
        self.tool_notes.iter().find(|n| n.tool == tool)
    }

    /// Whether this scan searched `path` and could read all the way: its
    /// absence from the scan then means it is not there.
    pub fn covers(&self, path: &Path) -> bool {
        self.scan_roots.iter().any(|root| {
            path.starts_with(root)
                && !self.unavailable_roots.contains(root)
                && !self.incomplete_roots.contains(root)
        })
    }

    /// Whether `r` stays when only artifacts older than `days` are wanted. One
    /// whose age is not fully known does not.
    pub fn passes_age(&self, r: &Reclaimable, days: u64) -> bool {
        if r.kind != Kind::ProjectArtifact {
            return true;
        }
        let cutoff = self
            .taken_at_ms
            .saturating_sub((days as i64).saturating_mul(MS_PER_DAY));
        r.complete && r.age_known && r.last_modified_ms.is_some_and(|ms| ms < cutoff)
    }

    /// Keeps only the project artifacts older than `days`.
    pub fn retain_older_than(&mut self, days: u64) {
        let keep: Vec<bool> = self
            .reclaimables
            .iter()
            .map(|r| self.passes_age(r, days))
            .collect();
        let mut keep = keep.into_iter();
        self.reclaimables.retain(|_| keep.next().unwrap_or(false));
    }
}

pub const MS_PER_DAY: i64 = 24 * 60 * 60 * 1000;

pub fn run(env: &Env, scope: &Scope) -> Scan {
    env.forget_reports();
    let started = std::time::Instant::now();
    let taken_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let mut measurer = Measurer::default();
    let mut totals: BTreeMap<String, DiskTotal> = BTreeMap::new();
    let mut reclaimables = Vec::new();
    let mut tool_notes: Vec<ToolNote> = Vec::new();
    let mut add = |kind: Kind, id: &str, name: &str, path: PathBuf, source: Source| {
        let Source {
            run_from,
            reported,
            location_assumed,
        } = source;
        if reported.is_none() {
            env.progress_step(|| format!("Measuring {}", path.display()));
        }
        let measured = match reported {
            Some(bytes) => Measurement {
                bytes,
                unique_bytes: bytes,
                complete: true,
                last_modified_ms: None,
            },
            None => measurer.measure(&path),
        };
        let disk = if path.is_absolute() {
            disks::of_path(&path)
        } else {
            None
        };
        add_to_total(&mut totals, disk.as_ref(), &measured);
        let (last_modified_ms, age_known) = if kind == Kind::ProjectArtifact {
            let project = path.parent().map(discover::project_age).unwrap_or_default();
            let newest = measured.last_modified_ms.max(project.last_modified_ms);
            (newest, project.complete)
        } else {
            (None, true)
        };
        reclaimables.push(Reclaimable {
            id: id.to_string(),
            kind,
            name: name.to_string(),
            size_bytes: measured.bytes,
            complete: measured.complete,
            disk: disk.map(|d| d.name),
            last_modified_ms,
            age_known,
            change_bytes: None,
            change_state: ChangeState::Unknown,
            path,
            run_from,
            location_assumed,
        });
    };

    let mut not_projects = env.application_folders();
    for kind in catalog::tool_caches() {
        env.progress_step(|| format!("Looking for the {}", kind.name));
        env.narrate(|| format!("  looking for the {}", kind.name));
        for located in (kind.locate)(env, &scope.roots) {
            let in_folder = located.path.is_absolute() && located.path.is_dir();
            if in_folder {
                not_projects.push(located.path.clone());
            }
            let reported = match located.size {
                LocatedSize::Measured if in_folder => None,
                LocatedSize::Measured => continue,
                LocatedSize::Reported(bytes) => Some(bytes),
                LocatedSize::Unavailable(reason) => {
                    let note = ToolNote {
                        tool: kind.cleanup.tool.to_string(),
                        reason,
                    };
                    if !tool_notes.contains(&note) {
                        tool_notes.push(note);
                    }
                    continue;
                }
            };
            add(
                Kind::ToolCache,
                kind.id,
                kind.name,
                located.path,
                Source {
                    run_from: located.run_from,
                    reported,
                    location_assumed: located.assumed,
                },
            );
        }
    }
    let found = discover::find(env, &scope.roots, &not_projects);
    if !found.artifacts.is_empty() {
        env.narrate(|| {
            format!(
                "  measuring {}",
                crate::output::counted(found.artifacts.len(), "project artifact")
            )
        });
    }
    for artifact in found.artifacts {
        add(
            Kind::ProjectArtifact,
            artifact.kind.id,
            artifact.kind.name,
            artifact.path,
            Source::default(),
        );
    }
    env.narrate(|| {
        format!(
            "  found {} ({:.1} s)",
            crate::output::counted(reclaimables.len(), "reclaimable"),
            started.elapsed().as_secs_f32()
        )
    });
    env.progress_done();
    reclaimables.sort_by_key(|r| match r.kind {
        Kind::ToolCache => (0, 0),
        Kind::ProjectArtifact => (1, u64::MAX - r.size_bytes),
    });
    Scan {
        reclaimables,
        disks: totals.into_values().collect(),
        gone: Vec::new(),
        history_available: false,
        scan_roots: scope.roots.clone(),
        unavailable_roots: found.unavailable_roots,
        incomplete_roots: found.incomplete_roots,
        project_artifacts_skipped: scope.roots.is_empty(),
        tool_notes,
        partial: scope.one_off,
        taken_at_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reclaimable_on_a_disk_the_system_cannot_name_still_counts() {
        let mut totals = BTreeMap::new();
        let measured = Measurement {
            bytes: 10,
            unique_bytes: 10,
            complete: true,
            last_modified_ms: None,
        };

        add_to_total(&mut totals, None, &measured);

        let total = &totals[UNKNOWN_DISK];
        assert_eq!(total.reclaimable_bytes, 10);
        assert_eq!(total.free_bytes, None);
    }
}
