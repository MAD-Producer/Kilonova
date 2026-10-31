//! OS CPU scheduling for the optional native owner, never the foreground owner.
//!
//! This is not GPU queue priority and does not preempt submitted driver work.
//! Each call changes only its calling thread; the worker keeps the policy until
//! it exits, including native-resource teardown. No process policy is changed.

pub(super) fn lower_current_thread() -> Result<(), String> {
    platform::lower().map_err(|cause| format!("prerender background thread scheduling: {cause}"))
}

#[cfg(target_os = "windows")]
mod platform {
    use std::{ffi::c_void, io};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn GetThreadPriority(thread: *mut c_void) -> i32;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }
    pub(super) fn current() -> Result<i32, io::Error> {
        // SAFETY: the pseudo-handle denotes this thread and needs no close.
        let value = unsafe { GetThreadPriority(GetCurrentThread()) };
        if value == i32::MAX {
            Err(io::Error::last_os_error())
        } else {
            Ok(value)
        }
    }
    pub(super) fn lower() -> Result<(), io::Error> {
        // THREAD_PRIORITY_LOWEST. Preserve an inherited still lower priority.
        if current()? > -2 {
            // SAFETY: only the calling thread's scheduling is changed.
            if unsafe { SetThreadPriority(GetCurrentThread(), -2) } == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if current()? > -2 {
            return Err(io::Error::other("CPU priority did not decrease"));
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io;

    unsafe extern "C" {
        fn getpriority(which: i32, who: u32) -> i32;
        fn setpriority(which: i32, who: u32, priority: i32) -> i32;
        fn __errno_location() -> *mut i32;
    }
    pub(super) fn current() -> Result<i32, io::Error> {
        // SAFETY: glibc/musl errno is thread-local. On Linux PRIO_PROCESS/0
        // addresses the calling thread, not every thread in the process.
        unsafe {
            *__errno_location() = 0;
            let value = getpriority(0, 0);
            if value == -1 && *__errno_location() != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(value)
            }
        }
    }
    pub(super) fn lower() -> Result<(), io::Error> {
        // Increasing niceness needs no privilege; never raise an inherited
        // lower scheduling priority. This dedicated thread need not restore it.
        if current()? < 10 {
            // SAFETY: PRIO_PROCESS/0 changes only this thread on Linux.
            if unsafe { setpriority(0, 0, 10) } != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if current()? < 10 {
            return Err(io::Error::other("CPU niceness did not increase"));
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::{ffi::c_void, io};

    unsafe extern "C" {
        fn pthread_self() -> *mut c_void;
        fn pthread_set_qos_class_self_np(class: u32, relative_priority: i32) -> i32;
        fn pthread_get_qos_class_np(
            thread: *mut c_void,
            class: *mut u32,
            relative_priority: *mut i32,
        ) -> i32;
    }
    pub(super) fn current() -> Result<u32, io::Error> {
        let (mut class, mut relative_priority) = (0, 0);
        // SAFETY: pthread_self is valid and both output pointers are writable.
        let result =
            unsafe { pthread_get_qos_class_np(pthread_self(), &mut class, &mut relative_priority) };
        if result != 0 {
            Err(io::Error::from_raw_os_error(result))
        } else {
            Ok(class)
        }
    }
    pub(super) fn lower() -> Result<(), io::Error> {
        // QOS_CLASS_BACKGROUND. No other pthread or dispatch queue is changed.
        // SAFETY: this public API changes only its calling pthread.
        let result = unsafe { pthread_set_qos_class_self_np(0x09, 0) };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result));
        }
        if current()? != 0x09 {
            return Err(io::Error::other("background QoS was not applied"));
        }
        Ok(())
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    pub(super) fn lower() -> Result<(), std::io::Error> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "background thread scheduling is unavailable",
        ))
    }
}

#[cfg(all(
    test,
    any(target_os = "windows", target_os = "linux", target_os = "macos")
))]
pub(super) fn assert_background() {
    let actual = platform::current().expect("read actual OS thread priority");
    #[cfg(target_os = "windows")]
    assert!(
        actual <= -2,
        "native owner still has foreground CPU priority: {actual}"
    );
    #[cfg(target_os = "linux")]
    assert!(
        actual >= 10,
        "native owner still has foreground niceness: {actual}"
    );
    #[cfg(target_os = "macos")]
    assert_eq!(actual, 0x09, "native owner does not have background QoS");
}

#[cfg(all(
    test,
    any(target_os = "windows", target_os = "linux", target_os = "macos")
))]
#[test]
fn background_policy_changes_only_its_own_thread_and_is_idempotent() {
    let foreground = platform::current().unwrap();
    std::thread::spawn(|| {
        lower_current_thread().unwrap();
        assert_background();
        lower_current_thread().unwrap();
        assert_background();
    })
    .join()
    .unwrap();
    assert_eq!(
        platform::current().unwrap(),
        foreground,
        "foreground priority changed"
    );
}
