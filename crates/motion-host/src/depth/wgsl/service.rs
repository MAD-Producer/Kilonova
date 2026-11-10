//! Cancellation-aware, single-request file mailbox for an owned native child.
//! Payloads stay in fresh owned directories, stdout must stay empty, and reader
//! threads retain bounded bytes. A failed service is never restarted implicitly.
use super::*;
use std::{
    io,
    process::Child,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub(super) struct Service {
    // Directory must outlive the direct child and its readers.
    directory: tempfile::TempDir,
    child: Child,
    reaped: bool,
    stdout_written: Arc<AtomicBool>,
    stdout: Option<JoinHandle<io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    execution: Execution,
    sequence: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    schema: u32,
    sequence: u64,
    pid: u32,
    execution: Execution,
}

fn drain(
    mut reader: impl Read,
    maximum: usize,
    written: Option<Arc<AtomicBool>>,
) -> io::Result<Vec<u8>> {
    let mut tail = Vec::with_capacity(maximum);
    let mut buffer = [0; 2048];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            return Ok(tail);
        }
        if let Some(flag) = &written {
            flag.store(true, Ordering::Release);
        }
        let excess = tail.len().saturating_add(n).saturating_sub(maximum);
        tail.drain(..excess.min(tail.len()));
        tail.extend_from_slice(&buffer[n.saturating_sub(maximum)..n]);
    }
}

fn failure_context(primary: Error, root: &Path, diagnostic: &str) -> Error {
    // An idle service did not cause a transform child to fail. Only use the
    // native receipt as a primary cause when that service actually failed.
    let native_failed = read_limited(&root.join("receipt.json"), 65536)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| value["status"] == "failed_original_cause_retained");
    let native_panic = completed_panic(root).is_some() || last_panic(diagnostic).is_some();
    let child = Error::Backend(format!("{primary}; retained_child{diagnostic}"));
    if native_failed || native_panic {
        native_error(child, root)
    } else {
        child
    }
}

impl Service {
    pub fn workspace(&self) -> &Path {
        self.directory.path()
    }
    fn output(&self) -> PathBuf {
        self.workspace().join("service")
    }

    pub fn start(
        config: &Config,
        expected: &Attestation,
        input: &Input,
        cancellation: &Cancellation,
    ) -> Result<Self> {
        cancellation.check()?;
        fs::create_dir_all(&config.scratch)?;
        let directory = tempfile::Builder::new()
            .prefix("motion-depth-retained-")
            .tempdir_in(&config.scratch)?;
        // Canonicalize path spellings once, including Windows extended paths;
        // the worker validates command paths against this exact owned root.
        let root = directory.path().canonicalize()?;
        let request = root.join("resolve.json");
        fs::write(
            &request,
            serde_json::to_vec(&serde_json::json!({"version":1,
            "program":config.program,"program_sha256":PROGRAM_SHA256,
            "requested_backend":config.hardware.requested()}))?,
        )?;
        let identity = root.join("execution.json");
        fs::write(&identity, serde_json::to_vec(&expected.native)?)?;
        let mut command = Command::new(&config.worker);
        command
            .arg("--serve")
            .arg(identity)
            .arg(request)
            .arg(root.join("service"))
            .env("TMP", &root)
            .env("TEMP", &root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        log::info!(
            "operation=depth_wgsl asset={} phase=start_retained_service start",
            input.key().source.asset
        );
        log::logger().flush();
        let child = command.spawn()?;
        let mut owner = Self {
            directory,
            child,
            reaped: false,
            stdout_written: Arc::new(AtomicBool::new(false)),
            stdout: None,
            stderr: None,
            execution: expected.native.clone(),
            sequence: 0,
        };
        let result = (|| {
            let stdout = owner
                .child
                .stdout
                .take()
                .ok_or(Error::Invalid("missing service stdout"))?;
            let stderr = owner
                .child
                .stderr
                .take()
                .ok_or(Error::Invalid("missing service stderr"))?;
            let written = owner.stdout_written.clone();
            owner.stdout = Some(
                thread::Builder::new()
                    .name("depth-service-stdout".into())
                    .spawn(move || drain(stdout, 2048, Some(written)))?,
            );
            owner.stderr = Some(
                thread::Builder::new()
                    .name("depth-service-stderr".into())
                    .spawn(move || drain(stderr, 8192, None))?,
            );
            owner.wait_ack(0, cancellation)
        })();
        if let Err(primary) = result {
            return Err(owner.failure(primary));
        }
        Ok(owner)
    }

    fn wait_ack(&mut self, sequence: u64, cancellation: &Cancellation) -> Result<()> {
        let path = self.output().join("ack.json");
        loop {
            cancellation.check()?;
            if self.stdout_written.load(Ordering::Acquire) {
                return Err(Error::Invalid("retained WGSL worker wrote to stdout"));
            }
            if let Some(status) = self.child.try_wait()? {
                self.reaped = true;
                return Err(Error::Backend(format!(
                    "retained WGSL child exit={status} sequence={sequence}"
                )));
            }
            match fs::metadata(&path) {
                Ok(_) => {
                    let ack: Ack = serde_json::from_slice(&read_limited(&path, 4096)?)?;
                    ack.execution.validate()?;
                    if ack.schema != 1
                        || ack.sequence != sequence
                        || ack.pid != self.child.id()
                        || ack.execution != self.execution
                    {
                        return Err(Error::Invalid(
                            "retained WGSL acknowledgment identity/sequence differs",
                        ));
                    }
                    fs::remove_file(path)?;
                    return cancellation.check();
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn infer(
        &mut self,
        request: &Path,
        output: &Path,
        cancellation: &Cancellation,
    ) -> Result<()> {
        cancellation.check()?;
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(Error::Invalid("retained WGSL sequence exhausted"))?;
        let root = request
            .parent()
            .ok_or(Error::Invalid("missing retained request root"))?
            .canonicalize()?;
        let request = root.join(
            request
                .file_name()
                .ok_or(Error::Invalid("missing request filename"))?,
        );
        let output = root.join(
            output
                .file_name()
                .ok_or(Error::Invalid("missing output filename"))?,
        );
        let bytes = serde_json::to_vec(&serde_json::json!({"schema":1,"sequence":sequence,
            "request":request,"output":output}))?;
        if bytes.len() > 16 * 1024 {
            return Err(Error::Invalid("retained command exceeds byte bound"));
        }
        let out = self.output();
        log::info!(
            "operation=depth_wgsl request={sequence} pid={} phase=wait_retained_inference start",
            self.child.id()
        );
        log::logger().flush();
        fs::write(out.join("command.tmp"), bytes)?;
        fs::rename(out.join("command.tmp"), out.join("command.json"))?;
        self.wait_ack(sequence, cancellation)?;
        self.sequence = sequence;
        Ok(())
    }

    fn terminate(&mut self) -> io::Result<()> {
        if self.reaped {
            return Ok(());
        }
        if self.child.try_wait()?.is_none() {
            if let Err(error) = self.child.kill() {
                if self.child.try_wait()?.is_none() {
                    return Err(error);
                }
            }
            self.child.wait()?;
        }
        self.reaped = true;
        Ok(())
    }
    fn readers(&mut self) -> String {
        let mut diagnostic = String::new();
        for (name, handle) in [
            ("stdout", self.stdout.take()),
            ("stderr", self.stderr.take()),
        ] {
            if let Some(handle) = handle {
                match handle.join() {
                    Ok(Ok(bytes)) => {
                        if !bytes.is_empty() {
                            diagnostic
                                .push_str(&format!(" {name}={}", String::from_utf8_lossy(&bytes)));
                        }
                    }
                    Ok(Err(error)) => diagnostic.push_str(&format!(" {name}_read={error}")),
                    Err(panic) => diagnostic.push_str(&format!(
                        " {name}_panic={}",
                        super::super::panic_cause(panic.as_ref())
                    )),
                }
            }
        }
        diagnostic
            .chars()
            .rev()
            .take(2048)
            .collect::<String>()
            .chars()
            .rev()
            .collect()
    }
    pub fn failure(&mut self, primary: Error) -> Error {
        let cleanup = self.terminate();
        // Join only after successful reap: a failed kill must not block unwind
        // indefinitely waiting for pipes from a still-running process.
        let primary = if cleanup.is_ok() {
            let diagnostic = self.readers();
            if matches!(primary, Error::Canceled) {
                if let Some(cause) = completed_panic(&self.output()) {
                    log::error!(
                        "operation=depth_wgsl phase=retained_panic_during_cancellation cause={cause}"
                    );
                }
                primary
            } else {
                failure_context(primary, &self.output(), &diagnostic)
            }
        } else {
            primary
        };
        match cleanup {
            Ok(()) => primary,
            Err(cleanup) => Error::Cleanup {
                primary: Box::new(primary),
                cleanup,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_service_does_not_displace_an_original_transform_failure() {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("receipt.json"),
            br#"{"status":"running","phase":"wait_retained_depth_request"}"#,
        )
        .unwrap();
        let original = "WGSL phase=prepare elapsed_ms=42 cause=runtime exit=0xc0000005";
        let error = failure_context(
            Error::Backend(original.into()),
            root.path(),
            &"secondary".repeat(300),
        )
        .to_string();
        assert!(error.starts_with(&format!(
            "depth backend failed: depth backend failed: {original}"
        )));
        assert!(!error.contains("native_receipt"));
        assert!(
            error
                .chars()
                .take(512)
                .collect::<String>()
                .contains(original)
        );
        assert!(error.contains("secondary"));
    }
    #[test]
    fn actual_service_failure_still_precedes_a_long_pipe_diagnostic() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("receipt.json"), br#"{"status":"failed_original_cause_retained","phase":"read_hash_finite_current_input","failure":"native-input-digest-sentinel"}"#).unwrap();
        let error = failure_context(
            Error::Backend("original child exit 1".into()),
            root.path(),
            &"backtrace".repeat(300),
        )
        .to_string();
        assert!(
            error
                .chars()
                .take(512)
                .collect::<String>()
                .contains("native-input-digest-sentinel")
        );
        assert!(error.contains("original child exit 1"));
        assert!(error.contains("backtrace"));
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        match self.terminate() {
            Ok(()) => {
                let diagnostic = self.readers();
                if self.stdout_written.load(Ordering::Acquire) {
                    log::error!(
                        "operation=depth_wgsl phase=retained_service_closed cause=worker_wrote_stdout diagnostic={diagnostic}"
                    );
                } else if !diagnostic.is_empty() {
                    log::warn!(
                        "operation=depth_wgsl phase=retained_service_closed diagnostic={diagnostic}"
                    );
                }
            }
            Err(error) => log::error!(
                "operation=depth_wgsl phase=retained_service_cleanup pid={} cause={error}",
                self.child.id()
            ),
        }
    }
}
