//! How much disk space reclaimables occupy.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

/// What measuring one reclaimable found.
#[derive(Debug, Clone, Copy, Default)]
pub struct Measurement {
    /// Allocated space of its files, each hard-linked file counted once.
    pub bytes: u64,
    /// The part of `bytes` no earlier measurement by the same `Measurer` had.
    pub unique_bytes: u64,
    /// False when something under it could not be read, making `bytes` a
    /// lower bound.
    pub complete: bool,
}

/// Measures reclaimables, remembering hard-linked files across them.
#[derive(Default)]
pub struct Measurer {
    seen: HashSet<FileId>,
}

type FileId = (u64, u64);

struct FileInfo {
    allocated: u64,
    id: Option<FileId>,
}

struct Walk<'a> {
    measurer: &'a mut Measurer,
    own: HashSet<FileId>,
    root: Option<fs::Metadata>,
    result: Measurement,
}

impl Measurer {
    /// Measures the tree at `path`. `path` itself may be a symbolic link; links
    /// inside it are not followed, and another disk mounted inside it is not
    /// entered. A file that disappears while being measured simply is not
    /// counted.
    pub fn measure(&mut self, path: &Path) -> Measurement {
        let mut walk = Walk {
            measurer: self,
            own: HashSet::new(),
            root: None,
            result: Measurement {
                complete: true,
                ..Measurement::default()
            },
        };
        match fs::metadata(path) {
            Ok(meta) if meta.is_dir() => {
                walk.root = Some(meta);
                walk.dir(path);
            }
            Ok(meta) if meta.is_file() => walk.file(path, &meta),
            Ok(_) => {}
            Err(e) => walk.failed(&e),
        }
        walk.result
    }
}

impl Walk<'_> {
    fn failed(&mut self, error: &io::Error) {
        if error.kind() != io::ErrorKind::NotFound {
            self.result.complete = false;
        }
    }

    fn dir(&mut self, dir: &Path) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) => return self.failed(&e),
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    self.failed(&e);
                    continue;
                }
            };
            let path = entry.path();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => {
                    if self.root.as_ref().is_none_or(|root| same_disk(root, &meta)) {
                        self.dir(&path);
                    }
                }
                Ok(meta) if meta.is_file() => self.file(&path, &meta),
                Ok(_) => {}
                Err(e) => self.failed(&e),
            }
        }
    }

    fn file(&mut self, path: &Path, meta: &fs::Metadata) {
        let info = match file_info(path, meta) {
            Ok(info) => info,
            Err(e) => return self.failed(&e),
        };
        match info.id {
            Some(id) => {
                if self.own.insert(id) {
                    self.result.bytes += info.allocated;
                }
                if self.measurer.seen.insert(id) {
                    self.result.unique_bytes += info.allocated;
                }
            }
            None => {
                self.result.bytes += info.allocated;
                self.result.unique_bytes += info.allocated;
            }
        }
    }
}

#[cfg(unix)]
fn same_disk(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev()
}

/// A disk mounted in a folder on Windows is a reparse point, which the walk
/// does not enter as a directory at all.
#[cfg(windows)]
fn same_disk(_a: &fs::Metadata, _b: &fs::Metadata) -> bool {
    true
}

/// Allocated size of a regular file, and its identity when it has more than one
/// name.
#[cfg(unix)]
fn file_info(_path: &Path, meta: &fs::Metadata) -> io::Result<FileInfo> {
    use std::os::unix::fs::MetadataExt;
    Ok(FileInfo {
        allocated: meta.blocks() * 512,
        id: (meta.nlink() > 1).then(|| (meta.dev(), meta.ino())),
    })
}

/// Allocated size of a regular file, and its identity when it has more than one
/// name.
#[cfg(windows)]
fn file_info(path: &Path, _meta: &fs::Metadata) -> io::Result<FileInfo> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;

    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FileStandardInfo,
        GetFileInformationByHandle, GetFileInformationByHandleEx,
    };

    let file = fs::OpenOptions::new()
        .access_mode(0)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let handle = file.as_raw_handle() as HANDLE;
    let mut standard = std::mem::MaybeUninit::<FILE_STANDARD_INFO>::zeroed();
    let mut identity = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    // SAFETY: `handle` is open for the life of `file`, and each out pointer is
    // writable for the size given or implied by the structure it points to.
    let (standard, identity) = unsafe {
        if GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            standard.as_mut_ptr().cast(),
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        ) == 0
            || GetFileInformationByHandle(handle, identity.as_mut_ptr()) == 0
        {
            return Err(io::Error::last_os_error());
        }
        (standard.assume_init(), identity.assume_init())
    };
    let index = (u64::from(identity.nFileIndexHigh) << 32) | u64::from(identity.nFileIndexLow);
    Ok(FileInfo {
        allocated: standard.AllocationSize.max(0) as u64,
        id: (standard.NumberOfLinks > 1)
            .then_some((u64::from(identity.dwVolumeSerialNumber), index)),
    })
}

#[cfg(test)]
mod tests {
    use super::Measurer;

    #[test]
    fn a_path_that_no_longer_exists_measures_as_nothing_not_as_unreadable() {
        let measured = Measurer::default().measure(std::path::Path::new("/no/such/clch/path"));
        assert_eq!(measured.bytes, 0);
        assert!(measured.complete);
    }
}
