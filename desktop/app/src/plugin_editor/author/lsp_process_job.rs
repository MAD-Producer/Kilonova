//! Independently authored Win32 lifetime guard. No UI or scheduling limits.
use std::{ffi::c_void, os::windows::io::AsRawHandle};
#[repr(C)]
#[derive(Default)]
struct Basic {
    process_time: i64,
    job_time: i64,
    flags: u32,
    min_working_set: usize,
    max_working_set: usize,
    process_limit: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
#[derive(Default)]
struct Extended {
    basic: Basic,
    io: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
    fn SetInformationJobObject(job: *mut c_void, class: i32, data: *const c_void, size: u32)
        -> i32;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
// Only this non-inherited handle owns the job; usize allows moving it to the
// supervisor. Windows closes it on owner exit even if Rust destructors don't run.
pub struct Job(usize);
impl Job {
    pub fn new() -> Result<Self, String> {
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if raw.is_null() {
            return Err(format!(
                "create language process job: {}",
                std::io::Error::last_os_error()
            ));
        }
        let job = Self(raw as usize);
        let mut limits = Extended::default();
        limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        let ok = unsafe {
            SetInformationJobObject(
                raw,
                9,
                (&limits as *const Extended).cast(),
                std::mem::size_of::<Extended>() as u32,
            )
        };
        if ok == 0 {
            return Err(format!(
                "set language process lifetime: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(job)
    }
    pub fn assign(&self, child: &std::process::Child) -> Result<(), String> {
        if unsafe { AssignProcessToJobObject(self.0 as *mut c_void, child.as_raw_handle()) } == 0 {
            return Err(format!(
                "assign language process lifetime: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if unsafe { CloseHandle(self.0 as *mut c_void) } == 0 {
            log::error!(
                "plugin.language cleanup phase=close-job cause={}",
                std::io::Error::last_os_error()
            );
        }
    }
}
