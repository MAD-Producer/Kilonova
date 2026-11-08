//! A single owned inference process's lifetime OS high-water marks.
//! This is neither whole-application memory nor physical device allocation.
use crate::depth::{Error, Result};

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

impl ProcessPeak {
    pub(crate) fn validate(&self) -> Result<()> {
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
            Err(Error::Invalid(
                "invalid inference-process memory observation",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
