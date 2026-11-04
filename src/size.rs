//! How much space a directory takes.

use std::fs;
use std::path::Path;

/// Total length of the files under `path`. `path` itself may be a symbolic
/// link; links inside it are not followed.
pub fn of_dir(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => children(path),
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    }
}

fn entry(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => children(path),
        Ok(meta) if meta.is_file() => meta.len(),
        _ => 0,
    }
}

fn children(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| entry(&e.path()))
        .sum()
}
