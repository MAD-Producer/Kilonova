//! Visible-row requests only. No filesystem or codec runs on the panel thread.
use super::*;
use motion_ui::paint::Bitmap;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    root: String,
    generation: u64,
    kind: u8,
    id: u64,
    metadata: String,
}
struct Record {
    used: u64,
    state: Status,
}
enum Status {
    Pending {
        receive: Receiver<Completion>,
        request: u64,
        started: Instant,
        cancel: Arc<AtomicBool>,
    },
    Ready(Arc<Bitmap>),
    Failed,
}
struct Completion {
    result: Result<Option<Arc<Bitmap>>, String>,
    _permit: Permit,
}
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Drop for Status {
    fn drop(&mut self) {
        if let Self::Pending { cancel, .. } = self {
            cancel.store(true, Ordering::Release);
        }
    }
}
#[derive(Default)]
pub(crate) struct Thumbnails {
    pub enabled: bool,
    pub(crate) message: Option<String>,
    document: Option<(String, u64)>,
    records: BTreeMap<Key, Record>,
    permits: Arc<AtomicUsize>,
    tick: u64,
    request: u64,
    #[cfg(test)]
    before: Option<Box<dyn FnOnce() + Send>>,
}
impl Thumbnails {
    pub(crate) fn observe(&mut self, state: &Value) {
        let document = (
            state["root"].as_str().unwrap_or_default().to_owned(),
            state["document_generation"].as_u64().unwrap_or_default(),
        );
        if self.document.as_ref() != Some(&document) {
            self.refresh();
            self.document = Some(document);
        }
    }
    pub(super) fn failed(&self, state: &Value, key: &RowKey) -> bool {
        let (kind, id) = match key {
            RowKey::Item(ProjectItem::Image { id }) => (0, *id),
            RowKey::Item(ProjectItem::Video { id }) => (1, *id),
            _ => return false,
        };
        self.records.iter().any(|(key, record)| {
            key.kind == kind
                && key.id == id
                && state["root"] == key.root
                && state["document_generation"] == key.generation
                && matches!(record.state, Status::Failed)
        })
    }
    pub(crate) fn busy(&self) -> bool {
        self.records
            .values()
            .any(|r| matches!(r.state, Status::Pending { .. }))
    }
    pub(crate) fn refresh(&mut self) {
        self.records.clear();
        self.message = None;
    }
    pub(crate) fn poll(&mut self) -> bool {
        let mut changed = false;
        for record in self.records.values_mut() {
            let Status::Pending {
                receive,
                request,
                started,
                ..
            } = &record.state
            else {
                continue;
            };
            let result = match receive.try_recv() {
                Ok(completion) => completion.result,
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Disconnected) => {
                    let cause=format!("thumbnail worker disconnected request={request} phase=decode elapsed_ms={}",started.elapsed().as_millis());
                    log::error!("{cause}");
                    Err(cause)
                }
            };
            record.state = match result {
                Ok(Some(bitmap)) => Status::Ready(bitmap),
                Ok(None) => Status::Failed, // cancelled request; no document edit
                Err(cause) => {
                    self.message = Some(format!(
                        "thumbnail request={request}: {}",
                        crate::fallback_observation::short(cause)
                    ));
                    Status::Failed
                }
            };
            changed = true;
        }
        changed
    }
    pub(super) fn get(&mut self, state: &Value, key: &RowKey) -> Option<Arc<Bitmap>> {
        if !self.enabled {
            return None;
        }
        let (kind, id, field) = match key {
            RowKey::Item(ProjectItem::Image { id }) => (0, *id, "assets"),
            RowKey::Item(ProjectItem::Video { id }) => (1, *id, "video_assets"),
            _ => return None,
        };
        let root = state["root"].as_str()?.to_owned();
        let generation = state["document_generation"].as_u64()?;
        self.observe(state);
        let asset = state["project"][field]
            .as_array()?
            .iter()
            .find(|a| a["id"].as_u64() == Some(id))?;
        let key = Key {
            root,
            generation,
            kind,
            id,
            metadata: asset.to_string(),
        };
        self.records
            .retain(|old, _| old.kind != kind || old.id != id || old == &key);
        self.tick = self.tick.saturating_add(1);
        if let Some(record) = self.records.get_mut(&key) {
            record.used = self.tick;
            return if let Status::Ready(bitmap) = &record.state {
                Some(bitmap.clone())
            } else {
                None
            };
        }
        // A cancelled task keeps its permit until the worker actually returns.
        if self
            .permits
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .is_err()
        {
            return None;
        }
        let permit = Permit(self.permits.clone());
        let Some(request) = self.request.checked_add(1) else {
            log::error!("project thumbnail identity exhausted");
            return None;
        };
        self.request = request;
        if self.records.len() == motion_ui::paint::MAX_BITMAPS {
            if let Some(old) = self
                .records
                .iter()
                .filter(|(_, r)| !matches!(r.state, Status::Pending { .. }))
                .min_by_key(|(_, r)| r.used)
                .map(|(k, _)| k.clone())
            {
                self.records.remove(&old);
            }
        }
        let (send, receive) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        // Published before spawn/codec entry, including a decoder that never returns.
        self.records.insert(
            key.clone(),
            Record {
                used: self.tick,
                state: Status::Pending {
                    receive,
                    request,
                    started,
                    cancel: cancel.clone(),
                },
            },
        );
        let context = format!(
            "request={request} asset={id} kind={kind} source={}",
            crate::fallback_observation::short(asset["path"].as_str().unwrap_or("<missing>"))
        );
        log::info!("project thumbnail queued {context} phase=queued");
        let source = asset.clone();
        #[cfg(test)]
        let before = self.before.take();
        let thread_context = context.clone();
        if let Err(cause) = std::thread::Builder::new()
            .name("motion-project-thumbnail".into())
            .spawn(move || {
                let mut observation = Observation {
                    context: thread_context,
                    started,
                    done: false,
                };
                log::info!(
                    "project thumbnail start {} phase=decode",
                    observation.context
                );
                #[cfg(test)]
                if let Some(before) = before {
                    before();
                }
                let result = decode(&key.root, kind, &source, &cancel);
                if let Err(cause) = &result {
                    log::error!(
                        "project thumbnail failed {} phase=decode elapsed_ms={} cause={cause}",
                        observation.context,
                        started.elapsed().as_millis()
                    );
                }
                log::debug!(
                    "project thumbnail done {} elapsed_ms={}",
                    observation.context,
                    started.elapsed().as_millis()
                );
                observation.done = true;
                // Receiver cancellation is the defined best-effort completion boundary.
                let _ = send.send(Completion {
                    result,
                    _permit: permit,
                });
            })
        {
            log::error!("project thumbnail spawn {context} cause={cause}");
            // Keep a failed record; ordinary repaint must not restart a failed job.
            if let Some(record) = self
                .records
                .values_mut()
                .find(|r| matches!(r.state,Status::Pending{request:active,..} if active==request))
            {
                record.state = Status::Failed;
            }
        }
        None
    }
}
struct Observation {
    context: String,
    started: Instant,
    done: bool,
}
impl Drop for Observation {
    fn drop(&mut self) {
        if !self.done {
            log::error!(
                "project thumbnail interrupted {} phase=decode elapsed_ms={} unwinding={}",
                self.context,
                self.started.elapsed().as_millis(),
                std::thread::panicking()
            );
        }
    }
}
fn decode(
    root: &str,
    kind: u8,
    value: &Value,
    cancel: &AtomicBool,
) -> Result<Option<Arc<Bitmap>>, String> {
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let root = PathBuf::from(root);
    let source = if kind == 0 {
        let asset=serde_json::from_value::<motion_core::Asset>(value.clone()).map_err(|e| e.to_string())?;
        motion_render::image_resources::Source::new(&root,&asset).map_err(|e|e.to_string())?
    } else {
        let video: motion_core::VideoAsset =
            serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        let path = motion_media::video_cache_path(&root, &video)
            .map_err(|e| e.to_string())?
            .with_extension("png");
        let base = root.canonicalize().map_err(|e| e.to_string())?;
        let resolved = path.canonicalize().map_err(|e| e.to_string())?;
        if !resolved.starts_with(&base) {
            return Err("video thumbnail resolves outside project directory".into());
        }
        let (width, height, _, _) =
            motion_render::image_resources::inspect(&resolved).map_err(|e| e.to_string())?;
        // This is a host-generated cache image, not a registered image asset.
        // Its canonical project containment was checked before inspecting it.
        motion_render::image_resources::Source {
            id: video.id,
            path: resolved,
            root: base,
            width,
            height,
        }
    };
    let decoded = motion_render::image_resources::decode(
        &source,
        motion_render::image_resources::Resolution::Preview(128),
    )
    .map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    Bitmap::new(decoded.width, decoded.height, decoded.rgba)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
