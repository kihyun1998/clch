//! Finding project artifacts under scan roots, and how recent their projects are.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::catalog::{self, ProjectArtifactKind};
use crate::env::Env;

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

/// Searches `roots` for project artifacts. Symbolic links, hidden folders,
/// `.app` bundles and the folders in `not_projects` are not entered, and nothing inside a project
/// artifact is searched. A root that is missing or inside `not_projects` counts
/// as unavailable.
pub fn find(env: &Env, roots: &[PathBuf], not_projects: &[PathBuf]) -> Discovery {
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
            env,
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

/// The kind of project artifact `path` is, when it is one.
fn artifact_kind(path: &Path) -> Option<&'static ProjectArtifactKind> {
    let name = path.file_name()?.to_string_lossy();
    let kind = named(&name)?;
    is_artifact(kind, path).then_some(kind)
}

fn named(name: &str) -> Option<&'static ProjectArtifactKind> {
    catalog::project_artifacts()
        .iter()
        .find(|k| k.dir_name == name)
}

/// Whether the folder at `path` shows what `kind` asks for: a marker beside it,
/// any line or file the kind names, and no repository of its own.
pub fn is_artifact(kind: &ProjectArtifactKind, path: &Path) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    if path.join(".git").exists() {
        return false;
    }
    let marked = kind.markers.iter().any(|m| {
        let marker = dir.join(m);
        marker.is_file() && kind.marker_line.is_none_or(|line| has_line(&marker, line))
    });
    marked && (kind.inside.is_empty() || kind.inside.iter().any(|e| path.join(e).is_file()))
}

/// Whether a line of the file, leading spaces aside, is exactly `line`.
fn has_line(file: &Path, line: &str) -> bool {
    fs::read_to_string(file)
        .is_ok_and(|text| text.lines().any(|l| l.trim_end().trim_start() == line))
}

struct Search<'a> {
    env: &'a Env,
    not_projects: &'a [PathBuf],
    seen: &'a mut BTreeSet<PathBuf>,
    artifacts: &'a mut Vec<Found>,
    complete: bool,
}

impl Search<'_> {
    /// Searches `dir`, whose real location is `real`.
    fn dir(&mut self, dir: &Path, real: &Path) {
        self.env
            .progress_passing(|| format!("Searching {}", dir.display()));
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
                if is_artifact(kind, &path) {
                    if self.seen.insert(real_child) {
                        self.artifacts.push(Found { path, kind });
                    }
                    continue;
                }
                if !kind.search_otherwise {
                    continue;
                }
            }
            if !name.starts_with('.') && !name.ends_with(".app") {
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
            let path = entry.path();
            let artifact = named(&name).is_some_and(|kind| is_artifact(kind, &path));
            if !name.starts_with('.') && !artifact {
                walk_project(&path, age);
            }
        }
    }
}

/// Whether `dir` or a folder above it holds a marker of `kind`.
pub fn inside_project_of(kind: &ProjectArtifactKind, dir: &Path) -> bool {
    dir.ancestors()
        .any(|folder| kind.markers.iter().any(|m| folder.join(m).is_file()))
}

/// Whether `dir` or a folder above it holds a git repository.
pub fn inside_repository(dir: &Path) -> bool {
    dir.ancestors().any(|folder| folder.join(".git").exists())
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
