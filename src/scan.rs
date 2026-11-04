//! A scan: every reclaimable present on this machine, with its size.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::catalog::{self, Kind};
use crate::env::Env;
use crate::size;
use crate::snapshots::Gone;

#[derive(Debug, Serialize)]
pub struct Reclaimable {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    /// Growth since the previous snapshot; `None` when there is nothing to
    /// compare with, which `Scan::history_available` tells apart.
    pub change_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Scan {
    pub reclaimables: Vec<Reclaimable>,
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
    let reclaimables = catalog::tool_caches()
        .iter()
        .filter_map(|kind| {
            let path = (kind.locate)(env)?;
            path.is_dir().then(|| Reclaimable {
                id: kind.id.to_string(),
                kind: Kind::ToolCache,
                name: kind.name.to_string(),
                size_bytes: size::of_dir(&path),
                change_bytes: None,
                path,
            })
        })
        .collect();
    Scan {
        reclaimables,
        gone: Vec::new(),
        history_available: false,
        taken_at_ms,
    }
}
