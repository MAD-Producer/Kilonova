//! Bounded desktop timing records. Recording never performs file/network IO.
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{sync_channel, Receiver, Sender, SyncSender},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const RECENT_LIMIT: usize = 128;
const STALL_LIMIT: usize = 512;
const STALL_US: u64 = 16_000;
const MAX_REPORT_BYTES: usize = 2 * 1024 * 1024;
#[cfg(test)]
mod native_test_evidence;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub action: &'static str,
    pub panel: Option<u32>,
    pub playing: bool,
    pub pointer_dragging: bool,
    pub frame: f64,
    pub layers: usize,
    pub effects: usize,
    pub composition_size: [u64; 2],
}
impl Context {
    pub fn from_state(state: &Value) -> Self {
        let layers = state["project"]["layers"].as_array();
        Self {
            frame: state["frame"]
                .as_f64()
                .filter(|f| f.is_finite())
                .unwrap_or(0.),
            layers: layers.map_or(0, Vec::len),
            effects: layers.map_or(0, |ls| {
                ls.iter()
                    .map(|l| l["effects"].as_array().map_or(0, Vec::len))
                    .sum()
            }),
            composition_size: [
                state["project"]["width"].as_u64().unwrap_or(0),
                state["project"]["height"].as_u64().unwrap_or(0),
            ],
            ..Default::default()
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Timing {
    request: u64,
    at_us: u64,
    category: &'static str,
    operation: &'static str,
    wall_us: u64,
    queue_wait_us: Option<u64>,
    work_us: Option<u64>,
    context: Context,
}
#[derive(Default)]
struct State {
    recent: VecDeque<Timing>,
    stalls: VecDeque<Timing>,
    total_stalls: u64,
    last_checkpoint: Option<Instant>,
}
enum Job {
    Checkpoint(Option<Sender<Result<(), String>>>),
    Export(PathBuf, Sender<Result<(), String>>),
}
pub struct Diagnostics {
    began: Instant,
    started_unix_ms: u64,
    state: Mutex<State>,
    context: Mutex<Context>,
    gpu: Mutex<Option<Value>>,
    #[cfg(test)]
    native_phase: Mutex<Option<native_test_evidence::Phase>>,
    jobs: SyncSender<Job>,
    enabled: AtomicBool,
    configured: bool,
    sequence: AtomicU64,
    dropped_records: AtomicU64,
    dropped_jobs: AtomicU64,
    queued_uploads: AtomicU64,
    io_errors: AtomicU64,
}
impl Diagnostics {
    pub fn new(path: Option<PathBuf>, dsn: Option<String>) -> Arc<Self> {
        Self::with_transport(path,dsn,None)
    }
    fn with_transport(path:Option<PathBuf>,dsn:Option<String>,transport:Option<Arc<dyn sentry::TransportFactory>>)->Arc<Self> {
        let dsn = dsn.and_then(|s| match s.parse::<sentry::types::Dsn>() {
            Ok(dsn) => Some(dsn),
            Err(_) => {
                // Never copy a potentially credential-bearing DSN into logs.
                log::warn!("diagnostics upload disabled: invalid MOTION_SENTRY_DSN");
                None
            }
        });
        let (jobs, receive) = sync_channel(4);
        let this = Arc::new(Self {
            began: Instant::now(),
            started_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis().min(u128::from(u64::MAX)) as u64,
            state: Mutex::new(State::default()),
            context: Mutex::new(Context::default()),
            gpu: Mutex::new(None),
            #[cfg(test)]
            native_phase: Mutex::new(None),
            jobs,
            enabled: AtomicBool::new(false),
            configured: dsn.is_some(),
            sequence: AtomicU64::new(1),
            dropped_records: AtomicU64::new(0),
            dropped_jobs: AtomicU64::new(0),
            queued_uploads: AtomicU64::new(0),
            io_errors: AtomicU64::new(0),
        });
        let weak = Arc::downgrade(&this);
        if let Err(error) = std::thread::Builder::new()
            .name("motion-diagnostics".into())
            .spawn(move || reporter(weak, receive, path, dsn,transport)) {
            this.io_errors.fetch_add(1, Ordering::Relaxed);
            log::error!("diagnostics reporter could not start: {error}");
        }
        this
    }
    pub fn next_request(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }
    pub fn context(&self) -> Context {
        { static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.context, &FAILURES, "diagnostics.context.read", |value| value.clone()) }
    }
    pub fn set_context(&self, context: Context) {
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.context, &FAILURES, "diagnostics.context.write", |value| *value = context);
    }
    pub fn set_gpu(&self, info: wgpu::AdapterInfo) {
        let gpu = json!({"name":info.name.chars().take(128).collect::<String>(),
            "backend":format!("{:?}",info.backend),"deviceType":format!("{:?}",info.device_type),"vendor":info.vendor,
            "driver":info.driver.chars().take(128).collect::<String>()});
        static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        crate::fallback_observation::recovered(&self.gpu, &FAILURES, "diagnostics.gpu.write", |value| *value = Some(gpu));
    }
    pub fn record(
        &self,
        request: u64,
        category: &'static str,
        operation: &'static str,
        wall: Duration,
        queue: Option<Duration>,
        work: Option<Duration>,
        context: Context,
    ) {
        let wall_us = micros(wall);
        let event = Timing {
            request,
            at_us: micros(self.began.elapsed()),
            category,
            operation,
            wall_us,
            queue_wait_us: queue.map(micros),
            work_us: work.map(micros),
            context,
        };
        let Ok(mut state) = self.state.try_lock() else {
            self.dropped_records.fetch_add(1, Ordering::Relaxed);
            return;
        };
        push_bounded(&mut state.recent, event.clone(), RECENT_LIMIT);
        let slow = wall_us >= STALL_US;
        if slow {
            state.total_stalls += 1;
            push_bounded(&mut state.stalls, event, STALL_LIMIT);
        }
        let checkpoint = slow
            && state
                .last_checkpoint
                .is_none_or(|i| i.elapsed() >= Duration::from_secs(5));
        if checkpoint {
            state.last_checkpoint = Some(Instant::now());
        }
        drop(state);
        if checkpoint && self.jobs.try_send(Job::Checkpoint(None)).is_err() {
            self.dropped_jobs.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if enabled && !self.configured {
            return Err("diagnostics upload endpoint is not configured".into());
        }
        self.enabled.store(enabled, Ordering::Release);
        Ok(())
    }
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }
    pub fn configured(&self) -> bool {
        self.configured
    }
    pub fn status(&self) -> Value {
        json!({"configured":self.configured,"enabled":self.enabled(),"queuedUploads":self.queued_uploads.load(Ordering::Relaxed),
        "droppedRecords":self.dropped_records.load(Ordering::Relaxed),"droppedJobs":self.dropped_jobs.load(Ordering::Relaxed),"localWriteErrors":self.io_errors.load(Ordering::Relaxed)})
    }
    pub fn report(&self) -> Value {
        static GPU_FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        static STATE_FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
        let gpu = crate::fallback_observation::recovered(&self.gpu, &GPU_FAILURES, "diagnostics.gpu.report", |value| value.clone());
        crate::fallback_observation::recovered(&self.state, &STATE_FAILURES, "diagnostics.state.report", |state| {
        json!({"schemaVersion":1,"application":{"name":"Motion Studio","version":env!("CARGO_PKG_VERSION"),
            "build":if cfg!(debug_assertions){"debug"}else{"release"},"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,
            "commit":build_commit()},
            "startedUnixMs":self.started_unix_ms,"elapsedUs":micros(self.began.elapsed()),"gpu":gpu,
            "timingContract":"CPU wall clock; surface timings include driver/queue waiting, not GPU timestamp measurements",
            "stallThresholdUs":STALL_US,"totalStalls":state.total_stalls,"recent":state.recent,"stalls":state.stalls,"reporting":self.status()})
        })
    }
    pub fn export(&self, path: PathBuf) -> Result<Receiver<Result<(), String>>, String> {
        let (reply, receive) = std::sync::mpsc::channel();
        self.jobs
            .try_send(Job::Export(path, reply))
            .map_err(|_| "diagnostics reporter is busy or stopped".to_string())?;
        Ok(receive)
    }
    pub fn checkpoint(&self) -> Result<Receiver<Result<(), String>>, String> {
        let (reply, receive) = std::sync::mpsc::channel();
        self.jobs
            .try_send(Job::Checkpoint(Some(reply)))
            .map_err(|_| "diagnostics reporter is busy or stopped".to_string())?;
        Ok(receive)
    }
}
fn micros(d: Duration) -> u64 {
    d.as_micros().min(u128::from(u64::MAX)) as u64
}
fn build_commit()->Option<&'static str> {
    option_env!("MOTION_BUILD_COMMIT").or(option_env!("GITHUB_SHA"))
        .filter(|s|s.len()==40&&s.bytes().all(|b|b.is_ascii_hexdigit()))
}
fn push_bounded<T>(values: &mut VecDeque<T>, value: T, limit: usize) {
    if values.len() == limit {
        values.pop_front();
    }
    values.push_back(value);
}
fn write_report(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if bytes.len() > MAX_REPORT_BYTES {
        return Err("diagnostics report exceeds its byte budget".into());
    }
    let parent = path
        .parent()
        .ok_or("diagnostics destination has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = parent.join(format!(
        ".motion-diagnostics-{}-{}.tmp",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut created = false;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp)?;
        created = true;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })()
    .map_err(|e| e.to_string());
    if result.is_err() && created {
        if let Err(error) = std::fs::remove_file(&tmp) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!("diagnostics temporary cleanup failed path={}: {error}", tmp.display());
            }
        }
    }
    result
}
fn reporter(
    owner: Weak<Diagnostics>,
    receive: Receiver<Job>,
    local: Option<PathBuf>,
    dsn: Option<sentry::types::Dsn>,
    transport: Option<Arc<dyn sentry::TransportFactory>>,
) {
    log::info!("diagnostics reporter started");
    crate::logging::flush();
    let mut client: Option<sentry::Client> = None;
    let mut last_upload: Option<Instant> = None;
    let mut uploads = 0;
    let mut uploaded_stalls=0;
    let mut request = 0u64;
    let stop_reason = loop {
        let Ok(job) = receive.recv() else {
            break "channel_closed";
        };
        let Some(owner) = owner.upgrade() else {
            break "owner_dropped";
        };
        let (destination, reply, auto, operation) = match job {
            Job::Export(p, r) => (Some(p), Some(r), false, "export"),
            Job::Checkpoint(r) => (local.clone(), r, true, "checkpoint"),
        };
        request = request.wrapping_add(1);
        let began = Instant::now();
        let target = destination.as_ref().map_or_else(
            || "<disabled>".to_string(), |path| path.display().to_string(),
        );
        // This thread owns serialization and IO. Record before either can hang;
        // record() remains free of file IO, including the Present hot path.
        log::info!("diagnostics start #{request} {operation} path={target}");
        crate::logging::flush();
        let report = owner.report();
        let encoded = serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("diagnostics serialization failed: {error}"))
            .and_then(|bytes| if bytes.len() <= MAX_REPORT_BYTES { Ok(bytes) }
                else { Err("diagnostics report exceeds its byte budget".into()) });
        let bytes = match encoded {
            Ok(bytes) => bytes,
            Err(error) => {
                owner.io_errors.fetch_add(1, Ordering::Relaxed);
                if let Some(reply) = reply { let _ = reply.send(Err(error.clone())); }
                log::error!("diagnostics failed #{request} {operation} {}ms path={target} reason={error}", began.elapsed().as_millis());
                continue;
            }
        };
        let result = destination
            .as_ref()
            .map_or(Ok(()), |p| write_report(p, &bytes));
        if result.is_err() {
            owner.io_errors.fetch_add(1, Ordering::Relaxed);
        }
        if auto
            && report["totalStalls"].as_u64().is_some_and(|n| n > uploaded_stalls)
            && owner.enabled()
            && uploads < 12
            && last_upload.is_none_or(|i| i.elapsed() >= Duration::from_secs(300))
        {
            if let Some(dsn) = dsn.clone() {
                let c = client.get_or_insert_with(|| {
                    let mut options=sentry::ClientOptions::new()
                        .default_integrations(false).send_default_pii(false)
                        .attach_stacktrace(false)
                        .transport_channel_capacity(4).shutdown_timeout(Duration::from_millis(500));
                    options.transport=transport.clone();
                    options.dsn=Some(dsn);
                    options.auto_session_tracking=false;
                    sentry::Client::from_config(sentry::apply_defaults(
                        options,
                    ))
                });
                // Recheck consent after serialization/client creation.
                if owner.enabled() {
                    let (event, scope) = sentry_report(&report, bytes);
                    let id = c.capture_event(event, Some(&scope));
                    if !id.is_nil() {
                        owner.queued_uploads.fetch_add(1, Ordering::Relaxed);
                        uploads += 1;
                        uploaded_stalls=report["totalStalls"].as_u64().unwrap_or(0);
                        last_upload = Some(Instant::now());
                    }
                }
            }
        }
        if let Some(reply) = reply {
            let _ = reply.send(result.clone());
        }
        // Deliver the existing result before console stderr IO, which can block
        // when an MCP client pipes errors without draining them.
        match result {
            Ok(()) => log::debug!("diagnostics done #{request} {operation} {}ms path={target}", began.elapsed().as_millis()),
            Err(error) => log::error!("diagnostics failed #{request} {operation} {}ms path={target} reason={error}", began.elapsed().as_millis()),
        }
    };
    log::info!("diagnostics reporter stopping reason={stop_reason}");
    crate::logging::flush();
    if let Some(client) = client {
        let began = Instant::now();
        if !client.flush(Some(Duration::from_millis(500))) {
            log::warn!("diagnostics reporter upload flush incomplete wait_limit_ms=500 elapsed_ms={}", began.elapsed().as_millis());
        }
    }
    log::info!("diagnostics reporter stopped");
    crate::logging::flush();
}
fn sentry_report(
    report: &Value,
    bytes: Vec<u8>,
) -> (sentry::protocol::Event<'static>, sentry::Scope) {
    let operation = report["stalls"]
        .as_array()
        .and_then(|s| s.last())
        .and_then(|e| e["operation"].as_str())
        .unwrap_or("unknown");
    let event=sentry::protocol::Event {message:Some(format!("Motion Studio desktop stall: {operation}")),level:sentry::Level::Warning,
        release:Some(format!("motion-studio@{}{}",env!("CARGO_PKG_VERSION"),
            build_commit().map_or(String::new(),|s|format!("+{}",&s[..12]))).into()),
        fingerprint:vec!["motion-studio-desktop-stall".into(),operation.to_string().into()].into(),
        extra:[("stall_summary".into(),json!({"totalStalls":report["totalStalls"],"last":report["stalls"].as_array().and_then(|a|a.last()),"gpu":report["gpu"]}))].into(),
        ..Default::default()};
    let mut scope = sentry::Scope::default();
    scope.add_attachment(sentry::protocol::Attachment {
        buffer: bytes,
        filename: "motion-studio-stalls.json".into(),
        content_type: Some("application/json".into()),
        ty: None,
    });
    (event, scope)
}

#[cfg(test)]
mod tests;
