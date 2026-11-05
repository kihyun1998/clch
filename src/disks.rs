//! Which disk a path is on, and how much space that disk has free.

use std::path::Path;

/// A mounted disk: where it is mounted, and the space free for this user.
#[derive(Debug, Clone)]
pub struct Disk {
    pub name: String,
    pub free_bytes: u64,
}

/// The disk `path` is on, or `None` when the system cannot say.
pub fn of_path(path: &Path) -> Option<Disk> {
    imp::of_path(path)
}

#[cfg(unix)]
mod imp {
    use std::ffi::{CStr, CString};
    use std::mem::MaybeUninit;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::Disk;

    pub fn of_path(path: &Path) -> Option<Disk> {
        let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
        Some(Disk {
            name: mount_point(path, &c_path)?,
            free_bytes: free_bytes(&c_path)?,
        })
    }

    fn free_bytes(path: &CStr) -> Option<u64> {
        let mut stat = MaybeUninit::<libc::statvfs>::zeroed();
        // SAFETY: `path` is NUL-terminated and `stat` is writable.
        if unsafe { libc::statvfs(path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: statvfs returned 0, so it filled `stat`.
        let stat = unsafe { stat.assume_init() };
        #[allow(clippy::unnecessary_cast)]
        Some(stat.f_bavail as u64 * stat.f_frsize as u64)
    }

    #[cfg(target_os = "macos")]
    fn mount_point(_path: &Path, c_path: &CStr) -> Option<String> {
        let mut stat = MaybeUninit::<libc::statfs>::zeroed();
        // SAFETY: `c_path` is NUL-terminated and `stat` is writable.
        if unsafe { libc::statfs(c_path.as_ptr(), stat.as_mut_ptr()) } != 0 {
            return None;
        }
        // SAFETY: statfs returned 0, so it filled `stat`, and `f_mntonname` is
        // a NUL-terminated string within its array.
        let name = unsafe { CStr::from_ptr(stat.assume_init().f_mntonname.as_ptr()) };
        Some(name.to_string_lossy().into_owned())
    }

    #[cfg(not(target_os = "macos"))]
    fn mount_point(path: &Path, _c_path: &CStr) -> Option<String> {
        use std::os::unix::fs::MetadataExt;
        let path = path.canonicalize().ok()?;
        let device = std::fs::metadata(&path).ok()?.dev();
        let mut mount = path.as_path();
        while let Some(parent) = mount.parent() {
            if std::fs::metadata(parent).ok()?.dev() != device {
                break;
            }
            mount = parent;
        }
        Some(mount.display().to_string())
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumePathNameW};

    use super::Disk;

    pub fn of_path(path: &Path) -> Option<Disk> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut mount = vec![0u16; wide.len().max(261) + 1];
        // SAFETY: `wide` is NUL-terminated and `mount` is writable for the
        // length given.
        if unsafe { GetVolumePathNameW(wide.as_ptr(), mount.as_mut_ptr(), mount.len() as u32) } == 0
        {
            return None;
        }
        let end = mount.iter().position(|&c| c == 0)?;
        let mut free = 0u64;
        // SAFETY: `mount` is NUL-terminated, `free` is writable, and the
        // other two out pointers may be null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                volume.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return None;
        }
        Some(Disk {
            name: String::from_utf16_lossy(&mount[..end]),
            free_bytes: free,
        })
    }
}
