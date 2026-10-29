//! Library registration reuses the existing import publication and history.
use motion_core::{
    media_assets::{lookup, AssetRef, MediaAsset},
    Command, Content, Layer, LayerTimeline,
};
use motion_host::Session;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum Request {
    Model {
        file: PathBuf,
        folder: Option<u64>,
    },
    PlaceModel {
        asset: u64,
        at_frame: u32,
    },
    PreparedImage {
        prepared: super::library_image::Prepared,
        folder: Option<u64>,
    },
    Image {
        file: PathBuf,
        folder: Option<u64>,
    },
    Media {
        request: String,
        folder: Option<u64>,
    },
    Place {
        target: AssetRef,
        at_frame: u32,
    },
}
impl Request {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Model { .. } => "project_register_model",
            Self::PlaceModel { .. } => "project_place_model",
            Self::PreparedImage { .. } => "project_register_prepared_image",
            Self::Image { .. } => "project_register_image",
            Self::Media { .. } => "project_register_media",
            Self::Place { .. } => "project_place_asset",
        }
    }
    pub fn run(self, session: &mut Session) -> motion_host::Result<Value> {
        let context = match &self {
            Self::Model { file, folder } => format!(
                "{} source={:?} folder={folder:?} phase=prepare", self.name(),
                file.to_string_lossy().chars().take(1024).collect::<String>()
            ),
            Self::PlaceModel { asset, at_frame } => format!(
                "{} asset={asset} frame={at_frame} phase=apply", self.name()
            ),
            Self::PreparedImage { prepared, folder } => format!(
                "{} request={} folder={folder:?} phase=publish",
                self.name(),
                prepared.request()
            ),
            Self::Image { file, folder } => format!(
                "{} source={:?} folder={folder:?} phase=prepare",
                self.name(),
                file.to_string_lossy()
                    .chars()
                    .take(1024)
                    .collect::<String>()
            ),
            Self::Media { request, folder } => format!(
                "{} request={:?} folder={folder:?} phase=publish",
                self.name(),
                request.chars().take(128).collect::<String>()
            ),
            Self::Place { target, at_frame } => format!(
                "{} asset={} kind={:?} frame={at_frame} phase=apply",
                self.name(),
                target.id,
                target.kind
            ),
        };
        log::info!("{context}");
        let result = match self {
            Self::Model { file, folder } => super::model::register(session, &file, folder),
            Self::PlaceModel { asset, at_frame } => super::model::place(session, asset, at_frame),
            Self::PreparedImage { prepared, folder } => prepared.publish(session, folder),
            Self::Image { file, folder } => super::register_image(session, &file, folder),
            Self::Media { request, folder } => finish_media(session, &request, folder),
            Self::Place { target, at_frame } => place(session, target, at_frame),
        };
        result.map_err(|cause| motion_host::Error::with_context(context, cause))
    }
    pub fn cancel(&self, session: &Session) -> motion_host::Result<()> {
        let Self::Media { request, .. } = self else {
            return Ok(());
        };
        let context = format!(
            "library import request={:?} phase=cancel",
            request.chars().take(128).collect::<String>()
        );
        let (primary, cleanup) = if session.video_jobs.contains(request) {
            (
                session.video_jobs.cancel(request).map(|_| ()),
                session.video_jobs.release(request),
            )
        } else if session.audio_jobs.contains(request) {
            (
                session.audio_jobs.cancel(request).map(|_| ()),
                session.audio_jobs.release(request),
            )
        } else {
            // The old session may already have released this job on close.
            return Ok(());
        };
        if let Err(cause) = primary {
            if let Err(cleanup) = cleanup {
                log::error!("library import cancellation cleanup failed request={} phase=release reason={cleanup}",
                    request.chars().take(128).collect::<String>());
            }
            return Err(motion_host::Error::with_context(context, cause));
        }
        cleanup.map_err(|cause| motion_host::Error::with_context(context, cause))
    }
}

pub fn finish_media(
    session: &mut Session,
    request: &str,
    folder: Option<u64>,
) -> motion_host::Result<Value> {
    let status = if session.video_jobs.contains(request) {
        json!(session.video_jobs.status(request)?)
    } else {
        json!(session.audio_jobs.status(request)?)
    };
    if status["state"] != "ready" {
        return Err(status["error"]
            .as_str()
            .unwrap_or("library import is not ready for registration")
            .into());
    }
    let task = if session.video_jobs.contains(request) {
        json!(session
            .video_jobs
            .register(request, &mut session.engine, folder)?)
    } else {
        json!(session
            .audio_jobs
            .register(request, &mut session.engine, folder)?)
    };
    if task["state"] != "succeeded" {
        return Err(task["error"]
            .as_str()
            .unwrap_or("library import did not publish")
            .into());
    }
    let mut state = session.snapshot()?;
    state["assetRegistration"] = json!({
        "cancelled": false,
        "asset": task["edit_result"]["asset"],
        "folder": folder,
        "task": task,
    });
    Ok(state)
}

/// Place an already registered identity; never copy or import its source again.
pub fn place(session: &mut Session, target: AssetRef, at_frame: u32) -> motion_host::Result<Value> {
    let object = super::next_layer_id(&session.snapshot()?)?;
    let project = session.engine.project();
    if at_frame >= project.frames {
        return Err("asset placement outside composition".into());
    }
    let asset = lookup(project, target).ok_or("placement asset no longer exists")?;
    let name = Path::new(asset.path())
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Media");
    let center = [project.width as f32 / 2., project.height as f32 / 2., 0.];
    let (size, position, content, source_frames) = match &asset {
        MediaAsset::Image(asset) => (
            [asset.width as f32, asset.height as f32],
            center,
            Content::Image { asset: asset.id },
            u64::from(project.frames),
        ),
        MediaAsset::Video(asset) => (
            [asset.display_width as f32, asset.display_height as f32],
            center,
            Content::Video {
                video: motion_core::VideoClip::new(asset.id),
            },
            asset
                .duration_us
                .checked_mul(u64::from(project.fps))
                .ok_or("video duration exceeds frame range")?
                .div_ceil(1_000_000),
        ),
        MediaAsset::Audio(asset) => (
            [0.; 2],
            [0.; 3],
            Content::Audio {
                audio: motion_core::AudioClip::new(asset.id),
            },
            asset
                .sample_frames
                .checked_mul(u64::from(project.fps))
                .ok_or("audio duration exceeds frame range")?
                .div_ceil(u64::from(asset.sample_rate)),
        ),
    };
    let mut layer = Layer::solid(object, name, size, position, [1.; 4]);
    layer.size = size;
    layer.content = content;
    if matches!(target.kind, motion_core::media_assets::AssetKind::Video) {
        layer.rotation_order = motion_core::RotationOrder::FixedXyz;
    }
    layer.timeline = Some(LayerTimeline {
        in_frame: at_frame as i32,
        out_frame: (u64::from(at_frame) + source_frames).min(u64::from(project.frames)) as i32,
        offset_frame: at_frame as i32,
    });
    let mut state = session.apply_command_batch(vec![Command::Add { layer }])?;
    state["assetPlacement"] =
        json!({"cancelled":false,"object":object,"target":target,"frame":at_frame});
    Ok(state)
}
