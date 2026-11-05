//! Every kind of reclaimable clch knows, and where each one lives.

use std::path::PathBuf;

use serde::Serialize;

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
    pub locate: fn(&Env) -> Option<PathBuf>,
    pub cleanup: NativeCleanup,
}

/// The owning tool's own cleanup commands, per cleanup level. `None` where
/// the tool offers no command for that level.
pub struct NativeCleanup {
    pub tool: &'static str,
    pub unused: Option<&'static [&'static str]>,
    pub full: Option<&'static [&'static str]>,
    /// The oldest tool version that has these commands, as `tool --version`
    /// prints it.
    pub min_version: Option<&'static str>,
}

pub fn tool_caches() -> &'static [ToolCacheKind] {
    &[
        ToolCacheKind {
            id: "npm-cache",
            name: "npm cache",
            locate: npm::cache,
            cleanup: NativeCleanup {
                tool: "npm",
                unused: Some(&["cache", "verify"]),
                full: Some(&["cache", "clean", "--force"]),
                min_version: None,
            },
        },
        ToolCacheKind {
            id: "npx-cache",
            name: "npx cache",
            locate: npm::npx_cache,
            cleanup: NativeCleanup {
                tool: "npm",
                unused: None,
                full: Some(&["cache", "npx", "rm", "--force"]),
                min_version: Some("11.2.0"),
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
        if let Some(reported) = env.tool_value("npm", &["config", "get", "cache"]) {
            return Some(PathBuf::from(reported));
        }
        if cfg!(windows) {
            env.local_app_data()
                .or(env.home())
                .map(|d| d.join("npm-cache"))
        } else {
            env.home().map(|h| h.join(".npm"))
        }
    }

    pub fn cache(env: &Env) -> Option<PathBuf> {
        cache_root(env).map(|root| root.join("_cacache"))
    }

    pub fn npx_cache(env: &Env) -> Option<PathBuf> {
        cache_root(env).map(|root| root.join("_npx"))
    }
}
