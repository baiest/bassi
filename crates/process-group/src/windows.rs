use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};

/// A Windows Job Object that a spawned process — and, by inheritance,
/// anything it later spawns (e.g. `node.exe` under `cmd /C npx`) — is
/// assigned to.
///
/// `Command::kill` only terminates the direct child (`cmd.exe`); any
/// process `cmd` goes on to start is a separate, unrelated process from
/// Windows' point of view and survives. Assigning both to a job configured
/// with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` means closing our handle to
/// the job (on `Drop`) kills every process still in it, orphans included.
pub struct ProcessGroup {
    handle: HANDLE,
}

impl ProcessGroup {
    pub fn new() -> std::io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };

        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }

        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        let set = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if set == 0 {
            let error = std::io::Error::last_os_error();
            unsafe { CloseHandle(handle) };
            return Err(error);
        }

        Ok(Self { handle })
    }

    /// Assigns `child` to this job, so it (and its future children) are
    /// killed together when the job closes.
    pub fn assign(&self, child: &Child) -> std::io::Result<()> {
        let process_handle = child.as_raw_handle() as HANDLE;

        let assigned = unsafe { AssignProcessToJobObject(self.handle, process_handle) };

        if assigned == 0 {
            return Err(std::io::Error::last_os_error());
        }

        Ok(())
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn creates_a_job_object() {
        assert!(ProcessGroup::new().is_ok());
    }

    #[test]
    fn dropping_the_group_kills_an_assigned_child() {
        let group = ProcessGroup::new().expect("job object");
        let mut child = Command::new("cmd")
            .args(["/C", "timeout /T 30"])
            .spawn()
            .expect("spawn child");

        group.assign(&child).expect("assign child to job");
        drop(group);

        // Closing the job handle kills the process, but Windows needs a
        // moment to tear it down; poll instead of asserting immediately.
        let mut killed = false;
        for _ in 0..50 {
            if let Ok(Some(_)) = child.try_wait() {
                killed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if !killed {
            let _ = child.kill();
        }
        assert!(killed, "child process was not killed when the job closed");
    }
}
