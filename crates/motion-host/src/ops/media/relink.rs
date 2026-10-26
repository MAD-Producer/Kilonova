//! Decode replacement candidates in an isolated workspace, then publish one edit.
//! The draft sink deliberately has no persistence authority: missing sources in
//! the live document are checked once, against the complete replacement set.
use crate::{session::Session, Result};
use motion_core::{
    media_assets::{lookup, AssetAction, AssetKind, AssetRef, MediaAsset},
    Command, Engine, Project,
};
use motion_media::{AudioJobs, VideoJobs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NONCE: AtomicU64 = AtomicU64::new(1);
const MAX_RELINKS: usize = motion_core::MAX_LAYERS * 2;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub target: AssetRef,
    pub path: PathBuf,
}

struct Directory {
    path: PathBuf,
    parent: PathBuf,
    keep: bool,
}
impl Directory {
    fn exact(parent: &Path, name: &std::ffi::OsStr) -> Result<Self> {
        let mut components = Path::new(name).components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err("relink directory name must be a single component".into());
        }
        fs::create_dir_all(parent)?;
        let parent = parent.canonicalize()?;
        let path = parent.join(name);
        fs::create_dir(&path)?; // Never claim or replace somebody else's directory.
        Ok(Self {
            path,
            parent,
            keep: false,
        })
    }
    fn unique(parent: &Path, prefix: &str) -> Result<Self> {
        let name = format!(
            "{prefix}-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        );
        Self::exact(parent, std::ffi::OsStr::new(&name))
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        if self.keep {
            return;
        }
        let result = (|| -> std::io::Result<()> {
            match fs::symlink_metadata(&self.path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(e),
                Ok(meta) if !meta.file_type().is_dir() => {
                    return Err(std::io::Error::other("relink directory changed type"))
                }
                Ok(_) => (),
            }
            let resolved = self.path.canonicalize()?;
            if resolved.parent() != Some(self.parent.as_path()) {
                return Err(std::io::Error::other(
                    "relink cleanup path left its owned parent",
                ));
            }
            fs::remove_dir_all(resolved)
        })();
        if let Err(error) = result {
            log::error!(
                "asset relink cleanup failed path={:?} cause={error}",
                self.path
                    .to_string_lossy()
                    .chars()
                    .take(1024)
                    .collect::<String>()
            );
        }
    }
}

struct Draft {
    engine: Engine,
    commands: Vec<Command>,
}
impl motion_core::EditSink for Draft {
    fn project(&self) -> &Project {
        self.engine.project()
    }
    fn apply_batch_saved(
        &mut self,
        commands: Vec<Command>,
        _root: &Path,
    ) -> motion_core::Result<Vec<motion_core::EditResult>> {
        let results = self.engine.apply_batch(commands.clone())?;
        self.commands.extend(commands);
        Ok(results)
    }
}

pub struct Batch {
    // Drop and join existing workers before removing the workspace they use.
    audio: AudioJobs,
    video: VideoJobs,
    probe: motion_media::ProbeVideo,
    draft: Draft,
    candidates: Vec<Candidate>,
    next: usize,
    source_bytes: u64,
    active: Option<(AssetKind, String, u64)>,
    pub request: String,
    revision: u64,
    generation: u64,
    state: &'static str,
    error: Option<String>,
    began: Instant,
    workspace: Directory,
}
impl Batch {
    pub fn new(
        s: &Session,
        request: String,
        candidates: Vec<Candidate>,
        revision: u64,
    ) -> Result<Self> {
        if request.is_empty()
            || request.len() > 128
            || candidates.is_empty()
            || candidates.len() > MAX_RELINKS
        {
            return Err("asset relink request exceeds its bounded input limits".into());
        }
        if revision != s.engine.revision() {
            return Err("project changed before asset relink".into());
        }
        for (index, candidate) in candidates.iter().enumerate() {
            lookup(s.engine.project(), candidate.target).ok_or("relink asset no longer exists")?;
            if candidates[..index]
                .iter()
                .any(|c| c.target == candidate.target)
            {
                return Err("duplicate relink target".into());
            }
            if candidate.target.kind == AssetKind::Audio
                && s.engine
                    .project()
                    .video_assets
                    .iter()
                    .any(|v| v.audio_asset == Some(candidate.target.id))
            {
                return Err("relink the owning video instead of its linked audio".into());
            }
        }
        let workspace = Directory::unique(&s.root, ".asset-relink")?;
        let decode = crate::session::audio_decoder(s.platform());
        let audio = AudioJobs::with_decoder(
            workspace.path.clone(),
            motion_media::Limits::default(),
            decode.clone(),
        )?;
        let video = VideoJobs::with_audio_decoder(workspace.path.clone(), decode)?;
        let probe = super::probe_fn(s.platform());
        let draft = Draft {
            engine: Engine::new(s.engine.project().clone())?,
            commands: Vec::new(),
        };
        log::info!(
            "asset relink start request={request} revision={revision} count={} phase=stage",
            candidates.len()
        );
        Ok(Self {
            audio,
            video,
            probe,
            draft,
            candidates,
            next: 0,
            source_bytes: 0,
            active: None,
            request,
            revision,
            generation: s.document_generation,
            state: "running",
            error: None,
            began: Instant::now(),
            workspace,
        })
    }
    pub fn snapshot(&self) -> Value {
        json!({"request_id":self.request,"operation":"asset_relink","state":self.state,
            "phase":if self.state=="ready" {"awaiting_commit"}else{self.state},
            "progress":self.next as f64/self.candidates.len() as f64,"error":self.error,
            "prepared":self.next,"count":self.candidates.len()})
    }
    pub fn poll(&mut self) -> Value {
        if self.state == "running" {
            if let Err(cause) = self.advance() {
                let target = self.candidates.get(self.next).map(|c| c.target);
                let source = self.candidates.get(self.next).map(|c| {
                    c.path
                        .to_string_lossy()
                        .chars()
                        .take(1024)
                        .collect::<String>()
                });
                let error=format!("asset relink request={} asset={target:?} source={source:?} phase=stage: {cause}",self.request).chars().take(8192).collect::<String>();
                log::error!("{error} elapsed_ms={}", self.began.elapsed().as_millis());
                self.error = Some(error);
                self.state = "failed";
            }
        }
        self.snapshot()
    }
    fn advance(&mut self) -> Result<()> {
        if let Some((kind, request, target)) = &self.active {
            let status = match kind {
                AssetKind::Audio => json!(self.audio.status(request)?),
                AssetKind::Video => json!(self.video.status(request)?),
                AssetKind::Image => return Err("invalid image media task".into()),
            };
            match status["state"].as_str() {
                Some("running") => return Ok(()),
                Some("ready") => (),
                _ => {
                    return Err(status["error"]
                        .as_str()
                        .unwrap_or("relink preparation stopped")
                        .into())
                }
            }
            let bytes = match kind {
                AssetKind::Audio => status["metadata"]["bytes"].as_u64(),
                AssetKind::Video => status["metadata"]["video"]["bytes"].as_u64(),
                AssetKind::Image => None,
            }
            .ok_or("prepared relink source size missing")?;
            self.source_bytes = self
                .source_bytes
                .checked_add(bytes)
                .filter(|n| *n <= motion_core::storage::MAX_PACKAGE)
                .ok_or("relink source budget exceeded")?;
            let result = match kind {
                AssetKind::Audio => json!(self.audio.replace(request, &mut self.draft, *target)?),
                AssetKind::Video => json!(self.video.replace(request, &mut self.draft, *target)?),
                AssetKind::Image => return Err("invalid image media task".into()),
            };
            if result["state"] != "succeeded" {
                return Err(result["error"]
                    .as_str()
                    .unwrap_or("relink draft was rejected")
                    .into());
            }
            match kind {
                AssetKind::Audio => self.audio.release(request)?,
                AssetKind::Video => self.video.release(request)?,
                AssetKind::Image => (),
            }
            self.active = None;
            self.next += 1;
        }
        let Some(candidate) = self.candidates.get(self.next) else {
            self.state = "ready";
            return Ok(());
        };
        let mut request = self.request.clone();
        while request.len() > 110 {
            request.pop();
        }
        request.push_str(&format!("-{}", self.next));
        log::info!(
            "asset relink prepare request={} asset={} kind={:?} phase=decode index={}",
            self.request,
            candidate.target.id,
            candidate.target.kind,
            self.next
        );
        match candidate.target.kind {
            AssetKind::Image => {
                let info = super::super::images::image_info(&candidate.path.to_string_lossy())?;
                let path = format!(
                    "assets/relink-image-{}-{}.{}",
                    candidate.target.id,
                    self.workspace
                        .path
                        .file_name()
                        .ok_or("relink directory name missing")?
                        .to_string_lossy(),
                    if info["format"] == "Png" {
                        "png"
                    } else {
                        "jpg"
                    }
                );
                fs::create_dir_all(self.workspace.path.join("assets"))?;
                let mut source = fs::File::open(&candidate.path)?
                    .take(motion_render::image_resources::MAX_ENCODED_BYTES + 1);
                let mut dest = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.workspace.path.join(&path))?;
                let copied = std::io::copy(&mut source, &mut dest)?;
                dest.flush()?;
                drop(dest);
                if copied > motion_render::image_resources::MAX_ENCODED_BYTES {
                    return Err("relink image exceeds encoded size limit".into());
                }
                self.source_bytes = self
                    .source_bytes
                    .checked_add(copied)
                    .filter(|n| *n <= motion_core::storage::MAX_PACKAGE)
                    .ok_or("relink source budget exceeded")?;
                let decoded =
                    super::super::images::prepare_image(self.workspace.path.clone(), path.clone())?;
                if decoded["width"] != info["width"] || decoded["height"] != info["height"] {
                    return Err("relink image changed during staging".into());
                }
                let asset = motion_core::Asset {
                    id: candidate.target.id,
                    path,
                    width: decoded["width"].as_u64().ok_or("image width missing")? as u32,
                    height: decoded["height"].as_u64().ok_or("image height missing")? as u32,
                };
                motion_core::EditSink::apply_batch_saved(
                    &mut self.draft,
                    vec![Command::MediaAsset {
                        action: AssetAction::Replace {
                            source: MediaAsset::Image(asset),
                        },
                    }],
                    &self.workspace.path,
                )?;
                self.next += 1;
            }
            AssetKind::Audio => {
                self.audio.start_with_source(
                    super::file_opener(candidate.path.clone()),
                    motion_media::ImportOptions {
                        request_id: request.clone(),
                        at_frame: 0,
                        track: None,
                        name: "Relink".into(),
                    },
                    false,
                )?;
                self.active = Some((AssetKind::Audio, request, candidate.target.id));
            }
            AssetKind::Video => {
                self.video.start(
                    super::file_opener(candidate.path.clone()),
                    motion_media::VideoImportOptions {
                        request_id: request.clone(),
                        at_frame: 0,
                        track: None,
                        name: "Relink".into(),
                        with_audio: true,
                        audio_track: None,
                    },
                    false,
                    self.probe.clone(),
                )?;
                self.active = Some((AssetKind::Video, request, candidate.target.id));
            }
        }
        Ok(())
    }
    pub fn cancel(&mut self) -> Result<Value> {
        if matches!(self.state, "running" | "ready") {
            if let Some((kind, request, _)) = &self.active {
                match kind {
                    AssetKind::Audio => {
                        self.audio.cancel(request)?;
                    }
                    AssetKind::Video => {
                        self.video.cancel(request)?;
                    }
                    AssetKind::Image => (),
                }
            }
            self.state = "cancelled";
        }
        Ok(self.snapshot())
    }
    fn publish(&mut self, root: &Path) -> Result<Vec<Directory>> {
        let mut owned = Vec::new();
        let asset_parent = contained_parent(root, &root.join("assets"))?;
        let assets = Directory::unique(&asset_parent, "relinked")?;
        let prefix = format!(
            "assets/{}/",
            assets
                .path
                .file_name()
                .ok_or("relink directory name missing")?
                .to_string_lossy()
        );
        move_files(&self.workspace.path.join("assets"), &assets.path)?;
        owned.push(assets);
        // Existing cache keys are source stems. Preserve the warmed PCM, waveform,
        // index and thumbnail, and refuse collisions instead of replacing caches.
        let mut caches = Vec::new();
        for command in &self.draft.commands {
            match command {
                Command::MediaAsset {
                    action:
                        AssetAction::Replace {
                            source: MediaAsset::Audio(a),
                        },
                } => caches.push(motion_media::cache_path(&self.workspace.path, a)?),
                Command::MediaAsset {
                    action: AssetAction::ReplaceVideo { video, audio },
                } => {
                    caches.push(motion_media::video_cache_path(&self.workspace.path, video)?);
                    if let Some(a) = audio {
                        caches.push(motion_media::cache_path(&self.workspace.path, a)?);
                    }
                }
                _ => (),
            }
        }
        for cache in caches {
            let source = cache.parent().ok_or("relink cache parent missing")?;
            let relative = source
                .strip_prefix(&self.workspace.path)
                .map_err(|_| "relink cache escaped staging")?;
            let destination = root.join(relative);
            let cache_parent = contained_parent(
                root,
                destination
                    .parent()
                    .ok_or("relink cache destination parent missing")?,
            )?;
            let directory = Directory::exact(
                &cache_parent,
                destination
                    .file_name()
                    .ok_or("relink cache destination name missing")?,
            )?;
            move_files(source, &directory.path)?;
            owned.push(directory);
        }
        for command in &mut self.draft.commands {
            let Command::MediaAsset { action } = command else {
                return Err("relink draft contains an unrelated command".into());
            };
            let rewrite = |path: &mut String| -> Result<()> {
                let name = path
                    .strip_prefix("assets/")
                    .ok_or("unexpected relink source path")?;
                *path = format!("{prefix}{name}");
                Ok(())
            };
            match action {
                AssetAction::Replace { source } => match source {
                    MediaAsset::Image(a) => rewrite(&mut a.path)?,
                    MediaAsset::Audio(a) => rewrite(&mut a.path)?,
                    MediaAsset::Video(_) => {
                        return Err("video replacement needs its linked audio".into())
                    }
                },
                AssetAction::ReplaceVideo { video, audio } => {
                    rewrite(&mut video.path)?;
                    if let Some(a) = audio {
                        rewrite(&mut a.path)?;
                    }
                }
                AssetAction::Remove { .. } => {
                    return Err("relink draft cannot remove assets".into())
                }
            }
        }
        Ok(owned)
    }
}
fn contained_parent(root: &Path, path: &Path) -> Result<PathBuf> {
    fs::create_dir_all(path)?;
    let resolved = path.canonicalize()?;
    if !resolved.starts_with(root.canonicalize()?) {
        return Err("relink publication directory escapes project".into());
    }
    Ok(resolved)
}
fn move_files(source: &Path, destination: &Path) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err("unexpected file type in relink publication".into());
        }
        fs::rename(entry.path(), destination.join(entry.file_name()))?;
    }
    Ok(())
}

pub fn missing(s: &Session) -> Result<Value> {
    let p = s.engine.project();
    let mut assets = Vec::new();
    for source in p
        .assets
        .iter()
        .cloned()
        .map(MediaAsset::Image)
        .chain(
            p.audio_assets
                .iter()
                .filter(|a| !p.video_assets.iter().any(|v| v.audio_asset == Some(a.id)))
                .cloned()
                .map(MediaAsset::Audio),
        )
        .chain(p.video_assets.iter().cloned().map(MediaAsset::Video))
    {
        let absent = match fs::metadata(s.root.join(source.path())) {
            Ok(meta) => !meta.is_file(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => {
                return Err(crate::Error::with_context(
                    format!("inspect relink asset={:?} phase=missing", source.target()),
                    e,
                ))
            }
        };
        if absent {
            assets.push(json!({"target":source.target(),"path":source.path(),"name":motion_core::media_assets::usages(p,source.target()).first().map(|u|u.name.clone())}));
        }
    }
    Ok(
        json!({"assets":assets,"revision":s.engine.revision(),"document_generation":s.document_generation}),
    )
}

pub fn finish(s: &mut Session, request: &str) -> Result<Value> {
    if !s
        .asset_relink
        .as_ref()
        .is_some_and(|b| b.request == request)
    {
        return Err("asset relink task missing".into());
    }
    let mut batch = s.asset_relink.take().ok_or("asset relink task missing")?;
    let result: Result<Value> = (|| {
        if batch.generation != s.document_generation || batch.revision < s.engine.revision() {
            batch.cancel()?;
            return Ok(
                json!({"asset_operation_cancelled":true,"asset_operation_reason":"superseded","state":s.snapshot()?}),
            );
        }
        if batch.revision != s.engine.revision() {
            return Err("invalid relink revision".into());
        }
        if batch.state != "ready" {
            return Err(batch
                .error
                .as_deref()
                .unwrap_or("asset relink is not ready")
                .into());
        }
        let mut owned = batch.publish(&s.root)?;
        let before = s.engine.revision();
        let result = s.apply_command_batch(std::mem::take(&mut batch.draft.commands));
        // A snapshot failure after commit must never delete committed sources.
        if s.engine.revision() != before {
            for directory in &mut owned {
                directory.keep = true;
            }
        }
        let state = result?;
        batch.state = "succeeded";
        s.audio_mixer = None;
        s.video_frames.clear();
        log::debug!(
            "asset relink done request={request} revision={} elapsed_ms={}",
            s.engine.revision(),
            batch.began.elapsed().as_millis()
        );
        Ok(json!({"state":state,"task":batch.snapshot()}))
    })();
    if let Err(cause) = &result {
        log::error!("asset relink failed request={request} revision={} elapsed_ms={} phase=commit cause={cause}",s.engine.revision(),batch.began.elapsed().as_millis());
        batch.state = "failed";
        batch.error = Some(cause.to_string());
    }
    s.asset_relink = Some(batch);
    result
}

#[cfg(test)]
mod tests;
