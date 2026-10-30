//! Bounded stdio language-service session. Source never enters protocol logs.
//! No implicit installation, retries, server restarts or synchronous UI IO.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
const MAX_MESSAGE: usize = 1024 * 1024;
#[cfg(target_os = "windows")]
pub(super) const URI: &str = "file:///c:/motion-studio-author-preview.wgsl";
#[cfg(not(target_os = "windows"))]
pub(super) const URI: &str = "file:///motion-studio-author-preview.wgsl";
#[cfg(test)]
#[path = "lsp_failures.rs"]
mod failures;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Check,
    Completion,
    Hover,
    Definition,
}
impl Kind {
    fn method(self) -> &'static str {
        match self {
            Self::Check => "textDocument/diagnostic",
            Self::Completion => "textDocument/completion",
            Self::Hover => "textDocument/hover",
            Self::Definition => "textDocument/definition",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Request {
    pub revision: u64,
    pub text: String,
    pub prefix: usize,
    pub position: [usize; 2],
    pub kind: Kind,
}
#[derive(Clone, Debug)]
pub struct Reply {
    pub id: u64,
    pub request: Request,
    pub result: Value,
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub error: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Ready,
    Stopped,
    Failed(String),
}
#[derive(Clone)]
pub struct Snapshot {
    pub status: Status,
    pub flight: Option<(&'static str, Instant)>,
    pub reply: Option<Arc<Reply>>,
    pub diagnostics: Vec<Diagnostic>,
    pub revision: u64,
    pub changed: u64,
}
impl Snapshot {
    fn new() -> Self {
        Self {
            status: Status::Starting,
            flight: Some(("initialize", Instant::now())),
            reply: None,
            diagnostics: vec![],
            revision: 0,
            changed: 0,
        }
    }
}
pub struct Client {
    tx: mpsc::SyncSender<Request>,
    stop: mpsc::SyncSender<()>,
    state: Arc<Mutex<Snapshot>>,
}
impl Client {
    pub fn start(path: PathBuf) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(Snapshot::new()));
        let shared = state.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        let (stop, stopped) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("motion-wgsl-lsp".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(path, rx, stopped, &shared)
                }));
                let failure = match result {
                    Ok(Ok(())) => {
                        if let Ok(mut s) = shared.lock() {
                            s.status = Status::Stopped;
                            s.flight = None;
                            s.changed = s.changed.wrapping_add(1);
                        }
                        None
                    }
                    Ok(Err(e)) => Some(e),
                    Err(_) => Some("language worker panicked; source draft retained".into()),
                };
                if let Some(cause) = failure {
                    log::error!(
                        "plugin.language phase=session cause={}",
                        crate::fallback_observation::short(&cause)
                    );
                    // Logging is outside the language-state lock.
                    if let Ok(mut s) = shared.lock() {
                        s.status = Status::Failed(cause);
                        s.flight = None;
                        s.changed = s.changed.wrapping_add(1);
                    }
                }
            })
            .map_err(|e| format!("start language worker: {e}"))?;
        Ok(Self { tx, stop, state })
    }
    pub fn snapshot(&self) -> Result<Snapshot, String> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "language state lock poisoned".into())
    }
    pub fn submit(&self, request: Request) -> Result<bool, String> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| "language state lock poisoned")?;
        if s.status != Status::Ready || s.flight.is_some() {
            return Ok(false);
        }
        let phase = request.kind.method();
        s.flight = Some((phase, Instant::now()));
        s.changed = s.changed.wrapping_add(1);
        match self.tx.try_send(request) {
            Ok(()) => Ok(true),
            Err(mpsc::TrySendError::Full(_)) => {
                s.flight = None;
                Ok(false)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                s.flight = None;
                Err("language command receiver disconnected".into())
            }
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
    }
}
fn update(state: &Mutex<Snapshot>, edit: impl FnOnce(&mut Snapshot)) -> Result<(), String> {
    let mut s = state.lock().map_err(|_| "language state lock poisoned")?;
    edit(&mut s);
    s.changed = s.changed.wrapping_add(1);
    Ok(())
}
fn send(w: &mut impl Write, value: &Value) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|e| format!("encode LSP: {e}"))?;
    if bytes.len() > MAX_MESSAGE {
        return Err("outbound LSP message exceeds 1 MiB".into());
    }
    write!(w, "Content-Length: {}\r\n\r\n", bytes.len())
        .and_then(|_| w.write_all(&bytes))
        .and_then(|_| w.flush())
        .map_err(|e| format!("write LSP: {e}"))
}
fn read(r: &mut impl BufRead) -> Result<Value, String> {
    let mut length = None;
    let mut bytes = 0;
    loop {
        let mut line = String::new();
        let n = (&mut *r)
            .take(8193)
            .read_line(&mut line)
            .map_err(|e| format!("read LSP header: {e}"))?;
        if n == 0 {
            return Err("language server closed stdout".into());
        }
        bytes += n;
        if bytes > 8192 {
            return Err("LSP header exceeds 8 KiB".into());
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim().split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                if length.is_some() {
                    return Err("duplicate LSP content length".into());
                }
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "invalid LSP content length")?,
                );
            }
        }
    }
    let length = length
        .filter(|n| *n > 0 && *n <= MAX_MESSAGE)
        .ok_or("missing or excessive LSP content length")?;
    let mut data = vec![0; length];
    r.read_exact(&mut data)
        .map_err(|e| format!("read LSP body: {e}"))?;
    serde_json::from_slice(&data)
        .map_err(|e| format!("decode LSP JSON at {}:{}", e.line(), e.column()))
}
fn notify(w: &mut impl Write, method: &str, params: Value) -> Result<(), String> {
    send(w, &json!({"jsonrpc":"2.0","method":method,"params":params}))
}
fn rpc(w: &mut impl Write, id: u64, method: &str, params: Value) -> Result<(), String> {
    send(
        w,
        &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
    )
}
#[cfg(windows)]
#[path = "lsp_process_job.rs"]
mod process_job;
struct ChildOwner(std::process::Child);
impl std::ops::Deref for ChildOwner {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildOwner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildOwner {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {}
            Err(e) => log::error!("plugin.language cleanup phase=inspect cause={e}"),
        }
        if let Err(e) = self.0.kill() {
            log::error!("plugin.language cleanup phase=kill cause={e}");
        }
        if let Err(e) = self.0.wait() {
            log::error!("plugin.language cleanup phase=reap cause={e}");
        }
    }
}
fn run(
    path: PathBuf,
    commands: mpsc::Receiver<Request>,
    stopped: mpsc::Receiver<()>,
    state: &Arc<Mutex<Snapshot>>,
) -> Result<(), String> {
    let began = Instant::now();
    log::info!(
        "plugin.language start request=0 operation=initialize executable={}",
        crate::fallback_observation::short(path.display())
    );
    crate::logging::flush();
    let mut command = Command::new(&path);
    command.current_dir(std::env::temp_dir());
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    #[cfg(windows)]
    let job = process_job::Job::new()?;
    let mut child = ChildOwner(command.spawn().map_err(|e| {
        format!(
            "spawn language server {}: {e}",
            crate::fallback_observation::short(path.display())
        )
    })?);
    #[cfg(windows)]
    job.assign(&child)?;
    let mut stdin = child.stdin.take().ok_or("language stdin missing")?;
    let stdout = child.stdout.take().ok_or("language stdout missing")?;
    let stderr = child.stderr.take().ok_or("language stderr missing")?;
    // Supervisor owns the child independently of possibly blocked pipe IO.
    // Session disposal explicitly closes this one process; it never restarts it.
    let (end, ended) = mpsc::sync_channel::<()>(1);
    let (exit_tx, exit_rx) = mpsc::sync_channel::<Result<(), String>>(1);
    let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let close_state = closing.clone();
    thread::Builder::new()
        .name("motion-wgsl-process".into())
        .spawn(move || {
            // The non-inherited job also closes on abrupt Windows owner exit,
            // when a detached Rust supervisor cannot finish its normal cleanup.
            #[cfg(windows)]
            let _job = job;
            let result = (|| loop {
                if stopped.try_recv().is_ok() || ended.try_recv() != Err(mpsc::TryRecvError::Empty)
                {
                    close_state.store(true, std::sync::atomic::Ordering::Release);
                    let primary = match child.try_wait() {
                        Ok(Some(_)) => Ok(()),
                        Ok(None) => child
                            .kill()
                            .map_err(|e| format!("close language process: {e}")),
                        Err(e) => Err(format!("inspect language process: {e}")),
                    };
                    let reaped = child
                        .wait()
                        .map_err(|e| format!("reap language process: {e}"));
                    if primary.is_err() {
                        if let Err(cleanup) = &reaped {
                            log::error!("plugin.language cleanup phase=reap cause={cleanup}");
                        }
                    }
                    return primary.and(reaped.map(|_| ()));
                }
                if let Some(status) = child
                    .try_wait()
                    .map_err(|e| format!("poll language process: {e}"))?
                {
                    return Err(format!("language server exited: {status}"));
                }
                thread::sleep(Duration::from_millis(20));
            })();
            if let Err(e) = &result {
                log::error!("plugin.language phase=process cause={e}");
            }
            let _ = exit_tx.try_send(result);
        })
        .map_err(|e| format!("start language process supervisor: {e}"))?;
    struct End(mpsc::SyncSender<()>);
    impl Drop for End {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }
    let _end = End(end);
    let (tx, rx) = mpsc::sync_channel::<Result<Value, String>>(8);
    thread::Builder::new()
        .name("motion-wgsl-read".into())
        .spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let message = read(&mut reader);
                let end = message.is_err();
                if tx.send(message).is_err() || end {
                    break;
                }
            }
        })
        .map_err(|e| format!("start language reader: {e}"))?;
    thread::Builder::new()
        .name("motion-wgsl-stderr".into())
        .spawn(move || {
            let mut r = BufReader::new(stderr);
            let mut buffer = [0; 1024];
            let mut count = 0u64;
            loop {
                match r.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        count = count.saturating_add(n as u64);
                    }
                    Err(e) => {
                        log::error!("plugin.language phase=stderr_read cause={e}");
                        break;
                    }
                }
            }
            // Server stderr can contain authored code. Record the bounded byte count,
            // not payloads. Protocol IO and process exit retain their original causes.
            log::debug!("plugin.language server_stderr_bytes={count}");
        })
        .map_err(|e| format!("start language stderr reader: {e}"))?;
    rpc(
        &mut stdin,
        0,
        "initialize",
        json!({"processId":std::process::id(),"rootUri":null,"capabilities":{"general":{"positionEncodings":["utf-16"]},"textDocument":{"diagnostic":{"relatedDocumentSupport":false},"completion":{"completionItem":{"snippetSupport":false}},"hover":{"contentFormat":["plaintext","markdown"]}}},"clientInfo":{"name":"Motion Studio","version":env!("CARGO_PKG_VERSION")}}),
    )?;
    let mut initialized = false;
    let mut current: Option<Request> = None;
    let mut version = 0i32;
    let mut id = 0u64;
    let mut pending: Option<(u64, Request, Instant)> = None;
    let mut capabilities = Value::Null;
    loop {
        match exit_rx.try_recv() {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(e)) => return Err(e),
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err("language process supervisor disconnected".into())
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        match rx.recv_timeout(Duration::from_millis(10)) {
            Ok(Err(e)) => {
                if closing.load(std::sync::atomic::Ordering::Acquire) {
                    return exit_rx
                        .recv()
                        .map_err(|_| "language close supervisor disconnected".to_owned())?;
                }
                return match exit_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(result) => result,
                    Err(_) => Err(e),
                };
            }
            Ok(Ok(message)) => {
                if message.get("method").is_some() {
                    if let Some(server_id) = message.get("id") {
                        let method = message["method"].as_str().unwrap_or("");
                        let result = match method {
                            "workspace/configuration" => Some(Value::Array(
                                message["params"]["items"]
                                    .as_array()
                                    .map_or_else(Vec::new, |v| vec![Value::Null; v.len().min(64)]),
                            )),
                            "workspace/workspaceFolders" => Some(json!([])),
                            "client/registerCapability" | "window/workDoneProgress/create" => {
                                Some(Value::Null)
                            }
                            _ => None,
                        };
                        let response = if let Some(result) = result {
                            json!({"jsonrpc":"2.0","id":server_id,"result":result})
                        } else {
                            json!({"jsonrpc":"2.0","id":server_id,"error":{"code":-32601,"message":"Client method not supported"}})
                        };
                        send(&mut stdin, &response)?;
                    } else if message["method"] == "textDocument/publishDiagnostics" {
                        if let Some(request) = &current {
                            let params = &message["params"];
                            // After edits, unversioned messages cannot be proven current.
                            let matches = params["uri"] == URI
                                && (params["version"].as_i64() == Some(version as i64)
                                    || (version == 1 && params.get("version").is_none()));
                            if matches {
                                let diagnostics = params["diagnostics"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .take(32)
                                    .filter_map(|d| {
                                        let line = d["range"]["start"]["line"].as_u64()? as usize;
                                        let column =
                                            d["range"]["start"]["character"].as_u64()? as usize;
                                        (line >= request.prefix
                                            && line < request.text.lines().count())
                                        .then(|| Diagnostic {
                                            line: line - request.prefix,
                                            column,
                                            message: bounded(
                                                d["message"]
                                                    .as_str()
                                                    .unwrap_or("Language diagnostic"),
                                                2048,
                                            ),
                                            error: d["severity"].as_u64() == Some(1),
                                        })
                                    })
                                    .collect();
                                update(state, |s| {
                                    s.diagnostics = diagnostics;
                                    s.revision = request.revision;
                                })?;
                            }
                        }
                    }
                } else if message["id"].as_u64() == Some(0) && !initialized {
                    if message.get("error").is_some() {
                        return Err(format!(
                            "initialize rejected: {}",
                            bounded(
                                message["error"]["message"]
                                    .as_str()
                                    .unwrap_or("unknown cause"),
                                512
                            )
                        ));
                    }
                    capabilities = message["result"]["capabilities"].clone();
                    if capabilities["positionEncoding"]
                        .as_str()
                        .is_some_and(|v| v != "utf-16")
                    {
                        return Err("language server selected unsupported position encoding".into());
                    }
                    notify(&mut stdin, "initialized", json!({}))?;
                    initialized = true;
                    update(state, |s| {
                        s.status = Status::Ready;
                        s.flight = None;
                    })?;
                    log::debug!(
                        "plugin.language done request=0 operation=initialize elapsed_ms={}",
                        began.elapsed().as_millis()
                    );
                } else if let Some((expected, request, start)) = pending.take() {
                    if message["id"].as_u64() == Some(expected) {
                        let result = if let Some(e) = message.get("error") {
                            let cause = bounded(
                                e["message"].as_str().unwrap_or("language request rejected"),
                                512,
                            );
                            log::error!("plugin.language error request={expected} revision={} operation={} elapsed_ms={} code={} cause={cause}",request.revision,request.kind.method(),start.elapsed().as_millis(),e["code"]);
                            json!({"languageError":cause})
                        } else {
                            message["result"].clone()
                        };
                        log::debug!("plugin.language done request={expected} revision={} operation={} elapsed_ms={}",request.revision,request.kind.method(),start.elapsed().as_millis());
                        update(state, |s| {
                            if request.kind == Kind::Check && result.get("languageError").is_none()
                            {
                                s.diagnostics = diagnostics(&result["items"], &request);
                                s.revision = request.revision;
                            } else {
                                s.reply = Some(Arc::new(Reply {
                                    id: expected,
                                    request,
                                    result,
                                }));
                            }
                            s.flight = None;
                        })?;
                    } else {
                        pending = Some((expected, request, start));
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("language reader disconnected".into())
            }
        }
        if initialized && pending.is_none() {
            match commands.try_recv() {
                Ok(request) => {
                    id = id
                        .checked_add(1)
                        .ok_or("language request identity exhausted")?;
                    log::info!(
                        "plugin.language start request={id} revision={} operation={}",
                        request.revision,
                        request.kind.method()
                    );
                    crate::logging::flush();
                    let start = Instant::now();
                    let changed = current.as_ref().is_none_or(|r| r.text != request.text);
                    if changed {
                        version = version
                            .checked_add(1)
                            .ok_or("language document version exhausted")?;
                        if current.is_none() {
                            notify(
                                &mut stdin,
                                "textDocument/didOpen",
                                json!({"textDocument":{"uri":URI,"languageId":"wgsl","version":version,"text":request.text}}),
                            )?;
                        } else {
                            notify(
                                &mut stdin,
                                "textDocument/didChange",
                                json!({"textDocument":{"uri":URI,"version":version},"contentChanges":[{"text":request.text}]}),
                            )?;
                        }
                    }
                    current = Some(request.clone());
                    if request.kind == Kind::Check
                        && capabilities
                            .get("diagnosticProvider")
                            .is_none_or(|v| v.is_null() || *v == Value::Bool(false))
                    {
                        update(state, |s| {
                            s.flight = None;
                            if changed {
                                s.diagnostics.clear();
                            }
                            s.revision = request.revision;
                        })?;
                        log::debug!(
                            "plugin.language done request={id} operation=sync elapsed_ms={}",
                            start.elapsed().as_millis()
                        );
                    } else {
                        let key = match request.kind {
                            Kind::Completion => "completionProvider",
                            Kind::Hover => "hoverProvider",
                            Kind::Definition => "definitionProvider",
                            Kind::Check => "diagnosticProvider",
                        };
                        if capabilities
                            .get(key)
                            .is_none_or(|v| v.is_null() || *v == Value::Bool(false))
                        {
                            update(state, |s| {
                                s.reply = Some(Arc::new(Reply {
                                    id,
                                    request,
                                    result: json!({"languageError":"Server does not support this feature"}),
                                }));
                                s.flight = None;
                            })?;
                        } else {
                            let params = if request.kind == Kind::Check {
                                json!({"textDocument":{"uri":URI}})
                            } else {
                                let mut p = json!({"textDocument":{"uri":URI},"position":{"line":request.position[0]+request.prefix,"character":request.position[1]}});
                                if request.kind == Kind::Completion {
                                    p["context"] = json!({"triggerKind":1});
                                }
                                p
                            };
                            rpc(&mut stdin, id, request.kind.method(), params)?;
                            pending = Some((id, request, start));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            }
        }
    }
}
pub fn bounded(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}
fn diagnostics(items: &Value, request: &Request) -> Vec<Diagnostic> {
    items
        .as_array()
        .into_iter()
        .flatten()
        .take(32)
        .filter_map(|d| {
            let line = d["range"]["start"]["line"].as_u64()? as usize;
            let column = d["range"]["start"]["character"].as_u64()? as usize;
            (line >= request.prefix && line < request.text.lines().count()).then(|| Diagnostic {
                line: line - request.prefix,
                column,
                message: bounded(d["message"].as_str().unwrap_or("Language diagnostic"), 2048),
                error: d["severity"].as_u64() == Some(1),
            })
        })
        .collect()
}
/// LSP positions count UTF-16 code units, never Rust bytes or displayed glyphs.
pub fn position(text: &str, byte: usize) -> Option<[usize; 2]> {
    let prefix = text.get(..byte)?;
    Some([
        prefix.bytes().filter(|b| *b == b'\n').count(),
        prefix.rsplit('\n').next()?.encode_utf16().count(),
    ])
}
pub fn byte(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut base = 0;
    for (n, part) in text.split('\n').enumerate() {
        if n == line {
            let mut units = 0;
            for (i, c) in part.char_indices() {
                if units == column {
                    return Some(base + i);
                }
                units += c.len_utf16();
                if units > column {
                    return None;
                }
            }
            return (units == column).then_some(base + part.len());
        }
        base += part.len() + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_bounds_and_utf16_positions() {
        let text = "a😀中\nhello";
        assert_eq!(position(text, 5), Some([0, 3]));
        assert_eq!(byte(text, 0, 3), Some(5));
        assert_eq!(byte(text, 0, 2), None);
        assert_eq!(byte(text, 1, 5), Some(text.len()));
        let mut buffer = vec![];
        send(&mut buffer, &json!({"id":1,"text":"中😀"})).unwrap();
        assert_eq!(read(&mut std::io::Cursor::new(buffer)).unwrap()["id"], 1);
        assert!(read(&mut std::io::Cursor::new(
            b"Content-Length: 1048577\r\n\r\n"
        ))
        .is_err());
        assert!(read(&mut std::io::Cursor::new(
            b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}"
        ))
        .is_err());
    }
    #[test]
    #[ignore = "explicit real wgsl-analyzer acceptance; MOTION_WGSL_ANALYZER required"]
    fn real_language_server_completion_hover_definition_and_diagnostics() {
        let path =
            std::env::var_os("MOTION_WGSL_ANALYZER").expect("real language server path required");
        let client = Client::start(path.into()).unwrap();
        let wait = |predicate: &dyn Fn(&Snapshot) -> bool| -> Snapshot {
            let start = Instant::now();
            loop {
                let s = client.snapshot().unwrap();
                assert!(!matches!(s.status, Status::Failed(_)), "{:?}", s.status);
                if predicate(&s) {
                    return s;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(30),
                    "language request did not complete"
                );
                thread::sleep(Duration::from_millis(10));
            }
        };
        wait(&|s| s.status == Status::Ready);
        let mut project = super::super::tests::example();
        let source="// 中文😀\nfn helper(v: f32) -> f32 { return v * 2.0; }\nfn plugin_fx(p: vec2<f32>) -> vec4<f32> {\n  let amount = param_amount();\n  return vec4<f32>(helper(amount));\n}\n";
        if let motion_graph::plugin_project::PluginLogic::Wgsl { document } = &mut project.logic {
            document.source = source.into();
        }
        let (text, prefix) = project.wgsl_language_document().unwrap().unwrap();
        let position = position(source, source.find("param_amount").unwrap() + 4).unwrap();
        assert!(client
            .submit(Request {
                revision: 7,
                text: text.clone(),
                prefix,
                position,
                kind: Kind::Hover
            })
            .unwrap());
        let hover = wait(&|s| {
            s.reply
                .as_ref()
                .is_some_and(|r| r.request.kind == Kind::Hover)
        });
        // Pinned upstream 659b968: ide/src/hover.rs explicitly returns None.
        // Empty hover is a normal capability gap, never fabricated type text.
        assert!(hover.reply.unwrap().result.is_null());
        assert!(client
            .submit(Request {
                revision: 7,
                text: text.clone(),
                prefix,
                position,
                kind: Kind::Definition
            })
            .unwrap());
        let definition = wait(&|s| {
            s.reply
                .as_ref()
                .is_some_and(|r| r.request.kind == Kind::Definition)
        });
        let definition = definition.reply.unwrap().result.clone();
        let location = definition
            .as_array()
            .and_then(|a| a.first())
            .unwrap_or(&definition);
        assert!(
            location["range"]["start"]["line"].as_u64().unwrap() < prefix as u64,
            "host helper must resolve in generated declarations: {location}"
        );
        assert!(client
            .submit(Request {
                revision: 7,
                text: text.clone(),
                prefix,
                position,
                kind: Kind::Completion
            })
            .unwrap());
        let completion = wait(&|s| {
            s.reply
                .as_ref()
                .is_some_and(|r| r.request.kind == Kind::Completion)
        });
        let result = &completion.reply.unwrap().result;
        let items = result
            .as_array()
            .or_else(|| result["items"].as_array())
            .expect("completion list");
        assert!(
            items.iter().any(|i| i["label"]
                .as_str()
                .is_some_and(|s| s.contains("param_amount"))),
            "real host-parameter completion missing: {result}"
        );
        let bad="fn plugin_fx(p: vec2<f32>) -> vec4<f32> {\n  let broken = ;\n  return vec4<f32>(1.0);\n}";
        if let motion_graph::plugin_project::PluginLogic::Wgsl { document } = &mut project.logic {
            document.source = bad.into();
        }
        let (text, prefix) = project.wgsl_language_document().unwrap().unwrap();
        assert!(client
            .submit(Request {
                revision: 8,
                text,
                prefix,
                position: [1, 4],
                kind: Kind::Check
            })
            .unwrap());
        let diagnostics = wait(&|s| s.revision == 8 && !s.diagnostics.is_empty());
        assert!(
            diagnostics
                .diagnostics
                .iter()
                .any(|d| d.line == 1 && d.error),
            "author row mapping: {:?}",
            diagnostics.diagnostics
        );
    }
}
