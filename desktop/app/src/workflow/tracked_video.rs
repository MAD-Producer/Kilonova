//! Bounded editor workflow for preparing and committing a tracked plate.
//! Preparation owns only a media task; the host commits all author state once.
use motion_core::{
    media_assets::{AssetKind, AssetRef},
    tracking_assets::replacement::BindingUpdate,
};
use motion_host::{ops::media, session, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Solve {
    pub asset: u64,
    pub path: String,
}

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Request {
    Prepare {
        request_id: String,
        target: AssetRef,
        path: String,
        expected_revision: u64,
        expected_document_generation: u64,
        with_audio: bool,
        track: Option<u32>,
        audio_track: Option<u32>,
    },
    Status {
        request_id: String,
    },
    Finish {
        request_id: String,
        target: AssetRef,
        expected_revision: u64,
        expected_document_generation: u64,
        solves: Vec<Solve>,
        bindings: Vec<BindingUpdate>,
    },
    Cancel {
        request_id: String,
    },
    Release {
        request_id: String,
    },
}

impl Request {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => "prepare_tracked_video_replacement",
            Self::Status { .. } => "tracked_video_replacement_status",
            Self::Finish { .. } => "finish_tracked_video_replacement",
            Self::Cancel { .. } => "cancel_tracked_video_replacement",
            Self::Release { .. } => "release_tracked_video_replacement",
        }
    }

    pub fn read_only(&self) -> bool {
        matches!(self, Self::Status { .. })
    }

    pub fn validate(&self) -> std::result::Result<(), String> {
        let request = match self {
            Self::Prepare {
                request_id,
                target,
                path,
                ..
            } => {
                video_target(*target)?;
                bounded_path(path)?;
                request_id
            }
            Self::Finish {
                request_id,
                target,
                solves,
                bindings,
                ..
            } => {
                video_target(*target)?;
                if solves.is_empty()
                    || solves.len() > motion_core::tracking_assets::MAX_TRACKING_ASSETS
                    || bindings.len()
                        > motion_core::tracking_assets::replacement::MAX_BINDING_UPDATES
                {
                    return Err("tracked replacement exceeds solve/binding limits".into());
                }
                let mut ids = std::collections::HashSet::new();
                for solve in solves {
                    if solve.asset == 0 || !ids.insert(solve.asset) {
                        return Err("supply each positive solve ID exactly once".into());
                    }
                    bounded_path(&solve.path)?;
                }
                request_id
            }
            Self::Status { request_id }
            | Self::Cancel { request_id }
            | Self::Release { request_id } => request_id,
        };
        if request.is_empty() || request.len() > 128 || request.chars().any(char::is_control) {
            return Err(
                "requestId must be a bounded nonempty string without control characters".into(),
            );
        }
        Ok(())
    }
}

fn bounded_path(path: &str) -> std::result::Result<(), String> {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return Err("path must be a bounded nonempty string without NUL".into());
    }
    Ok(())
}

fn video_target(target: AssetRef) -> std::result::Result<(), String> {
    if target.kind != AssetKind::Video || target.id == 0 {
        return Err("target must identify a positive video asset".into());
    }
    Ok(())
}

/// Called only by the serial engine worker. Never keep a session guard across
/// source IO, media job startup or host solve verification.
pub fn request(id: i64, request: Request) -> Result<Value> {
    request.validate()?;
    let task_op = match &request {
        Request::Status { .. } => "media_status",
        Request::Cancel { .. } => "cancel_media_import",
        Request::Release { .. } => "release_media_task",
        _ => "",
    };
    let (value, opener) = match request {
        Request::Prepare {
            request_id,
            target,
            path,
            expected_revision,
            expected_document_generation,
            with_audio,
            track,
            audio_track,
        } => {
            session::with_session(id, |s| {
                if s.engine.revision() != expected_revision
                    || s.document_generation != expected_document_generation
                {
                    return Err(motion_host::tracking::import::Error::Superseded.into());
                }
                if s.engine.gesture_active() || s.editor.as_ref().is_some_and(|e| e.gesture) {
                    return Err("finish the active gesture before tracked replacement".into());
                }
                let project = s.engine.project();
                if !project.video_assets.iter().any(|a| a.id == target.id)
                    || !project.tracking_assets.iter().any(|a| a.source == target)
                {
                    return Err("target must be a video with attached tracking solves".into());
                }
                Ok(())
            })?;
            let open = media::file_opener(path.clone().into());
            (
                json!({"op":"import_media","request_id":request_id,"path":path,"kind":"video",
                "at_frame":0,"with_audio":with_audio,"track":track,"audio_track":audio_track}),
                Some(open),
            )
        }
        Request::Finish {
            request_id,
            target,
            expected_revision,
            expected_document_generation,
            solves,
            bindings,
        } => (
            json!({"op":"finish_tracked_video_replacement","request_id":request_id,"target":target,
                "revision":expected_revision,"document_generation":expected_document_generation,"solves":solves,"bindings":bindings}),
            None,
        ),
        Request::Status { request_id }
        | Request::Cancel { request_id }
        | Request::Release { request_id } => {
            session::with_session(id, |s| {
                if !s.video_jobs.contains(&request_id) {
                    return Err("video task missing".into());
                }
                Ok(())
            })?;
            (json!({"op":task_op,"request_id":request_id}), None)
        }
    };
    media::request(id, value, opener)
}
