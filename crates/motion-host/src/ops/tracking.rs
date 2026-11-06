//! Explicit original-solve import. All source IO and rollback observation happen
//! outside the Session guard; its owner thread still performs the atomic edit.
use crate::{
    session,
    tracking::import::{Error as ImportError, Prepared},
    Error, Result,
};
use motion_core::media_assets::AssetRef;
use serde_json::{json, Value};
use std::{path::Path, time::Instant};

pub fn import(
    id: i64,
    request: u64,
    document: &Path,
    source: AssetRef,
    revision: u64,
    generation: u64,
) -> Result<Value> {
    let began = Instant::now();
    log::info!("tracking import start site=host.tracking.import request={request} session={id} source={:?}:{} phase=read_original",source.kind,source.id);
    let result = (|| {
        let (root, project, platform) = session::with_session(id, |s| {
            if s.engine.revision() != revision || s.document_generation != generation {
                return Err(ImportError::Superseded.into());
            }
            if s.engine.gesture_active() || s.editor.as_ref().is_some_and(|e| e.gesture) {
                return Err(ImportError::Invalid(
                    "finish the active gesture before tracking import",
                )
                .into());
            }
            Ok((s.root.clone(), s.engine.snapshot(), s.platform().clone()))
        })?;
        let mut prepared = Prepared::read(
            &root,
            &project,
            revision,
            document,
            source,
            Some(platform.as_ref()),
            || false,
        )?;
        let result = session::with_session(id, |s| {
            if s.engine.revision() != revision
                || s.document_generation != generation
                || s.root != root
            {
                return Err(ImportError::Superseded.into());
            }
            if s.editor.as_ref().is_some_and(|e| e.gesture) {
                return Err(ImportError::Invalid(
                    "finish the active gesture before tracking import",
                )
                .into());
            }
            prepared.commit(&mut s.engine)?;
            // The saved authoring edit has committed. Preserve the existing
            // frame-error reporting contract rather than rolling back its file.
            if let Err(error) = s.sample() {
                s.last_error = Some(error.to_string());
            }
            let mut state = s.snapshot()?;
            state["trackingImport"] = json!({"asset":prepared.descriptor().id,"source":source,
                "format":prepared.descriptor().format,
                "tracks":prepared.track_count(),"observations":prepared.observation_count(),
                "originalPreserved":true,"encodedSourceVerified":true,
                "decodedSourceVerified":true,"decodedTimelineVerified":source.kind==motion_core::media_assets::AssetKind::Video,
                "poseApplied":false});
            Ok(state)
        });
        match (result, prepared.cleanup_uncommitted()) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(primary), Ok(())) => Err(primary),
            (Ok(_), Err(error)) => Err(error.into()),
            (Err(primary), Err(secondary)) => Err(Error::Cleanup {
                primary: Box::new(primary),
                secondary: Box::new(secondary.into()),
            }),
        }
    })();
    match &result {
        Ok(_)=>log::debug!("tracking import done request={request} session={id} source={:?}:{} elapsed_ms={}",source.kind,source.id,began.elapsed().as_millis()),
        Err(Error::TrackingImport(ImportError::Cancelled|ImportError::Superseded))=>log::debug!("tracking import superseded request={request} session={id} source={:?}:{} elapsed_ms={}",source.kind,source.id,began.elapsed().as_millis()),
        Err(error)=>log::error!("tracking import failed request={request} session={id} source={:?}:{} elapsed_ms={} cause={}",source.kind,source.id,began.elapsed().as_millis(),error.to_string().chars().take(1536).collect::<String>()),
    }
    result
}
