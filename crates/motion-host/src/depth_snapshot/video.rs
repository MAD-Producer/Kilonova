//! Source-checked temporal scalar ownership. RGB is decoded one sample at a
//! time from the frozen original, never retained for the entire movie.
use super::*;
use crate::depth::{video::VideoInput, VideoKey};
use motion_model::depth::{VideoDepthSample, VideoDepthTrack};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub(crate) struct VideoEntry {
    pub object: u64,
    pub asset: motion_core::VideoAsset,
    pub key: Key,
    pub input: PreparedDepthInput,
    pub sample: VideoDepthSample,
    pub estimate: Arc<Estimate>,
    pub request: u64,
    pub decoded_budget: u64,
}

impl DepthSnapshot {
    pub(super) fn validate_videos(&self, static_objects: &HashSet<u64>) -> Result<()> {
        let mut tracks: HashMap<u64, (motion_core::VideoAsset, VideoDepthTrack)> = HashMap::new();
        for entry in &self.videos {
            entry.asset.validate()?;
            entry.sample.validate()?;
            if static_objects.contains(&entry.object)
                || entry.asset.path.len() > 1024
                || !Path::new(&entry.asset.path)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                || entry.asset.id != entry.key.source.asset
                || [entry.asset.display_width, entry.asset.display_height]
                    != entry.key.geometry.source_size
                || entry.estimate.provider.is_empty()
                || entry.estimate.provider.len() > 256
                || entry.estimate.provider.chars().any(char::is_control)
            {
                return Err("invalid bounded frozen video depth source metadata".into());
            }
            let descriptor = entry.estimate.resource.descriptor();
            if entry.key.source != descriptor.source
                || entry.key.geometry != descriptor.geometry
                || entry.key.model != descriptor.model
            {
                return Err("frozen video depth resource differs from original request".into());
            }
            entry
                .sample
                .calibration()
                .rebind(&entry.estimate.resource, entry.input)?;
            let (asset, track) = tracks
                .entry(entry.object)
                .or_insert_with(|| (entry.asset.clone(), VideoDepthTrack::default()));
            if asset != &entry.asset
                || track
                    .samples()
                    .iter()
                    .any(|s| s.start_us() == entry.sample.start_us())
            {
                return Err("duplicate or inconsistent frozen video depth sample".into());
            }
            track.upsert(entry.sample.clone())?;
            track.validate_asset(asset)?;
        }
        Ok(())
    }

    pub(super) fn validate_video_project(
        &self,
        project: &motion_core::Project,
        composition: &str,
        layer: &motion_core::Layer,
    ) -> Result<()> {
        let entries: Vec<_> = self
            .videos
            .iter()
            .filter(|e| e.object == layer.id)
            .collect();
        let track = layer
            .video_depth_calibration
            .as_ref()
            .ok_or("missing frozen video depth track")?;
        if composition != self.composition
            || entries.len() != track.samples().len()
            || entries.is_empty()
        {
            return Err("frozen video depth project sample mapping changed".into());
        }
        for entry in entries {
            if !matches!(&layer.content, motion_core::Content::Video { video } if video.asset == entry.asset.id)
                || !project.video_assets.contains(&entry.asset)
                || !track.samples().contains(&entry.sample)
            {
                return Err(
                    "frozen video depth project source or calibration mapping changed".into(),
                );
            }
        }
        Ok(())
    }
}

impl VideoEntry {
    pub(super) fn decode(
        &self,
        root: &Path,
        platform: Arc<dyn crate::Platform>,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<motion_render::DepthImage> {
        check()?;
        let began = std::time::Instant::now();
        log::info!("start frozen_depth operation=verify_video source_request={} object={} source_id={} pts_us={} phase=decode_frozen_rgb path={:?}",
            self.request, self.object, self.asset.id, self.sample.start_us(), self.asset.path);
        log::logger().flush();
        // Preserve the original export/control failure across the depth/media
        // callback boundary. Wrapping a cancellation in source context changes
        // the exact protocol reason used by the engine and frame exporter.
        let primary = std::cell::RefCell::new(None);
        let checked = || match check() {
            Ok(()) => Ok(()),
            Err(cause) => {
                if primary.borrow().is_none() {
                    *primary.borrow_mut() = Some(cause);
                }
                Err("frozen video source check interrupted".into())
            }
        };
        let decoded = VideoInput {
            key: VideoKey {
                asset: self.asset.id,
                object: self.object,
                revision: self.key.source.revision,
                target_us: self.sample.start_us(),
                geometry: self.key.geometry.clone(),
                model: self.key.model.clone(),
            },
            root: root.into(),
            asset: self.asset.clone(),
            platform,
            budget: self.decoded_budget,
        }
        .decode_checked(&Cancellation::default(), &checked);
        if let Some(cause) = primary.into_inner() {
            return Err(cause);
        }
        let input = decoded.map_err(|cause| crate::Error::with_context(
            format!("frozen depth source_request={} object={} source_id={} pts_us={} phase=decode_frozen_rgb",
                self.request, self.object, self.asset.id, self.sample.start_us()), cause))?;
        check()?;
        if input.key() != &self.key
            || input.video_end_us() != Some(self.sample.end_us())
            || input.fingerprint(&Cancellation::default())? != self.input
        {
            return Err(format!("frozen depth source_request={} object={} source_id={} pts_us={} phase=verify_frozen_rgb: prepared RGB fingerprint or indexed interval changed",
                self.request, self.object, self.asset.id, self.sample.start_us()).into());
        }
        let image = input
            .into_image()
            .ok_or("frozen video depth has no owned decoded image")?;
        check()?;
        log::debug!("done frozen_depth operation=verify_video source_request={} object={} pts_us={} elapsed_ms={}",
            self.request, self.object, self.sample.start_us(), began.elapsed().as_millis());
        Ok(image)
    }
}
