//! Bounded background image inspection/copy/proxy decode. The serial engine
//! only publishes a prepared owned file after checking its captured context.
use crate::owned_file::OwnedFile;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
pub(crate) struct Pool(Arc<Mutex<BTreeMap<u64, Arc<Status>>>>);
struct Status {
    request: u64,
    phase: AtomicU8,
    began: Instant,
}
impl Status {
    fn phase(&self) -> &'static str {
        match self.phase.load(Ordering::Acquire) {
            0 => "queued",
            1 => "inspect",
            2 => "copy",
            3 => "decode",
            4 => "ready",
            5 => "cancelled",
            6 => "failed",
            _ => "unwind",
        }
    }
    fn enter(&self, phase: u8) {
        self.phase.store(phase, Ordering::Release);
        log::info!(
            "project image start request={} operation=prepare_library_image phase={}",
            self.request,
            self.phase()
        );
    }
}
/// Counts queued/running work and unconsumed completions together. Dropping a
/// cancelled UI receiver never frees a permit still owned by a blocked worker.
struct Permit {
    request: u64,
    pool: Arc<Mutex<BTreeMap<u64, Arc<Status>>>>,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let poisoned = match self.pool.lock() {
            Ok(mut states) => {
                states.remove(&self.request);
                false
            }
            Err(cause) => {
                let mut states = cause.into_inner();
                states.remove(&self.request);
                true
            }
        };
        if poisoned {
            log::error!("project image observation release failed request={} cause=poisoned observation lock",self.request);
        }
    }
}
struct Observation {
    status: Arc<Status>,
    finished: bool,
}
impl Drop for Observation {
    fn drop(&mut self) {
        if !self.finished {
            let phase = self.status.phase();
            self.status.phase.store(7, Ordering::Release);
            log::error!("project image exited unexpectedly request={} operation=prepare_library_image phase={phase} elapsed_ms={} reason={}",
                self.status.request,self.status.began.elapsed().as_millis(),if std::thread::panicking(){"unwind"}else{"unexpected_return"});
        }
    }
}
struct Completion {
    result: Result<Option<Prepared>, String>,
    _permit: Permit,
}
pub(crate) struct Task {
    status: Arc<Status>,
    cancel: Arc<AtomicBool>,
    receive: mpsc::Receiver<Completion>,
}
impl Task {
    pub(crate) fn start(pool: &Pool, root: PathBuf, file: PathBuf) -> Result<Self, String> {
        Self::start_with_before(pool, root, file, || {})
    }
    fn start_with_before(
        pool: &Pool,
        root: PathBuf,
        file: PathBuf,
        before: impl FnOnce() + Send + 'static,
    ) -> Result<Self, String> {
        let request = NEXT
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |id| id.checked_add(1))
            .map_err(|_| "image import request IDs exhausted".to_string())?;
        let status = Arc::new(Status {
            request,
            phase: AtomicU8::new(0),
            began: Instant::now(),
        });
        {
            let mut states = pool
                .0
                .lock()
                .map_err(|e| format!("image preparation observation lock: {e}"))?;
            if states.len() >= 2 {
                return Err(format!(
                    "image preparation is full: {}",
                    states
                        .values()
                        .map(|s| format!(
                            "request={} phase={} elapsed_ms={}",
                            s.request,
                            s.phase(),
                            s.began.elapsed().as_millis()
                        ))
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
            states.insert(request, status.clone());
        }
        let permit = Permit {
            request,
            pool: pool.0.clone(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let worker_status = status.clone();
        let (send, receive) = mpsc::channel();
        std::thread::Builder::new().name("motion-project-image".into()).spawn(move||{
            let mut observation=Observation {status:worker_status.clone(),finished:false};
            let path=file.to_string_lossy().chars().take(1024).collect::<String>();
            log::info!("project image start request={request} operation=prepare_library_image phase=queued source={path:?}");
            before();
            let result=prepare(root,file,&worker_cancel,&worker_status);
            worker_status.phase.store(match &result {Ok(Some(_))=>4,Ok(None)=>5,Err(_)=>6},Ordering::Release);
            if let Err(cause)=&result {
                log::error!("project image failed request={request} operation=prepare_library_image elapsed_ms={} source={path:?} cause={cause}",worker_status.began.elapsed().as_millis());
            } else {
                log::debug!("project image done request={request} phase={} elapsed_ms={}",worker_status.phase(),worker_status.began.elapsed().as_millis());
            }
            observation.finished=true;
            let _=send.send(Completion {result,_permit:permit});
        }).map_err(|cause|{
            log::error!("project image spawn failed request={request} cause={cause}");cause.to_string()
        })?;
        Ok(Self {
            status,
            cancel,
            receive,
        })
    }
    pub(crate) fn request(&self) -> u64 {
        self.status.request
    }
    pub(crate) fn phase(&self) -> &'static str {
        self.status.phase()
    }
    pub(crate) fn elapsed_ms(&self) -> u128 {
        self.status.began.elapsed().as_millis()
    }
    pub(crate) fn poll(&self) -> Result<Option<Result<Option<Prepared>, String>>, String> {
        match self.receive.try_recv() {
            Ok(completion) => Ok(Some(completion.result)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(format!(
                "image preparation worker disconnected request={} phase={} elapsed_ms={}",
                self.request(),
                self.phase(),
                self.elapsed_ms()
            )),
        }
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}

pub(crate) struct Prepared {
    root: PathBuf,
    relative: String,
    width: u32,
    height: u32,
    file: Option<OwnedFile>,
    request: u64,
}
impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedImage")
            .field("request", &self.request)
            .field("relative", &self.relative)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(file) = self.file.take() {
            if let Err(cause) = file.discard() {
                let path = self
                    .root
                    .join(&self.relative)
                    .to_string_lossy()
                    .chars()
                    .take(1024)
                    .collect::<String>();
                log::error!("project image cleanup failed request={} phase=discard path={path:?} cause={cause}",self.request);
            }
        }
    }
}
impl Prepared {
    pub(crate) fn request(&self) -> u64 {
        self.request
    }
    pub(crate) fn publish(
        mut self,
        session: &mut motion_host::Session,
        folder: Option<u64>,
    ) -> motion_host::Result<serde_json::Value> {
        if self.root != session.root {
            return Err("prepared image belongs to another project".into());
        }
        let p = session.engine.project();
        let id = p
            .assets
            .iter()
            .map(|a| a.id)
            .chain(p.audio_assets.iter().map(|a| a.id))
            .chain(p.video_assets.iter().map(|a| a.id))
            .chain(p.model_assets.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("asset IDs exhausted")?;
        let mut state = session.apply_command_batch(vec![
            motion_core::Command::RegisterAsset {
                asset: motion_core::Asset {
                    id,
                    path: self.relative.clone(),
                    width: self.width,
                    height: self.height,
                },
            },
            motion_core::Command::ProjectLibrary {
                action: motion_core::LibraryAction::MoveItems {
                    items: vec![motion_core::ProjectItem::Image { id }],
                    folder,
                },
            },
        ])?;
        if let Some(file) = self.file.take() {
            file.keep();
        }
        state["assetRegistration"] =
            serde_json::json!({"cancelled":false,"asset":id,"folder":folder});
        Ok(state)
    }
}
fn prepare(
    root: PathBuf,
    file: PathBuf,
    cancel: &AtomicBool,
    status: &Status,
) -> Result<Option<Prepared>, String> {
    let cancelled = || cancel.load(Ordering::Acquire);
    if cancelled() {
        return Ok(None);
    }
    status.enter(1);
    let info =
        motion_host::ops::images::image_info(&file.to_string_lossy()).map_err(|e| e.to_string())?;
    if cancelled() {
        return Ok(None);
    }
    let width = info["width"].as_u64().ok_or("image width missing")? as u32;
    let height = info["height"].as_u64().ok_or("image height missing")? as u32;
    let maximum = info["maxEncodedBytes"]
        .as_u64()
        .ok_or("image byte limit missing")?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let extension = if info["format"] == "Png" {
        "png"
    } else {
        "jpg"
    };
    let relative = format!(
        "assets/import-image-{}-{stamp}-{}.{}",
        std::process::id(),
        status.request,
        extension
    );
    status.enter(2);
    fs::create_dir_all(root.join("assets")).map_err(|e| e.to_string())?;
    if !root
        .join("assets")
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(root.canonicalize().map_err(|e| e.to_string())?)
    {
        return Err("image asset folder resolves outside project".into());
    }
    let output = OwnedFile::create(&root.join(&relative)).map_err(|e| e.to_string())?;
    let mut prepared = Prepared {
        root,
        relative,
        width,
        height,
        file: Some(output),
        request: status.request,
    };
    let mut input = fs::File::open(file)
        .map_err(|e| e.to_string())?
        .take(maximum + 1);
    let mut bytes = [0u8; 64 * 1024];
    let mut copied = 0u64;
    while !cancelled() {
        let count = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        copied += count as u64;
        if copied > maximum {
            return Err("image exceeds source byte budget".into());
        }
        prepared
            .file
            .as_mut()
            .ok_or("image output missing")?
            .file()
            .write_all(&bytes[..count])
            .map_err(|e| e.to_string())?;
    }
    if cancelled() {
        return Ok(None);
    }
    let output = prepared.file.as_mut().ok_or("image output missing")?;
    output.file().flush().map_err(|e| e.to_string())?;
    output.close();
    status.enter(3);
    let proxy =
        motion_host::ops::images::prepare_image(prepared.root.clone(), prepared.relative.clone())
            .map_err(|e| e.to_string())?;
    if proxy["width"] != info["width"] || proxy["height"] != info["height"] {
        return Err("image changed during import".into());
    }
    if cancelled() {
        return Ok(None);
    }
    Ok(Some(prepared))
}

#[cfg(test)]
mod tests;
