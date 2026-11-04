//! Every kind of reclaimable clch knows, and where each one lives.

use std::path::PathBuf;

use serde::Serialize;

use crate::env::Env;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    ToolCache,
}

/// One kind of tool cache: its identity and how to find it on this machine.
pub struct ToolCacheKind {
    pub id: &'static str,
    pub name: &'static str,
    pub locate: fn(&Env) -> Option<PathBuf>,
}

pub fn tool_caches() -> &'static [ToolCacheKind] {
    &[
        ToolCacheKind {
            id: "npm-cache",
            name: "npm cache",
            locate: npm::cache,
        },
        ToolCacheKind {
            id: "npx-cache",
            name: "npx cache",
            locate: npm::npx_cache,
        },
    ]
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
