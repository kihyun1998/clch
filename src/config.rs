//! The user's settings: a TOML file edited by clch and by hand.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{Array, DocumentMut, value};

use crate::env::Env;

const FILE: &str = "config.toml";
const LOCK_FILE: &str = "config.lock";
const SCAN_ROOTS: &str = "scan_roots";

#[derive(Debug)]
pub enum ConfigError {
    NoFolder,
    Read(PathBuf, std::io::Error),
    Parse(PathBuf, String),
    Write(PathBuf, std::io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NoFolder => write!(f, "no home folder to keep settings in"),
            ConfigError::Read(p, e) => write!(f, "could not read {}: {e}", p.display()),
            ConfigError::Parse(p, e) => write!(f, "could not understand {}: {e}", p.display()),
            ConfigError::Write(p, e) => write!(f, "could not write {}: {e}", p.display()),
        }
    }
}

/// The settings file, keeping whatever a person wrote in it.
pub struct Config {
    path: PathBuf,
    document: DocumentMut,
}

impl Config {
    pub fn load(env: &Env) -> Result<Config, ConfigError> {
        let path = env.config_dir().ok_or(ConfigError::NoFolder)?.join(FILE);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(ConfigError::Read(path, e)),
        };
        let document = text
            .parse::<DocumentMut>()
            .map_err(|e| ConfigError::Parse(path.clone(), e.to_string()))?;
        let config = Config { path, document };
        config.read_roots(env)?;
        Ok(config)
    }

    fn read_roots(&self, env: &Env) -> Result<Vec<PathBuf>, ConfigError> {
        let wrong = |why: String| ConfigError::Parse(self.path.clone(), why);
        let Some(item) = self.document.get(SCAN_ROOTS) else {
            return Ok(Vec::new());
        };
        let list = item
            .as_array()
            .ok_or_else(|| wrong(format!("{SCAN_ROOTS} must be a list of folders")))?;
        list.iter()
            .map(|v| {
                let text = v
                    .as_str()
                    .ok_or_else(|| wrong(format!("{SCAN_ROOTS} must be a list of folders")))?;
                let root = expand_home(env, text);
                if root.is_absolute() {
                    Ok(root)
                } else {
                    Err(wrong(format!(
                        "scan root '{text}' must be an absolute path or start with ~/"
                    )))
                }
            })
            .collect()
    }

    pub fn scan_roots(&self, env: &Env) -> Vec<PathBuf> {
        self.read_roots(env).unwrap_or_default()
    }

    /// Adds `root`; returns false when it was already there.
    pub fn add_scan_root(&mut self, env: &Env, root: &Path) -> bool {
        if self.scan_roots(env).iter().any(|r| r == root) {
            return false;
        }
        if self.document.get(SCAN_ROOTS).is_none() {
            self.document[SCAN_ROOTS] = value(Array::new());
        }
        if let Some(list) = self.document[SCAN_ROOTS].as_array_mut() {
            list.push(root.display().to_string());
        }
        true
    }

    /// Removes `root`; returns false when it was not there.
    pub fn remove_scan_root(&mut self, env: &Env, root: &Path) -> bool {
        let Some(list) = self
            .document
            .get_mut(SCAN_ROOTS)
            .and_then(|item| item.as_array_mut())
        else {
            return false;
        };
        let before = list.len();
        list.retain(|v| v.as_str().is_none_or(|text| expand_home(env, text) != root));
        list.len() != before
    }

    /// Writes the file, replacing it in one step. A file that is a link stays
    /// one, and the file keeps its permissions.
    pub fn save(&self) -> Result<(), ConfigError> {
        let fail = |e| ConfigError::Write(self.path.clone(), e);
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(fail)?;
        }
        let target = fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let temporary = target.with_extension(format!("toml.{}", std::process::id()));
        fs::write(&temporary, self.document.to_string()).map_err(fail)?;
        if let Ok(meta) = fs::metadata(&target) {
            fs::set_permissions(&temporary, meta.permissions()).map_err(fail)?;
        }
        fs::rename(&temporary, &target).map_err(fail)
    }
}

/// Holds the settings for one change, so that changes at the same time do not
/// overwrite each other. Released when dropped.
pub fn lock(env: &Env) -> Result<fs::File, ConfigError> {
    let dir = env.config_dir().ok_or(ConfigError::NoFolder)?;
    let path = dir.join(LOCK_FILE);
    let fail = |e| ConfigError::Write(path.clone(), e);
    fs::create_dir_all(&dir).map_err(fail)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(fail)?;
    file.lock().map_err(fail)?;
    Ok(file)
}

fn expand_home(env: &Env, text: &str) -> PathBuf {
    let home = env.home();
    match (
        text.strip_prefix("~/").or(text.strip_prefix("~\\")),
        text,
        home,
    ) {
        (Some(rest), _, Some(home)) => home.join(rest),
        (None, "~", Some(home)) => home.clone(),
        _ => PathBuf::from(text),
    }
}
