//! Every kind of reclaimable clch knows, and where each one lives.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::disks;
use crate::env::Env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    ToolCache,
    ProjectArtifact,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::ToolCache => "tool-cache",
            Kind::ProjectArtifact => "project-artifact",
        }
    }
}

impl std::str::FromStr for Kind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "tool-cache" => Ok(Kind::ToolCache),
            "project-artifact" => Ok(Kind::ProjectArtifact),
            other => Err(format!("unknown kind of reclaimable: {other}")),
        }
    }
}

/// One kind of tool cache: its identity, how to find it on this machine and
/// how its tool cleans it.
pub struct ToolCacheKind {
    pub id: &'static str,
    pub name: &'static str,
    /// Finds its instances, given the scan roots.
    pub locate: fn(&Env, &[PathBuf]) -> Vec<Located>,
    /// Whether each disk has its own, so that its identity is its path.
    pub one_per_disk: bool,
    pub cleanup: NativeCleanup,
}

/// Where an instance of a tool cache is.
pub struct Located {
    pub path: PathBuf,
    /// The folder its tool must run in to act on this instance, when not the
    /// home folder.
    pub run_from: Option<PathBuf>,
}

impl Located {
    fn at(path: PathBuf) -> Located {
        Located {
            path,
            run_from: None,
        }
    }
}

/// The owning tool's own cleanup commands, per cleanup level. `None` where
/// the tool offers no command for that level.
pub struct NativeCleanup {
    pub tool: &'static str,
    /// Other names the same tool goes by, tried in order after `tool`.
    pub also_named: &'static [&'static str],
    /// What may be deleted when the tool's official docs allow it, so that a
    /// full cleanup can do that where no command does it. `None` where they do
    /// not.
    pub deletable: Option<Deletable>,
    pub unused: Option<&'static [&'static str]>,
    pub full: Option<&'static [&'static str]>,
    /// The oldest tool version that has these commands, as `tool --version`
    /// prints it.
    pub min_version: Option<&'static str>,
    /// The first tool version that no longer has them.
    pub below_version: Option<&'static str>,
}

/// How to tell that a folder is the cache clch thinks it is, and what to delete.
#[derive(Debug, Clone, Copy)]
pub enum Deletable {
    /// A folder holding only these entries, deleted whole.
    FolderHolding(&'static [&'static str]),
    /// Each `v<number>` folder inside that has a `files` folder.
    VersionedStores,
}

pub fn tool_caches() -> &'static [ToolCacheKind] {
    &[
        ToolCacheKind {
            id: "npm-cache",
            name: "npm cache",
            locate: npm::cache,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "npm",
                also_named: &[],
                deletable: None,
                unused: Some(&["cache", "verify"]),
                full: Some(&["cache", "clean", "--force"]),
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "npx-cache",
            name: "npx cache",
            locate: npm::npx_cache,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "npm",
                also_named: &[],
                deletable: None,
                unused: None,
                full: Some(&["cache", "npx", "rm", "--force"]),
                min_version: Some("11.2.0"),
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "pnpm-store",
            name: "pnpm store",
            locate: pnpm::stores,
            one_per_disk: true,
            cleanup: NativeCleanup {
                tool: "pnpm",
                also_named: &[],
                deletable: Some(Deletable::VersionedStores),
                unused: Some(&["store", "prune"]),
                full: None,
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "yarn-cache",
            name: "yarn cache",
            locate: yarn::classic,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "yarn",
                also_named: &[],
                deletable: None,
                unused: None,
                full: Some(&["cache", "clean"]),
                min_version: None,
                below_version: Some("2.0.0"),
            },
        },
        ToolCacheKind {
            id: "yarn-berry-cache",
            name: "yarn Berry cache",
            locate: yarn::berry,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "yarn",
                also_named: &[],
                deletable: None,
                unused: None,
                full: Some(&["cache", "clean", "--all"]),
                min_version: Some("2.0.0"),
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "pip-cache",
            name: "pip cache",
            locate: python::pip,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "pip",
                also_named: &["pip3"],
                deletable: None,
                unused: None,
                full: Some(&["cache", "purge"]),
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "uv-cache",
            name: "uv cache",
            locate: python::uv,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "uv",
                also_named: &[],
                deletable: None,
                unused: Some(&["cache", "prune"]),
                full: Some(&["cache", "clean"]),
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "cargo-registry",
            name: "Cargo registry cache",
            locate: cargo::registry,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "cargo",
                also_named: &[],
                deletable: Some(Deletable::FolderHolding(&[
                    "cache",
                    "src",
                    "index",
                    "CACHEDIR.TAG",
                ])),
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "cargo-git",
            name: "Cargo git cache",
            locate: cargo::git,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "cargo",
                also_named: &[],
                deletable: Some(Deletable::FolderHolding(&[
                    "db",
                    "checkouts",
                    "CACHEDIR.TAG",
                ])),
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "go-build-cache",
            name: "Go build cache",
            locate: go::build,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "go",
                also_named: &[],
                deletable: None,
                unused: None,
                full: Some(&["clean", "-cache"]),
                min_version: None,
                below_version: None,
            },
        },
        ToolCacheKind {
            id: "go-module-cache",
            name: "Go module cache",
            locate: go::modules,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "go",
                also_named: &[],
                deletable: None,
                unused: None,
                full: Some(&["clean", "-modcache"]),
                min_version: None,
                below_version: None,
            },
        },
    ]
}

pub fn tool_cache(id: &str) -> Option<&'static ToolCacheKind> {
    tool_caches().iter().find(|k| k.id == id)
}

/// One kind of project artifact: the folder's name and the files, any of
/// which beside it marks it as what its toolchain produced.
#[derive(Debug)]
pub struct ProjectArtifactKind {
    pub id: &'static str,
    pub name: &'static str,
    pub dir_name: &'static str,
    pub markers: &'static [&'static str],
    /// Whether a folder of this name without a marker may still hold
    /// projects, and so is searched.
    pub search_unmarked: bool,
}

pub fn project_artifacts() -> &'static [ProjectArtifactKind] {
    &[ProjectArtifactKind {
        id: "node_modules",
        name: "node_modules",
        dir_name: "node_modules",
        markers: &["package.json"],
        search_unmarked: false,
    }]
}

pub fn project_artifact(id: &str) -> Option<&'static ProjectArtifactKind> {
    project_artifacts().iter().find(|k| k.id == id)
}

mod npm {
    use super::*;

    /// npm's `cache` setting: what npm reports, else npm's own default
    /// (`%LOCALAPPDATA%\npm-cache` on Windows, falling back to
    /// `~\npm-cache`; `~/.npm` elsewhere).
    fn cache_root(env: &Env) -> Option<PathBuf> {
        if let Some(reported) = env.tool_path(
            "npm",
            &["config", "get", "cache"],
            env.home().map(PathBuf::as_path),
        ) {
            return Some(reported);
        }
        if cfg!(windows) {
            env.local_app_data()
                .or(env.home())
                .map(|d| d.join("npm-cache"))
        } else {
            env.home().map(|h| h.join(".npm"))
        }
    }

    pub fn cache(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        cache_root(env)
            .map(|root| Located::at(root.join("_cacache")))
            .into_iter()
            .collect()
    }

    pub fn npx_cache(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        cache_root(env)
            .map(|root| Located::at(root.join("_npx")))
            .into_iter()
            .collect()
    }
}

mod pnpm {
    use super::*;

    /// One store per disk. The one for the disk with the home folder is what pnpm
    /// reports from there, else pnpm's default. On any other disk a scan root is
    /// on, it is `.pnpm-store` at the disk's root, and pnpm is asked and run from
    /// inside it: a project's folder may hold code and settings pnpm would act on.
    pub fn stores(env: &Env, roots: &[PathBuf]) -> Vec<Located> {
        let reported = env
            .tool_path("pnpm", &["store", "path"], env.home().map(PathBuf::as_path))
            .map(|store| store_folder(&store));
        let mut stores: Vec<Located> = reported
            .or_else(|| default_store(env))
            .map(Located::at)
            .into_iter()
            .collect();
        for root in roots {
            let Some(disk) = disks::of_path(root) else {
                continue;
            };
            let store = PathBuf::from(disk.name).join(".pnpm-store");
            if store.is_dir() && !stores.iter().any(|known| known.path == store) {
                stores.push(Located {
                    run_from: Some(store.clone()),
                    path: store,
                });
            }
        }
        stores
    }

    /// The folder holding every version's store, from the versioned one pnpm
    /// reports, such as `store/v10`.
    fn store_folder(reported: &Path) -> PathBuf {
        let versioned = reported
            .file_name()
            .map(|n| n.to_string_lossy())
            .and_then(|n| n.strip_prefix('v').map(|d| d.parse::<u32>().is_ok()));
        match (versioned, reported.parent()) {
            (Some(true), Some(parent)) => parent.to_path_buf(),
            _ => reported.to_path_buf(),
        }
    }

    fn default_store(env: &Env) -> Option<PathBuf> {
        if let Some(pnpm_home) = env.var("PNPM_HOME") {
            return Some(PathBuf::from(pnpm_home).join("store"));
        }
        if let Some(data) = env.var("XDG_DATA_HOME") {
            return Some(PathBuf::from(data).join("pnpm").join("store"));
        }
        let home = env.home()?;
        Some(if cfg!(windows) {
            home.join("AppData")
                .join("Local")
                .join("pnpm")
                .join("store")
        } else {
            home.join("Library").join("pnpm").join("store")
        })
    }
}

/// What `tool args` reports, from the home folder.
fn reported(env: &Env, tool: &str, args: &[&str]) -> Option<PathBuf> {
    env.tool_path(tool, args, env.home().map(PathBuf::as_path))
}

/// `%LOCALAPPDATA%`, or its default under the home folder.
fn local_app_data(env: &Env) -> Option<PathBuf> {
    env.local_app_data()
        .cloned()
        .or_else(|| env.home().map(|h| h.join("AppData").join("Local")))
}

fn one(path: Option<PathBuf>) -> Vec<Located> {
    path.map(Located::at).into_iter().collect()
}

mod python {
    use super::*;

    pub fn pip(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        let pip = ["pip", "pip3"]
            .into_iter()
            .find(|t| env.find_tool(t).is_some());
        one(pip
            .and_then(|pip| reported(env, pip, &["cache", "dir"]))
            .or_else(|| {
                if let Some(dir) = env.var("PIP_CACHE_DIR") {
                    return Some(PathBuf::from(dir));
                }
                if cfg!(windows) {
                    return Some(local_app_data(env)?.join("pip").join("Cache"));
                }
                if let Some(cache) = env.var("XDG_CACHE_HOME") {
                    return Some(PathBuf::from(cache).join("pip"));
                }
                Some(env.home()?.join("Library").join("Caches").join("pip"))
            }))
    }

    pub fn uv(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        one(reported(env, "uv", &["cache", "dir"]).or_else(|| {
            if let Some(dir) = env.var("UV_CACHE_DIR") {
                return Some(PathBuf::from(dir));
            }
            if cfg!(windows) {
                return Some(local_app_data(env)?.join("uv").join("cache"));
            }
            if let Some(cache) = env.var("XDG_CACHE_HOME") {
                return Some(PathBuf::from(cache).join("uv"));
            }
            Some(env.home()?.join(".cache").join("uv"))
        }))
    }
}

mod cargo {
    use super::*;

    fn home(env: &Env) -> Option<PathBuf> {
        env.var("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| Some(env.home()?.join(".cargo")))
    }

    pub fn registry(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        one(home(env).map(|h| h.join("registry")))
    }

    pub fn git(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        one(home(env).map(|h| h.join("git")))
    }
}

mod go {
    use super::*;

    pub fn build(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        one(reported(env, "go", &["env", "GOCACHE"]).or_else(|| {
            if let Some(dir) = env.var("GOCACHE") {
                return Some(PathBuf::from(dir));
            }
            if cfg!(windows) {
                return Some(local_app_data(env)?.join("go-build"));
            }
            Some(env.home()?.join("Library").join("Caches").join("go-build"))
        }))
    }

    pub fn modules(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        one(reported(env, "go", &["env", "GOMODCACHE"]).or_else(|| {
            if let Some(dir) = env.var("GOMODCACHE") {
                return Some(PathBuf::from(dir));
            }
            let gopath = env
                .var("GOPATH")
                .and_then(|list| std::env::split_paths(&list).next())
                .or_else(|| Some(env.home()?.join("go")))?;
            Some(gopath.join("pkg").join("mod"))
        }))
    }
}

mod yarn {
    use super::*;

    /// Whether the yarn on `PATH` is Classic, by its major version.
    fn is_classic(env: &Env) -> Option<bool> {
        let version = env.tool_value("yarn", &["--version"])?;
        let major = version.split('.').next()?.parse::<u32>().ok()?;
        Some(major < 2)
    }

    /// What Classic reports when it is the yarn on `PATH`, else its default.
    pub fn classic(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        let reported = (is_classic(env) == Some(true))
            .then(|| env.tool_path("yarn", &["cache", "dir"], env.home().map(PathBuf::as_path)))
            .flatten();
        reported
            .or_else(|| {
                let root = if cfg!(windows) {
                    env.local_app_data()
                        .cloned()
                        .or_else(|| env.home().map(|h| h.join("AppData").join("Local")))?
                        .join("Yarn")
                        .join("Cache")
                } else if let Some(cache) = env.var("XDG_CACHE_HOME") {
                    PathBuf::from(cache).join("yarn")
                } else {
                    env.home()?.join("Library").join("Caches").join("Yarn")
                };
                Some(root.join("v6"))
            })
            .map(Located::at)
            .into_iter()
            .collect()
    }

    /// What Berry reports when it is the yarn on `PATH`, else its default.
    pub fn berry(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        let reported = (is_classic(env) == Some(false))
            .then(|| {
                env.tool_path(
                    "yarn",
                    &["config", "get", "cacheFolder"],
                    env.home().map(PathBuf::as_path),
                )
            })
            .flatten();
        reported
            .or_else(|| {
                let global = if cfg!(windows) {
                    env.local_app_data()
                        .cloned()
                        .or_else(|| env.home().map(|h| h.join("AppData").join("Local")))?
                        .join("Yarn")
                        .join("Berry")
                } else if let Some(data) = env.var("XDG_DATA_HOME") {
                    PathBuf::from(data).join("yarn").join("berry")
                } else {
                    env.home()?.join(".yarn").join("berry")
                };
                Some(global.join("cache"))
            })
            .map(Located::at)
            .into_iter()
            .collect()
    }
}
