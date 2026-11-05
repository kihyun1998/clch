//! Every kind of reclaimable clch knows, and where each one lives.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::disks;
use crate::env::{Env, ToolAnswer};

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
    /// What a person must be told on Windows after cleaning it.
    pub windows_note: Option<&'static str>,
}

/// Where an instance of a tool cache is.
pub struct Located {
    pub path: PathBuf,
    /// The folder its tool must run in to act on this instance, when not the
    /// home folder.
    pub run_from: Option<PathBuf>,
    pub size: LocatedSize,
}

/// Where the size of an instance of a tool cache comes from.
pub enum LocatedSize {
    /// The files at its path, as measured.
    Measured,
    /// What its tool reports, for a store clch cannot walk.
    Reported(u64),
    /// Its tool cannot say at the moment, for this reason.
    Unavailable(String),
}

impl Located {
    fn at(path: PathBuf) -> Located {
        Located {
            path,
            run_from: None,
            size: LocatedSize::Measured,
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
    /// A folder holding every entry of `needs` and only entries of `holds`,
    /// deleted whole.
    FolderHolding {
        holds: &'static [&'static str],
        needs: &'static [&'static str],
    },
    /// Each `v<number>` folder inside that has a `files` folder.
    VersionedStores,
    /// A folder of this name, deleted whole.
    FolderNamed(&'static str),
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
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
        },
        ToolCacheKind {
            id: "cargo-registry",
            name: "Cargo registry cache",
            locate: cargo::registry,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "cargo",
                also_named: &[],
                deletable: Some(Deletable::FolderHolding {
                    holds: &["cache", "src", "index", "CACHEDIR.TAG"],
                    needs: &[],
                }),
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
        ToolCacheKind {
            id: "cargo-git",
            name: "Cargo git cache",
            locate: cargo::git,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "cargo",
                also_named: &[],
                deletable: Some(Deletable::FolderHolding {
                    holds: &["db", "checkouts", "CACHEDIR.TAG"],
                    needs: &[],
                }),
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
            windows_note: None,
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
            windows_note: None,
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
            windows_note: None,
        },
        ToolCacheKind {
            id: "docker-build-cache",
            name: "Docker build cache",
            locate: docker::build_cache,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "docker",
                also_named: &[],
                deletable: None,
                unused: Some(&["builder", "prune", "--force"]),
                full: Some(&["builder", "prune", "--all", "--force"]),
                min_version: None,
                below_version: None,
            },
            windows_note: Some(docker::WSL_DISK),
        },
        ToolCacheKind {
            id: "docker-images",
            name: "Docker unused images",
            locate: docker::images,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "docker",
                also_named: &[],
                deletable: None,
                unused: Some(&["image", "prune", "--force"]),
                full: Some(&["image", "prune", "--all", "--force"]),
                min_version: None,
                below_version: None,
            },
            windows_note: Some(docker::WSL_DISK),
        },
        ToolCacheKind {
            id: "pub-cache",
            name: "pub cache",
            locate: dart::pub_cache,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "dart",
                also_named: &[],
                deletable: Some(Deletable::FolderHolding {
                    holds: &[
                        "hosted",
                        "hosted-hashes",
                        "git",
                        "global_packages",
                        "bin",
                        "_temp",
                        "active_roots",
                        "README.md",
                    ],
                    needs: &["hosted"],
                }),
                unused: Some(&["pub", "cache", "gc", "--force"]),
                full: Some(&["pub", "cache", "clean", "--force"]),
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
        ToolCacheKind {
            id: "homebrew-cache",
            name: "Homebrew cache",
            locate: macos::homebrew,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "brew",
                also_named: &[],
                deletable: None,
                unused: Some(&["cleanup", "--scrub"]),
                full: Some(&["cleanup", "--prune=all"]),
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
        ToolCacheKind {
            id: "xcode-derived-data",
            name: "Xcode derived data",
            locate: macos::derived_data,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "xcodebuild",
                also_named: &[],
                deletable: Some(Deletable::FolderNamed("DerivedData")),
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
        ToolCacheKind {
            id: "xcode-device-support",
            name: "Xcode device support",
            locate: macos::device_support,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "xcodebuild",
                also_named: &[],
                deletable: None,
                unused: None,
                full: None,
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
        ToolCacheKind {
            id: "xcode-simulators",
            name: "Xcode simulators",
            locate: macos::simulators,
            one_per_disk: false,
            cleanup: NativeCleanup {
                tool: "xcrun",
                also_named: &[],
                deletable: None,
                unused: Some(&["simctl", "delete", "unavailable"]),
                full: Some(&["simctl", "delete", "unavailable"]),
                min_version: None,
                below_version: None,
            },
            windows_note: None,
        },
    ]
}

pub fn tool_cache(id: &str) -> Option<&'static ToolCacheKind> {
    tool_caches().iter().find(|k| k.id == id)
}

/// One kind of project artifact: the folder's name, and what shows that a folder
/// of that name is what its toolchain produced.
#[derive(Debug)]
pub struct ProjectArtifactKind {
    pub id: &'static str,
    pub name: &'static str,
    pub dir_name: &'static str,
    /// Files any one of which must sit beside it.
    pub markers: &'static [&'static str],
    /// A line the marker must have, when the marker alone does not say so.
    pub marker_line: Option<&'static str>,
    /// Entries any one of which must be inside it; empty when none is reliable.
    pub inside: &'static [&'static str],
    /// Whether a folder of this name that is not an artifact may still hold
    /// projects, and so is searched.
    pub search_otherwise: bool,
}

pub fn project_artifacts() -> &'static [ProjectArtifactKind] {
    &[
        ProjectArtifactKind {
            id: "node_modules",
            name: "node_modules",
            dir_name: "node_modules",
            markers: &["package.json"],
            marker_line: None,
            inside: &[],
            search_otherwise: false,
        },
        ProjectArtifactKind {
            id: "target",
            name: "Cargo target",
            dir_name: "target",
            markers: &["Cargo.toml"],
            marker_line: None,
            inside: &["CACHEDIR.TAG", ".rustc_info.json"],
            search_otherwise: true,
        },
        ProjectArtifactKind {
            id: ".venv",
            name: "Python .venv",
            dir_name: ".venv",
            markers: &["pyproject.toml", "requirements.txt"],
            marker_line: None,
            inside: &["pyvenv.cfg"],
            search_otherwise: false,
        },
        ProjectArtifactKind {
            id: ".dart_tool",
            name: "Dart .dart_tool",
            dir_name: ".dart_tool",
            markers: &["pubspec.yaml"],
            marker_line: None,
            inside: &["package_config.json"],
            search_otherwise: false,
        },
        ProjectArtifactKind {
            id: "build",
            name: "Flutter build",
            dir_name: "build",
            markers: &["pubspec.yaml"],
            marker_line: Some("sdk: flutter"),
            inside: &[],
            search_otherwise: true,
        },
    ]
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
                    size: LocatedSize::Measured,
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

mod dart {
    use super::*;

    /// `PUB_CACHE`, else the default in Dart's docs.
    pub fn pub_cache(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        if let Some(dir) = env.var("PUB_CACHE") {
            return one(Some(PathBuf::from(dir)));
        }
        one(if cfg!(windows) {
            local_app_data(env).map(|d| d.join("Pub").join("Cache"))
        } else {
            env.home().map(|h| h.join(".pub-cache"))
        })
    }
}

mod macos {
    use super::*;

    fn developer(env: &Env, rest: &str) -> Vec<Located> {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        one(env.home().map(|h| h.join("Library/Developer").join(rest)))
    }

    /// What brew reports, else `HOMEBREW_CACHE`, else brew's default.
    pub fn homebrew(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        one(reported(env, "brew", &["--cache"]).or_else(|| {
            if let Some(dir) = env.var("HOMEBREW_CACHE") {
                return Some(PathBuf::from(dir));
            }
            Some(env.home()?.join("Library/Caches/Homebrew"))
        }))
    }

    /// Where Xcode's settings put derived data, else its default.
    pub fn derived_data(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        if !cfg!(target_os = "macos") {
            return Vec::new();
        }
        let set = reported(
            env,
            "defaults",
            &["read", "com.apple.dt.Xcode", "IDECustomDerivedDataLocation"],
        );
        match set {
            Some(path) => one(Some(path)),
            None => developer(env, "Xcode/DerivedData"),
        }
    }

    pub fn device_support(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        developer(env, "Xcode/iOS DeviceSupport")
    }

    pub fn simulators(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        developer(env, "CoreSimulator/Devices")
    }
}

mod docker {
    use super::*;

    /// What a Docker reclaimable's path says when Docker's data is in no folder
    /// clch can name.
    const NO_FOLDER: &str = "docker engine";

    const ENDPOINT: &[&str] = &[
        "context",
        "inspect",
        "--format",
        "{{.Endpoints.docker.Host}}",
    ];

    pub const WSL_DISK: &str = "\
With Docker Desktop on WSL 2, Docker's data lives in a virtual disk, which may not shrink when data inside it is deleted. Check your free space: a sparse disk gives the space back by itself. If it did not come back, compact the disk yourself:
  1. Quit Docker Desktop.
  2. Run: wsl --shutdown
  3. Find the .vhdx file in %LOCALAPPDATA%\\Docker\\wsl, or in the folder set under Settings > Resources > Advanced in Docker Desktop.
  4. In a terminal run as administrator, run diskpart, then:
       select vdisk file=\"<path to the .vhdx file>\"
       compact vdisk
       exit";

    pub fn build_cache(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        found(env, build_cache_size)
    }

    pub fn images(env: &Env, _roots: &[PathBuf]) -> Vec<Located> {
        found(env, images_size)
    }

    /// A reclaimable sized by `size`, once it is known that the engine answering
    /// is on this machine.
    fn found(env: &Env, size: fn(&Env) -> LocatedSize) -> Vec<Located> {
        let size = match env.tool_report("docker", ENDPOINT) {
            ToolAnswer::Absent => return Vec::new(),
            ToolAnswer::Printed(endpoint) if is_local(endpoint.trim()) => size(env),
            ToolAnswer::Printed(endpoint) => LocatedSize::Unavailable(format!(
                "its engine is at {}, not on this machine, so it is left alone",
                endpoint.trim()
            )),
            _ => LocatedSize::Unavailable("it could not say where its engine is".to_string()),
        };
        let folder = match size {
            LocatedSize::Unavailable(_) => None,
            _ => desktop_folder(env),
        };
        vec![Located {
            path: folder.unwrap_or_else(|| PathBuf::from(NO_FOLDER)),
            run_from: None,
            size,
        }]
    }

    fn is_local(endpoint: &str) -> bool {
        endpoint.starts_with("unix://") || endpoint.starts_with("npipe://")
    }

    /// The report `args` asks the daemon for, as a size.
    fn from_daemon(env: &Env, args: &[&str], read: impl Fn(&str) -> Option<u64>) -> LocatedSize {
        match env.tool_report("docker", args) {
            ToolAnswer::Printed(report) => match read(&report) {
                Some(bytes) => LocatedSize::Reported(bytes),
                None => LocatedSize::Unavailable("its disk report could not be read".to_string()),
            },
            ToolAnswer::TimedOut => {
                LocatedSize::Unavailable("its daemon did not answer in time".to_string())
            }
            _ => LocatedSize::Unavailable("its daemon is not running or not answering".to_string()),
        }
    }

    /// The build cache's `Reclaimable` figure in `docker system df`.
    fn build_cache_size(env: &Env) -> LocatedSize {
        from_daemon(env, &["system", "df", "--format", "json"], |report| {
            report.lines().find_map(|line| {
                let line: serde_json::Value = serde_json::from_str(line).ok()?;
                (line["Type"] == "Build Cache")
                    .then(|| line["Reclaimable"].as_str().and_then(bytes))?
            })
        })
    }

    /// What the images no container uses hold alone, in `docker system df
    /// --verbose`.
    fn images_size(env: &Env) -> LocatedSize {
        from_daemon(
            env,
            &["system", "df", "--verbose", "--format", "json"],
            |report| {
                let report: serde_json::Value = serde_json::from_str(report).ok()?;
                let mut total = 0;
                for image in report["Images"].as_array()? {
                    let users: u64 = image["Containers"].as_str()?.parse().ok()?;
                    if users == 0 {
                        total += bytes(image["UniqueSize"].as_str()?)?;
                    }
                }
                Some(total)
            },
        )
    }

    /// The folder Docker's docs give for Docker Desktop's disk image, when the
    /// engine answering is Docker Desktop's and the folder is there.
    fn desktop_folder(env: &Env) -> Option<PathBuf> {
        let engine = env.tool_value("docker", &["info", "--format", "{{.OperatingSystem}}"])?;
        if engine != "Docker Desktop" {
            return None;
        }
        let folder = if cfg!(windows) {
            local_app_data(env)?.join("Docker").join("wsl")
        } else {
            env.home()?
                .join("Library/Containers/com.docker.docker/Data/vms/0/data")
        };
        folder.is_dir().then_some(folder)
    }

    /// Bytes from Docker's human-readable size, such as `2.18GB (37%)`: decimal
    /// units, rounded to four digits.
    fn bytes(text: &str) -> Option<u64> {
        let figure = text.split_whitespace().next()?;
        let number_end = figure.find(|c: char| c.is_ascii_alphabetic())?;
        let (number, unit) = figure.split_at(number_end);
        let scale: f64 = match unit {
            "B" => 1.0,
            "kB" => 1e3,
            "MB" => 1e6,
            "GB" => 1e9,
            "TB" => 1e12,
            "PB" => 1e15,
            _ => return None,
        };
        let value = number.parse::<f64>().ok()? * scale;
        (value.is_finite() && value >= 0.0).then(|| value.round() as u64)
    }

    #[cfg(test)]
    mod tests {
        use super::bytes;

        #[test]
        fn sizes_read_as_docker_writes_them() {
            assert_eq!(bytes("0B"), Some(0));
            assert_eq!(bytes("0B (0%)"), Some(0));
            assert_eq!(bytes("20.48kB (0%)"), Some(20_480));
            assert_eq!(bytes("2.18GB (37%)"), Some(2_180_000_000));
            assert_eq!(bytes("1.5TB"), Some(1_500_000_000_000));
            assert_eq!(bytes("3XB"), None);
            assert_eq!(bytes("N/A"), None);
            assert_eq!(bytes(""), None);
        }
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
