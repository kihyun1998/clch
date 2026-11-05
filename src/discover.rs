//! Finding project artifacts under scan roots, and how recent their projects are.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::catalog::{self, ProjectArtifactKind};

/// A project artifact found on disk.
pub struct Found {
    pub path: PathBuf,
    pub kind: &'static ProjectArtifactKind,
}

#[derive(Default)]
pub struct Discovery {
    pub artifacts: Vec<Found>,
    pub unavailable_roots: Vec<PathBuf>,
    pub incomplete_roots: Vec<PathBuf>,
}

/// Searches `roots` for project artifacts. Symbolic links, hidden folders and
/// the folders in `not_projects` are not entered, and nothing inside a project
/// artifact is searched. A root that is missing or inside `not_projects` counts
/// as unavailable.
pub fn find(roots: &[PathBuf], not_projects: &[PathBuf]) -> Discovery {
    let not_projects: Vec<PathBuf> = not_projects
        .iter()
        .map(|p| fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    let mut discovery = Discovery::default();
    let mut seen = BTreeSet::new();
    for root in roots {
        let real = fs::canonicalize(root).ok().filter(|r| r.is_dir());
        let Some(real) = real.filter(|r| !not_projects.iter().any(|n| r.starts_with(n))) else {
            discovery.unavailable_roots.push(root.clone());
            continue;
        };
        if let Some(kind) = artifact_kind(root) {
            if seen.insert(real) {
                discovery.artifacts.push(Found {
                    path: root.clone(),
                    kind,
                });
            }
            continue;
        }
        let mut search = Search {
            not_projects: &not_projects,
            seen: &mut seen,
            artifacts: &mut discovery.artifacts,
            complete: true,
        };
        search.dir(root, &real);
        if !search.complete {
            discovery.incomplete_roots.push(root.clone());
        }
    }
    discovery
}

/// The kind of project artifact `path` is, by its name and its marker file.
fn artifact_kind(path: &Path) -> Option<&'static ProjectArtifactKind> {
    let name = path.file_name()?.to_string_lossy();
    let kind = named(&name)?;
    let parent = path.parent()?;
    has_marker(kind, parent).then_some(kind)
}

fn named(name: &str) -> Option<&'static ProjectArtifactKind> {
    catalog::project_artifacts()
        .iter()
        .find(|k| k.dir_name == name)
}

fn has_marker(kind: &ProjectArtifactKind, dir: &Path) -> bool {
    kind.markers.iter().any(|m| dir.join(m).is_file())
}

struct Search<'a> {
    not_projects: &'a [PathBuf],
    seen: &'a mut BTreeSet<PathBuf>,
    artifacts: &'a mut Vec<Found>,
    complete: bool,
}

impl Search<'_> {
    /// Searches `dir`, whose real location is `real`.
    fn dir(&mut self, dir: &Path, real: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            self.complete = false;
            return;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                self.complete = false;
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                self.complete = false;
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let real_child = real.join(&name);
            if self.not_projects.contains(&real_child) {
                continue;
            }
            let name = name.to_string_lossy();
            let path = entry.path();
            if let Some(kind) = named(&name) {
                if has_marker(kind, dir) {
                    if self.seen.insert(real_child) {
                        self.artifacts.push(Found { path, kind });
                    }
                    continue;
                }
                if !kind.search_unmarked {
                    continue;
                }
            }
            if !name.starts_with('.') {
                self.dir(&path, &real_child);
            }
        }
    }
}

/// When a project last changed, outside its project artifacts and hidden folders.
#[derive(Debug, Default)]
pub struct ProjectAge {
    pub last_modified_ms: Option<i64>,
    /// False when part of the project could not be read, so a newer file may
    /// have gone unseen.
    pub complete: bool,
}

/// How recently anything changed in the project folder `dir`.
pub fn project_age(dir: &Path) -> ProjectAge {
    let mut age = ProjectAge {
        last_modified_ms: None,
        complete: true,
    };
    walk_project(dir, &mut age);
    age
}

fn walk_project(dir: &Path, age: &mut ProjectAge) {
    let Ok(entries) = fs::read_dir(dir) else {
        age.complete = false;
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            age.complete = false;
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            age.complete = false;
            continue;
        };
        if meta.is_file() {
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok());
            if let Some(since) = modified {
                let ms = since.as_millis() as i64;
                let latest = age.last_modified_ms.get_or_insert(ms);
                *latest = (*latest).max(ms);
            }
        } else if meta.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with('.') && named(&name).is_none() {
                walk_project(&entry.path(), age);
            }
        }
    }
}

/// Whether another disk is mounted somewhere inside `dir`.
#[cfg(unix)]
pub fn holds_another_disk(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    fn walk(dir: &Path, device: u64) -> bool {
        let Ok(entries) = fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            entry.metadata().is_ok_and(|meta| {
                meta.is_dir() && (meta.dev() != device || walk(&entry.path(), device))
            })
        })
    }
    fs::symlink_metadata(dir).is_ok_and(|meta| walk(dir, meta.dev()))
}

/// A disk mounted in a folder on Windows is a reparse point, which deleting
/// removes without entering.
#[cfg(windows)]
pub fn holds_another_disk(_dir: &Path) -> bool {
    false
}
