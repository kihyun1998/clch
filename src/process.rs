//! Running a tool as a process tree that can be ended as a whole.

use std::io;
use std::process::{Child, Command};

/// A spawned tool together with every process it starts.
pub struct ToolProcess {
    child: Child,
    #[cfg(windows)]
    job: windows::Job,
}

impl ToolProcess {
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            Ok(ToolProcess {
                child: command.spawn()?,
            })
        }
        #[cfg(windows)]
        {
            let job = windows::Job::new()?;
            let child = command.spawn()?;
            job.assign(&child)?;
            Ok(ToolProcess { child, job })
        }
    }

    pub fn child(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Ends the tool and everything it started, then reaps the tool.
    pub fn kill_tree(mut self) {
        #[cfg(unix)]
        {
            let pgid = self.child.id() as libc::pid_t;
            // SAFETY: kill(2) with a negative pid signals the process group
            // `process_group(0)` created for this child; it touches no memory.
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// A Job Object whose processes all end when it is terminated or closed.
    pub struct Job(HANDLE);

    impl Job {
        pub fn new() -> io::Result<Self> {
            // SAFETY: both arguments may be null; the returned handle is
            // owned by `Job` and closed in `Drop`.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Job(handle);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION
            // and the length passed is its size.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }

        pub fn assign(&self, child: &Child) -> io::Result<()> {
            // SAFETY: both handles are open: the job is owned by `self` and
            // the process handle by `child`.
            let ok = unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) };
            if ok == 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }

        pub fn terminate(&self) {
            // SAFETY: the job handle is open for the lifetime of `self`.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}
