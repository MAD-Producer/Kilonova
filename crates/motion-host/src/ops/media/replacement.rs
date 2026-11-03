//! Publish only the exact prepared video whose original solve was checked.
use crate::{session::Session, Result};
use motion_core::{
    media_assets::{AssetKind, AssetRef},
    Project,
};
use motion_media::VideoReplacementCandidate;
use serde_json::{json, Value};
use std::{path::PathBuf, sync::Arc, time::Instant};

struct Check {
    root: PathBuf,
    revision: u64,
    generation: u64,
    expected: Project,
    candidate: Project,
    assets: Vec<u64>,
    video: VideoReplacementCandidate,
    platform: Arc<dyn crate::Platform>,
}
impl Check {
    fn capture(
        s: &Session,
        request: &str,
        target: AssetRef,
        revision: u64,
    ) -> Result<Option<Self>> {
        if target.kind != AssetKind::Video || s.engine.revision() != revision {
            return Ok(None);
        }
        let expected = s.engine.project();
        let assets: Vec<_> = expected
            .tracking_assets
            .iter()
            .filter(|a| a.source == target)
            .map(|a| a.id)
            .collect();
        if assets.is_empty()
            || !s.video_jobs.contains(request)
            || s.video_jobs.status(request)?.state != "ready"
        {
            return Ok(None);
        }
        let video = s.video_jobs.replacement_candidate(request)?;
        let mut candidate = expected.clone();
        let selected = candidate
            .video_assets
            .iter_mut()
            .find(|v| v.id == target.id)
            .ok_or("replacement source missing")?;
        let mut replacement = video.asset().clone();
        replacement.id = target.id;
        replacement.audio_asset = selected.audio_asset;
        // Validate the proposed metadata in a regular project, keeping its
        // managed path. The private staging location is a separate IO input.
        replacement.path = selected.path.clone();
        *selected = replacement;
        Ok(Some(Self {
            root: s.root.clone(),
            revision,
            generation: s.document_generation,
            expected: expected.clone(),
            candidate,
            assets,
            video,
            platform: s.platform().clone(),
        }))
    }
    fn matches(&self, s: &Session) -> bool {
        self.revision == s.engine.revision()
            && self.generation == s.document_generation
            && self.expected == *s.engine.project()
    }
    fn verify(&self, request: &str, session: i64) -> Result<()> {
        let began = Instant::now();
        let result = super::tracking::same_streams(&self.expected, &self.candidate, &self.assets)
            .and_then(|()| {
                super::tracking::verify(
                    &self.root,
                    &self.root,
                    &self.candidate,
                    &self.assets,
                    self.platform.as_ref(),
                    request,
                    "media replacement",
                    Some(self.video.relative_source_path()),
                )
            });
        if let Err(cause) = &result {
            log::error!("media replacement tracking failed request={request} session={session} phase=verify_tracking elapsed_ms={} cause={}", began.elapsed().as_millis(), cause.to_string().chars().take(8192).collect::<String>());
        } else {
            log::debug!(
                "media replacement tracking done request={request} session={session} elapsed_ms={}",
                began.elapsed().as_millis()
            );
        }
        result
    }
}

pub(super) fn finish(id: i64, request: &str, target: AssetRef, revision: u64) -> Result<Value> {
    let check = crate::session::with_session(id, |s| Check::capture(s, request, target, revision))?;
    let verified = check.as_ref().map(|check| check.verify(request, id));
    crate::session::with_session(id, |s| {
        if let Some(check) = &check {
            if !check.matches(s) {
                return Ok(
                    json!({"asset_operation_cancelled":true,"asset_operation_reason":"superseded","state":s.snapshot()?}),
                );
            }
            if let Some(Err(cause)) = verified {
                return Err(cause);
            }
        }
        finish_inner(
            s,
            request,
            target,
            revision,
            check.as_ref().map(|c| &c.video),
        )
    })
}

fn finish_inner(
    s: &mut Session,
    request_id: &str,
    target: AssetRef,
    revision: u64,
    candidate: Option<&VideoReplacementCandidate>,
) -> Result<Value> {
    use motion_core::media_assets::{lookup, AssetKind};
    super::relink::observation::record(log::Level::Info, format_args!("media replacement commit request={} asset={} kind={:?} revision={} expected_revision={revision}",
                request_id.chars().take(128).collect::<String>(), target.id, target.kind, s.engine.revision()));
    if revision < s.engine.revision() {
        let task = match target.kind {
            AssetKind::Video if s.video_jobs.contains(&request_id) => {
                json!(s.video_jobs.cancel(&request_id)?)
            }
            AssetKind::Audio if s.audio_jobs.contains(&request_id) => {
                json!(s.audio_jobs.cancel(&request_id)?)
            }
            _ => return Err("replacement task and asset kinds do not match".into()),
        };
        return Ok(
            json!({"asset_operation_cancelled":true,"asset_operation_reason":"superseded","task":task,"state":s.snapshot()?}),
        );
    }
    if s.engine.revision() != revision {
        return Err("project changed during media replacement; original asset retained".into());
    }
    lookup(s.engine.project(), target).ok_or("replacement asset no longer exists")?;
    let prepared = match target.kind {
        AssetKind::Video if s.video_jobs.contains(&request_id) => {
            json!(s.video_jobs.status(&request_id)?)
        }
        AssetKind::Audio if s.audio_jobs.contains(&request_id) => {
            json!(s.audio_jobs.status(&request_id)?)
        }
        _ => return Err("replacement task and asset kinds do not match".into()),
    };
    if prepared["state"] != "ready" {
        return Err(prepared["error"]
            .as_str()
            .unwrap_or("replacement task is not ready; original asset retained")
            .into());
    }
    let task = match target.kind {
        AssetKind::Video if s.video_jobs.contains(&request_id) => {
            json!(if let Some(candidate) = candidate {
                s.video_jobs
                    .replace_candidate(request_id, &mut s.engine, target.id, candidate)?
            } else {
                s.video_jobs.replace(request_id, &mut s.engine, target.id)?
            })
        }
        AssetKind::Audio if s.audio_jobs.contains(&request_id) => {
            json!(s
                .audio_jobs
                .replace(&request_id, &mut s.engine, target.id)?)
        }
        _ => return Err("replacement task and asset kinds do not match".into()),
    };
    if task["state"] != "succeeded" {
        return Err(task["error"]
            .as_str()
            .unwrap_or("replacement did not publish; original asset retained")
            .into());
    }
    s.audio_mixer = None;
    s.video_frames.clear();
    if let Err(error) = s.sample() {
        super::relink::observation::record(log::Level::Error, format_args!("media replacement committed but sampling failed asset={} kind={:?} revision={} phase=sample reason={error}",target.id,target.kind,s.engine.revision()));
    }
    Ok(json!({"task":task,"target":target,"state":s.snapshot()?}))
}
