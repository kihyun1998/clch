//! A scan: every reclaimable present on this machine, with its size.

use std::path::PathBuf;

use serde::Serialize;

use crate::catalog::{self, Kind};
use crate::env::Env;
use crate::size;

#[derive(Debug, Serialize)]
pub struct Reclaimable {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct Scan {
    pub reclaimables: Vec<Reclaimable>,
}

pub fn run(env: &Env) -> Scan {
    let reclaimables = catalog::tool_caches()
        .iter()
        .filter_map(|kind| {
            let path = (kind.locate)(env)?;
            path.is_dir().then(|| Reclaimable {
                id: kind.id.to_string(),
                kind: Kind::ToolCache,
                name: kind.name.to_string(),
                size_bytes: size::of_dir(&path),
                path,
            })
        })
        .collect();
    Scan { reclaimables }
}
