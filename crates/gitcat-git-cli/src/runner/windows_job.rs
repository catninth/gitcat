use std::{ffi::c_void, io, mem::size_of, ptr};

use tokio::process::Child;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    },
};

pub(super) struct Job(HANDLE);

// The handle is uniquely owned and Windows job APIs are thread-safe.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}

impl Job {
    pub(super) fn assign(child: &Child) -> io::Result<Option<Self>> {
        let Some(process) = child.raw_handle() else {
            return Ok(None);
        };
        // SAFETY: every pointer passed here is either null as permitted by
        // the API or points to a live value for the duration of the call.
        unsafe {
            let handle = CreateJobObjectW(ptr::null(), ptr::null());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            if AssignProcessToJobObject(handle, process as HANDLE) == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Some(job))
        }
    }

    pub(super) fn terminate(&self) {
        // SAFETY: self owns a valid job handle until Drop.
        unsafe {
            TerminateJobObject(self.0, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // KILL_ON_JOB_CLOSE also removes any helper that outlived git.exe.
        // SAFETY: the owned handle is closed exactly once here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}
