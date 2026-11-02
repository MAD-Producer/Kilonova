//! Explicit new plate + complete new solves, verified outside Session guards.
use crate::{
    session,
    tracking::import::{Error as ImportError, Prepared},
    Error, Result,
};
use motion_core::{
    media_assets::{AssetKind, AssetRef},
    tracking_assets::replacement::{BindingUpdate, Replacement},
    Command, EditSink, Engine, Project, TrackingAsset,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Instant,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolveReplacement {
    pub asset: u64,
    pub path: PathBuf,
}

/// Remove bindings only in the temporary verification view, never in author
/// state. Each new original is admitted against the candidate plate's metadata.
fn verification_view(
    project: &Project,
    source: AssetRef,
    video: &motion_core::VideoAsset,
    ids: &HashSet<u64>,
) -> Result<Project> {
    let mut candidate = project.clone();
    candidate.tracking_assets.retain(|a| !ids.contains(&a.id));
    let selected = candidate
        .video_assets
        .iter_mut()
        .find(|a| a.id == source.id)
        .ok_or("tracked replacement source is missing")?;
    let mut next = video.clone();
    next.id = source.id;
    next.path = selected.path.clone();
    next.audio_asset = selected.audio_asset;
    *selected = next;
    let active = candidate.composition_id.clone();
    for id in candidate.composition_ids() {
        candidate.activate_composition(&id)?;
        if candidate
            .camera
            .tracking
            .as_ref()
            .is_some_and(|b| ids.contains(&b.asset))
        {
            candidate.camera.tracking = None;
        }
        for layer in &mut candidate.layers {
            if layer
                .tracking_point
                .as_ref()
                .is_some_and(|b| ids.contains(&b.asset))
            {
                layer.tracking_point = None;
            }
            if layer
                .tracking_plane
                .as_ref()
                .is_some_and(|b| ids.contains(&b.asset))
            {
                layer.tracking_plane = None;
            }
            if let motion_core::Content::Camera { camera } = &mut layer.content {
                if camera
                    .tracking
                    .as_ref()
                    .is_some_and(|b| ids.contains(&b.asset))
                {
                    camera.tracking = None;
                }
            }
        }
    }
    candidate.activate_composition(&active)?;
    candidate.validate()?;
    Ok(candidate)
}

/// The ordinary media publisher still owns source/audio/cache publication and
/// its candidate identity check. This sink replaces its single model edit with
/// the joint edit; it never separately commits a new plate or a new solve.
struct PairSink<'a> {
    engine: &'a mut Engine,
    originals: &'a mut [Prepared],
    tracking: Vec<TrackingAsset>,
    bindings: Vec<BindingUpdate>,
    acknowledgement: Option<motion_core::Error>,
}
impl EditSink for PairSink<'_> {
    fn project(&self) -> &Project {
        self.engine.project()
    }
    fn apply_batch_saved(
        &mut self,
        commands: Vec<Command>,
        root: &Path,
    ) -> motion_core::Result<Vec<motion_core::EditResult>> {
        let mut commands = commands.into_iter();
        let Some(Command::MediaAsset {
            action: motion_core::media_assets::AssetAction::ReplaceVideo { video, audio },
        }) = commands.next()
        else {
            return Err(motion_core::Error::Invalid(
                "paired publication requires a video replacement".into(),
            ));
        };
        if commands.next().is_some() {
            return Err(motion_core::Error::Invalid(
                "unexpected extra paired media commands".into(),
            ));
        }
        let replacement = Replacement {
            video,
            audio,
            tracking: self.tracking.clone(),
            bindings: self.bindings.clone(),
        };
        let originals = &mut self.originals;
        let acknowledgement = &mut self.acknowledgement;
        self.engine.apply_batch_with_prepare(
            vec![Command::ReplaceTrackedVideo {
                replacement: Box::new(replacement),
            }],
            |candidate| {
                for original in originals.iter_mut() {
                    original.retain_original(true);
                }
                match motion_core::storage::save(root, candidate) {
                    Ok(()) => Ok(()),
                    Err(
                        error @ motion_core::Error::Storage {
                            published: true, ..
                        },
                    ) => {
                        // Keep model/history and all newly referenced files aligned
                        // with published bytes. Report the durability error after
                        // the media publisher has retained its successful resources.
                        *acknowledgement = Some(error);
                        Ok(())
                    }
                    Err(motion_core::Error::Io(source)) => {
                        for original in originals.iter_mut() {
                            original.retain_original(false);
                        }
                        Err(motion_core::Error::Storage {
                            phase: "save_tracked_video_project",
                            path: root.join("project.json"),
                            published: false,
                            source,
                        })
                    }
                    Err(error) => {
                        for original in originals.iter_mut() {
                            original.retain_original(false);
                        }
                        Err(error)
                    }
                }
            },
        )
    }
}

pub(super) fn finish(
    id: i64,
    request: &str,
    source: AssetRef,
    revision: u64,
    generation: u64,
    solves: Vec<SolveReplacement>,
    bindings: Vec<BindingUpdate>,
) -> Result<Value> {
    let began = Instant::now();
    // Own preparations outside the Session guard, including on unwind.
    let mut originals = Vec::new();
    let mut sample_failure = None;
    log::info!(
        "tracked video replacement start request={} session={id} source={:?}:{} phase=admission",
        request.chars().take(128).collect::<String>(),
        source.kind,
        source.id
    );
    let result = (|| {
        if request.is_empty()
            || request.len() > 128
            || source.kind != AssetKind::Video
            || source.id == 0
        {
            return Err("tracked replacement requires a bounded request and a video source".into());
        }
        if solves.is_empty()
            || solves.len() > motion_core::tracking_assets::MAX_TRACKING_ASSETS
            || bindings.len() > motion_core::tracking_assets::replacement::MAX_BINDING_UPDATES
        {
            return Err("tracked replacement exceeds solve/binding limits".into());
        }
        let (root, expected, platform, video) = session::with_session(id, |s| {
            if s.engine.revision() != revision || s.document_generation != generation {
                return Err(ImportError::Superseded.into());
            }
            if s.engine.gesture_active() || s.editor.as_ref().is_some_and(|e| e.gesture) {
                return Err("finish the active gesture before tracked replacement".into());
            }
            let video = s.video_jobs.replacement_candidate(request)?;
            Ok((
                s.root.clone(),
                s.engine.snapshot(),
                s.platform().clone(),
                video,
            ))
        })?;
        let ids: HashSet<_> = expected
            .tracking_assets
            .iter()
            .filter(|a| a.source == source)
            .map(|a| a.id)
            .collect();
        let requested: HashSet<_> = solves.iter().map(|s| s.asset).collect();
        if ids.is_empty() || ids != requested || requested.len() != solves.len() {
            return Err("tracked replacement must supply every attached solve exactly once".into());
        }
        let mut bytes = 0_u64;
        for solve in &solves {
            if solve.path.as_os_str().is_empty()
                || solve.path.as_os_str().to_string_lossy().len() > 4096
            {
                return Err("tracked replacement original path exceeds limits".into());
            }
            let size = std::fs::metadata(&solve.path)?.len();
            bytes = bytes
                .checked_add(size)
                .ok_or("tracked replacement original size overflow")?;
            if size == 0 || bytes > motion_core::tracking_assets::MAX_TRACKING_ASSET_BYTES {
                return Err(
                    "tracked replacement original documents exceed the aggregate byte budget"
                        .into(),
                );
            }
        }
        let mut view = verification_view(&expected, source, video.asset(), &ids)?;
        let mut tracking = Vec::new();
        for solve in &solves {
            log::info!("tracked video replacement start request={request} session={id} source=Video:{} tracking_asset={} phase=verify_original",source.id,solve.asset);
            let original = Prepared::read_from(
                &root,
                &view,
                revision,
                &solve.path,
                source,
                Some(platform.as_ref()),
                Some(&video.source_path()),
                || false,
            )?;
            let mut descriptor = original.descriptor().clone();
            view.tracking_assets.push(descriptor.clone());
            descriptor.id = solve.asset;
            tracking.push(descriptor);
            originals.push(original);
        }
        session::with_session(id, |s| {
            if s.engine.revision() != revision
                || s.document_generation != generation
                || s.root != root
                || s.engine.project() != &expected
            {
                return Err(ImportError::Superseded.into());
            }
            if s.engine.gesture_active() || s.editor.as_ref().is_some_and(|e| e.gesture) {
                return Err("finish the active gesture before tracked replacement".into());
            }
            let mut sink = PairSink {
                engine: &mut s.engine,
                originals: &mut originals,
                tracking,
                bindings,
                acknowledgement: None,
            };
            let task = s
                .video_jobs
                .replace_candidate(request, &mut sink, source.id, &video)?;
            let acknowledgement = sink.acknowledgement.take();
            drop(sink);
            if task.state != "succeeded" {
                return Err(task
                    .error
                    .unwrap_or_else(|| "paired replacement did not publish".into())
                    .into());
            }
            s.audio_mixer = None;
            s.video_frames.clear();
            if let Err(cause) = s.sample() {
                let cause = cause.to_string();
                s.last_error = Some(cause.clone());
                sample_failure = Some(cause);
            }
            if let Some(error) = acknowledgement {
                return Err(error.into());
            }
            Ok(
                json!({"task":task,"target":source,"tracking_replaced":solves.iter().map(|s|s.asset).collect::<Vec<_>>(),"state":s.snapshot()?}),
            )
        })
    })();
    let mut result = result;
    for original in &mut originals {
        if let Err(secondary) = original.cleanup_uncommitted() {
            result = Err(match result {
                Ok(_) => secondary.into(),
                Err(primary) => Error::Cleanup {
                    primary: Box::new(primary),
                    secondary: Box::new(secondary.into()),
                },
            });
        }
    }
    if let Some(cause) = sample_failure {
        log::error!("tracked video replacement committed but sampling failed request={request} source=Video:{} phase=sample cause={}",source.id,cause.chars().take(1536).collect::<String>());
    }
    match &result {
        Ok(_) => log::debug!(
            "tracked video replacement done request={request} source=Video:{} elapsed_ms={}",
            source.id,
            began.elapsed().as_millis()
        ),
        Err(Error::TrackingImport(ImportError::Superseded)) => log::debug!(
            "tracked video replacement superseded request={} elapsed_ms={}",
            request.chars().take(128).collect::<String>(),
            began.elapsed().as_millis()
        ),
        Err(cause) => log::error!(
            "tracked video replacement failed request={} source=Video:{} elapsed_ms={} cause={}",
            request.chars().take(128).collect::<String>(),
            source.id,
            began.elapsed().as_millis(),
            cause.to_string().chars().take(1536).collect::<String>()
        ),
    }
    result
}
