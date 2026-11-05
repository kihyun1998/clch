//! A scan: every reclaimable present on this machine, with its size.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::catalog::{self, Kind};
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
    pub size_bytes: u64,
    /// False when part of it could not be read, making `size_bytes` a lower
    /// bound.
    pub complete: bool,
    /// The disk it lives on, when the system can say.
    pub disk: Option<String>,
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

#[derive(Debug, Serialize)]
pub struct Scan {
    pub reclaimables: Vec<Reclaimable>,
    /// Totals per disk, with each hard-linked file counted once.
    pub disks: Vec<DiskTotal>,
    /// What the previous snapshot had and this scan did not find.
    pub gone: Vec<Gone>,
    /// Whether this scan could be compared with the snapshot store.
    pub history_available: bool,
    /// When the scan started, in milliseconds since the Unix epoch.
    #[serde(skip)]
    pub taken_at_ms: i64,
}

pub fn run(env: &Env) -> Scan {
    let taken_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let mut measurer = Measurer::default();
    let mut totals: BTreeMap<String, DiskTotal> = BTreeMap::new();
    let mut reclaimables = Vec::new();
    for kind in catalog::tool_caches() {
        let Some(path) = (kind.locate)(env).filter(|p| p.is_dir()) else {
            continue;
        };
        let measured = measurer.measure(&path);
        let disk = disks::of_path(&path);
        add_to_total(&mut totals, disk.as_ref(), &measured);
        reclaimables.push(Reclaimable {
            id: kind.id.to_string(),
            kind: Kind::ToolCache,
            name: kind.name.to_string(),
            size_bytes: measured.bytes,
            complete: measured.complete,
            disk: disk.map(|d| d.name),
            change_bytes: None,
            change_state: ChangeState::Unknown,
            path,
        });
    }
    Scan {
        reclaimables,
        disks: totals.into_values().collect(),
        gone: Vec::new(),
        history_available: false,
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
        };

        add_to_total(&mut totals, None, &measured);

        let total = &totals[UNKNOWN_DISK];
        assert_eq!(total.reclaimable_bytes, 10);
        assert_eq!(total.free_bytes, None);
    }
}
