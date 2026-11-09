//! A single owned inference process's lifetime OS high-water marks.
//! This is neither whole-application memory nor physical device allocation.

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    WindowsProcessMemoryCounters,
    GetrusageLinux,
    GetrusageDarwin,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProcessPeak {
    Observed {
        pid: u32,
        method: Method,
        peak_resident_bytes: u64,
        peak_private_commit_bytes: Option<u64>,
    },
    Unavailable {
        pid: u32,
        cause: String,
    },
}

/// Fixed direct-child stages, in backend execution order. Missing observations
/// stay explicit. This does not enumerate processes or cover runtime descendants.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "backend", rename_all = "snake_case", deny_unknown_fields)]
pub enum Children {
    Onnx {
        resolution: Option<ProcessPeak>,
        inference: Option<ProcessPeak>,
    },
    Wgsl {
        resolution_transform: Option<ProcessPeak>,
        resolution_native: Option<ProcessPeak>,
        prepare: Option<ProcessPeak>,
        inference: Option<ProcessPeak>,
        remap: Option<ProcessPeak>,
    },
}

/// Queried lifetime peaks for a measured request's host and direct children.
/// Queries precede final serialization/exit. Other Jobs instances, descendants
/// and device memory are not covered; this is not a whole-host budget certificate.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub host: ProcessPeak,
    pub children: Children,
}

impl Snapshot {
    pub(crate) fn capture(children: Children) -> Self {
        Self {
            host: ProcessPeak::current(),
            children,
        }
    }

    /// Host queried resident peak plus largest queried child peak. Direct
    /// backend children execute sequentially. This observed sum is NOT a proven
    /// full-lifetime upper bound, nor simultaneous usage or memory attribution.
    /// Missing/unavailable/malformed counters and overflow remain unknown.
    pub fn observed_resident_sum(&self) -> Option<u64> {
        let (count, stages) = match &self.children {
            Children::Onnx {
                resolution,
                inference,
            } => (
                2,
                [resolution.as_ref(), inference.as_ref(), None, None, None],
            ),
            Children::Wgsl {
                resolution_transform,
                resolution_native,
                prepare,
                inference,
                remap,
            } => (
                5,
                [
                    resolution_transform.as_ref(),
                    resolution_native.as_ref(),
                    prepare.as_ref(),
                    inference.as_ref(),
                    remap.as_ref(),
                ],
            ),
        };
        self.host.validate().ok()?;
        let ProcessPeak::Observed {
            pid: host_pid,
            peak_resident_bytes: host,
            ..
        } = &self.host
        else {
            return None;
        };
        let mut largest = 0;
        for peak in &stages[..count] {
            let peak = (*peak)?;
            peak.validate().ok()?;
            let ProcessPeak::Observed {
                pid,
                peak_resident_bytes,
                ..
            } = peak
            else {
                return None;
            };
            if pid == host_pid {
                return None;
            }
            largest = largest.max(*peak_resident_bytes);
        }
        host.checked_add(largest)
    }
}

impl ProcessPeak {
    /// Query only this process, once. Peaks cover its lifetime up to this call;
    /// neither device memory nor the parent/children are included.
    pub fn current() -> Self {
        match query() {
            Ok(peak) => peak,
            Err(error) => Self::Unavailable {
                pid: std::process::id(),
                cause: error
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .take(512)
                    .collect(),
            },
        }
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let valid = match self {
            Self::Observed {
                pid,
                method,
                peak_resident_bytes,
                peak_private_commit_bytes,
            } => {
                *pid != 0
                    && *peak_resident_bytes != 0
                    && match method {
                        Method::WindowsProcessMemoryCounters => {
                            peak_private_commit_bytes.is_some_and(|n| n != 0)
                        }
                        Method::GetrusageLinux | Method::GetrusageDarwin => {
                            peak_private_commit_bytes.is_none()
                        }
                    }
            }
            Self::Unavailable { pid, cause } => {
                *pid != 0
                    && !cause.is_empty()
                    && cause.chars().count() <= 512
                    && !cause.chars().any(char::is_control)
            }
        };
        if valid {
            Ok(())
        } else {
            Err("invalid inference-process memory observation")
        }
    }
}

#[cfg(windows)]
fn query() -> Result<ProcessPeak, String> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX},
        Threading::GetCurrentProcess,
    };
    let mut counters: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of_val(&counters) as u32;
    // The pseudo-handle denotes only this process and must not be closed. The
    // writable EX buffer is fully allocated; cb advertises its documented size.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
            counters.cb,
        )
    };
    if ok == 0 {
        return Err(format!(
            "GetProcessMemoryInfo: {}",
            std::io::Error::last_os_error()
        ));
    }
    let peak = ProcessPeak::Observed {
        pid: std::process::id(),
        method: Method::WindowsProcessMemoryCounters,
        peak_resident_bytes: counters.PeakWorkingSetSize as u64,
        peak_private_commit_bytes: Some(counters.PeakPagefileUsage as u64),
    };
    peak.validate().map_err(str::to_owned)?;
    Ok(peak)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn query() -> Result<ProcessPeak, String> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // RUSAGE_SELF includes this process's threads; no child/process enumeration.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return Err(format!("getrusage: {}", std::io::Error::last_os_error()));
    }
    #[cfg(target_os = "linux")]
    let (method, multiplier) = (Method::GetrusageLinux, 1024u64);
    #[cfg(target_os = "macos")]
    let (method, multiplier) = (Method::GetrusageDarwin, 1u64);
    let resident = u64::try_from(usage.ru_maxrss)
        .ok()
        .and_then(|n| n.checked_mul(multiplier))
        .ok_or_else(|| "invalid resident high-water mark".to_string())?;
    let peak = ProcessPeak::Observed {
        pid: std::process::id(),
        method,
        peak_resident_bytes: resident,
        peak_private_commit_bytes: None,
    };
    peak.validate().map_err(str::to_owned)?;
    Ok(peak)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn query() -> Result<ProcessPeak, String> {
    Err("process peak query unsupported on this operating system".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn observed(pid: u32, resident: u64) -> ProcessPeak {
        ProcessPeak::Observed {
            pid,
            method: Method::GetrusageLinux,
            peak_resident_bytes: resident,
            peak_private_commit_bytes: None,
        }
    }

    #[test]
    fn sequential_stages_keep_unknowns_and_do_not_sum_nonconcurrent_child_peaks() {
        for children in [
            Children::Onnx {
                resolution: Some(observed(2, 100)),
                inference: Some(observed(3, 300)),
            },
            Children::Wgsl {
                resolution_transform: Some(observed(2, 10)),
                resolution_native: Some(observed(3, 20)),
                prepare: Some(observed(4, 300)),
                inference: Some(observed(5, 40)),
                remap: Some(observed(6, 50)),
            },
        ] {
            let snapshot = Snapshot {
                host: observed(1, 1000),
                children,
            };
            assert_eq!(snapshot.observed_resident_sum(), Some(1300));
            let value = serde_json::to_value(&snapshot).unwrap();
            let restored: Snapshot = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(&restored).unwrap(), value);
            assert_eq!(restored.observed_resident_sum(), Some(1300));
            for stage in value["children"]
                .as_object()
                .unwrap()
                .keys()
                .filter(|key| key.as_str() != "backend")
            {
                for missing in [
                    json!(null),
                    json!({"status":"unavailable","pid":2,"cause":"original query failure"}),
                    json!({"status":"observed","pid":1,"method":"getrusage_linux","peak_resident_bytes":1,"peak_private_commit_bytes":null}),
                    json!({"status":"observed","pid":2,"method":"getrusage_linux","peak_resident_bytes":0,"peak_private_commit_bytes":null}),
                ] {
                    let mut invalid = value.clone();
                    invalid["children"][stage] = missing;
                    assert!(serde_json::from_value::<Snapshot>(invalid)
                        .unwrap()
                        .observed_resident_sum()
                        .is_none());
                }
            }
            let mut invalid = value;
            invalid["children"]["unbounded_extra_phase"] = json!({});
            assert!(serde_json::from_value::<Snapshot>(invalid).is_err());
        }
        let overflow = Snapshot {
            host: observed(1, u64::MAX),
            children: Children::Onnx {
                resolution: Some(observed(2, 1)),
                inference: Some(observed(3, 1)),
            },
        };
        assert!(overflow.observed_resident_sum().is_none());
        let unavailable = Snapshot {
            host: ProcessPeak::Unavailable {
                pid: 1,
                cause: "original host query failure".into(),
            },
            children: Children::Onnx {
                resolution: Some(observed(2, 1)),
                inference: Some(observed(3, 1)),
            },
        };
        assert!(unavailable.observed_resident_sum().is_none());
    }

    #[test]
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    fn actual_owned_child_retains_touched_allocation_high_water_marks() {
        const CHILD: &str = "MOTION_PROCESS_PEAK_CONTROL_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let before = ProcessPeak::current();
            let mut allocation = vec![0u8; 64 << 20];
            for page in allocation.chunks_mut(4096) {
                page[0] = 71;
            }
            std::hint::black_box(&allocation);
            let during = ProcessPeak::current();
            drop(allocation);
            let after = ProcessPeak::current();
            println!(
                "MEMORY_CONTROL {}",
                json!({"pid":std::process::id(), "observations":[before,during,after]})
            );
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_memory::tests::actual_owned_child_retains_touched_allocation_high_water_marks", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let rows: Vec<_> = stdout
            .lines()
            .filter_map(|s| s.strip_prefix("MEMORY_CONTROL "))
            .collect();
        assert_eq!(rows.len(), 1);
        let report: serde_json::Value = serde_json::from_str(rows[0]).unwrap();
        let pid = report["pid"].as_u64().unwrap();
        assert_ne!(pid, u64::from(std::process::id()));
        let values = report["observations"].as_array().unwrap();
        assert_eq!(values.len(), 3);
        for row in values {
            serde_json::from_value::<ProcessPeak>(row.clone())
                .unwrap()
                .validate()
                .unwrap();
            assert_eq!(row["status"], "observed");
            assert_eq!(row["pid"], pid);
        }
        let peaks: Vec<_> = values
            .iter()
            .map(|s| s["peak_resident_bytes"].as_u64().unwrap())
            .collect();
        assert!(
            peaks[0] > 0 && peaks[1] >= 64 << 20 && peaks[1] >= peaks[0] && peaks[2] >= peaks[1]
        );
        #[cfg(windows)]
        {
            let commits: Vec<_> = values
                .iter()
                .map(|s| s["peak_private_commit_bytes"].as_u64().unwrap())
                .collect();
            assert!(commits[1] >= 64 << 20 && commits[1] >= commits[0] && commits[2] >= commits[1]);
        }
        #[cfg(not(windows))]
        assert!(values
            .iter()
            .all(|s| s["peak_private_commit_bytes"].is_null()));
    }

    #[test]
    fn peak_formats_keep_missing_platform_counters_unknown() {
        for method in [
            "windows_process_memory_counters",
            "getrusage_linux",
            "getrusage_darwin",
        ] {
            let value = json!({"status":"observed", "pid":75, "method":method,
                "peak_resident_bytes":4096, "peak_private_commit_bytes":
                    if method == "windows_process_memory_counters" { Some(8192) } else { None }});
            let peak: ProcessPeak = serde_json::from_value(value.clone()).unwrap();
            peak.validate().unwrap();
            assert_eq!(serde_json::to_value(peak).unwrap(), value);
            for field in ["pid", "peak_resident_bytes"] {
                let mut invalid = value.clone();
                invalid[field] = json!(0);
                assert!(serde_json::from_value::<ProcessPeak>(invalid)
                    .unwrap()
                    .validate()
                    .is_err());
            }
            let mut invalid = value;
            invalid["peak_private_commit_bytes"] = if method == "windows_process_memory_counters" {
                json!(null)
            } else {
                json!(0)
            };
            assert!(serde_json::from_value::<ProcessPeak>(invalid)
                .unwrap()
                .validate()
                .is_err());
        }
        for field in [
            json!({"status":"observed","pid":75,"method":"guessed","peak_resident_bytes":1}),
            json!({"status":"unavailable","pid":75,"cause":"original","device_peak_bytes":0}),
        ] {
            assert!(serde_json::from_value::<ProcessPeak>(field).is_err());
        }
    }

    #[test]
    fn unavailable_retains_bounded_cause_instead_of_zero_usage() {
        for cause in ["original query failure".to_string(), "x".repeat(512)] {
            let value = json!({"status":"unavailable", "pid":75, "cause":cause});
            let peak: ProcessPeak = serde_json::from_value(value.clone()).unwrap();
            peak.validate().unwrap();
            assert_eq!(serde_json::to_value(peak).unwrap(), value);
        }
        for cause in [
            "".to_string(),
            "x".repeat(513),
            "original\nquery failure".to_string(),
        ] {
            assert!(ProcessPeak::Unavailable { pid: 75, cause }
                .validate()
                .is_err());
        }
    }
}
