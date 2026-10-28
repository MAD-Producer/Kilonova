//! Optional session-owned depth results. The live document stamp is read from
//! Session at both sampling and installation, never supplied by a poll caller.
use super::{Result, Session};
use crate::depth::{
    self, image::ImageInput, video::VideoInput, Backend, Estimate, Identity, Jobs, Key,
    SessionStatus, VideoKey,
};
use motion_core::{Asset, Content, Layer, Project, VideoAsset};
use motion_model::depth::{DepthGeometry, DepthModel, DepthSource, DepthTime, PreparedDepthInput};
use std::{path::PathBuf, sync::Arc};
mod retained;

pub(super) struct State {
    jobs: Jobs,
    binding: Option<Binding>,
    installed: Option<(Arc<Estimate>, PreparedDepthInput, Option<u64>)>,
    retained: retained::Results,
    archive: Option<(Binding, Arc<depth::storage::Store>)>,
}
#[derive(Clone)]
struct Binding {
    request: u64,
    key: Identity,
    document: u64,
    revision: u64,
    frame: f64,
    composition: String,
    root: PathBuf,
    source: Source,
    decoded_budget: u64,
}
#[derive(Clone)]
enum Source {
    Image(Asset),
    Video { asset: VideoAsset, object: u64 },
}
fn video_target(project: &Project, layer: &Layer, asset: &VideoAsset, frame: f64) -> Option<u64> {
    let clip = layer.clip(project.frames);
    // Original sources may be inspected while hidden, but empty time intervals
    // remain normal misses. No clamp/repeat of another source frame is allowed.
    if !frame.is_finite()
        || frame < 0.
        || frame >= f64::from(project.frames)
        || frame < f64::from(clip.in_frame)
        || frame >= f64::from(clip.out_frame)
    {
        return None;
    }
    let Content::Video { video } = &layer.content else {
        return None;
    };
    let time = video.source_time_us(layer.local_frame(frame), project.fps);
    let time = u64::try_from(time).ok()?;
    (time >= asset.video_start_us && time < asset.video_end_us).then_some(time)
}
impl Source {
    fn registered(&self, project: &Project, key: &Identity) -> bool {
        match (self, key) {
            (Self::Image(asset), Identity::Sample(_)) => project.assets.iter().any(|a| a == asset),
            (Self::Video { asset, object }, Identity::Video(key)) => {
                key.object == *object && project.video_assets.iter().any(|a| a == asset)
                    && project.layers.iter().any(|layer| layer.id == *object
                        && matches!(&layer.content, Content::Video { video } if video.asset == asset.id))
            }
            _ => false,
        }
    }
    fn current(&self, session: &Session, key: &Identity) -> bool {
        self.current_project(session.engine.project(), session.frame, key)
    }
    fn current_project(&self, project: &Project, frame: f64, key: &Identity) -> bool {
        match (self, key) {
            (Self::Image(asset), Identity::Sample(_)) => project.assets.iter().any(|a| a == asset),
            (Self::Video { asset, object }, Identity::Video(key)) => {
                let Some(layer) = project.layers.iter().find(|layer| layer.id == *object) else {
                    return false;
                };
                let Content::Video { video } = &layer.content else {
                    return false;
                };
                key.object == *object && video.asset == asset.id
                    && project.video_assets.iter().any(|a| a == asset)
                    && video_target(project, layer, asset, frame) == Some(key.target_us)
            }
            _ => false,
        }
    }
}

pub(crate) struct CalibrationSample {
    request: u64,
    estimate: Arc<Estimate>,
    input: PreparedDepthInput,
    image: Option<motion_render::DepthImage>,
    video_end_us: Option<u64>,
    source: Source,
    key: Identity,
    frame: f64,
    composition: String,
}
impl CalibrationSample {
    pub(crate) fn validate_video(&self, project: &Project, composition: &str, object: u64,
        sample: &motion_model::depth::VideoDepthSample) -> Result<()> {
        if !matches!(self.source, Source::Video { .. }) || self.video_end_us != Some(sample.end_us()) {
            return Err(format!("video depth object={object} phase=prepare: actual indexed sample end differs").into());
        }
        self.validate(project, composition, object, sample.calibration())
    }
    pub(crate) fn validate(
        &self,
        project: &Project,
        composition: &str,
        object: u64,
        calibration: &motion_model::depth::SavedDepthCalibration,
    ) -> Result<()> {
        let view = project.composition_view(composition)?;
        let layer = view
            .layers
            .iter()
            .find(|layer| layer.id == object)
            .ok_or_else(|| {
                crate::Error::from(format!(
                    "depth calibration object={object}: layer does not exist"
                ))
            })?;
        let asset = match &layer.content {
            Content::Image { asset } => *asset,
            Content::Video { video } => video.asset,
            _ => {
                return Err(format!(
                    "depth calibration object={object}: original image or video source required"
                )
                .into())
            }
        };
        if composition != self.composition
            || !self.source.current_project(project, self.frame, &self.key)
            || asset != calibration.asset()
            || matches!(self.source, Source::Video { object: source_object, .. } if source_object != object)
        {
            return Err(format!("depth calibration object={object} phase=prepare: current original source or composition changed").into());
        }
        calibration.rebind(&self.estimate.resource, self.input)?;
        Ok(())
    }
}
impl Binding {
    fn current(&self, session: &Session) -> bool {
        if self.frame == session.frame && self.current_document(session) { return true; }
        // Only an authored indexed track can prove that a changed timeline
        // coordinate still denotes the same source sample while work is pending.
        // Decoder staging independently verifies the actual PTS/end afterward.
        if !self.document_matches(session) || !self.source.current_project(session.engine.project(), self.frame, &self.key) {
            return false;
        }
        let (Source::Video { asset, object }, Identity::Video(key)) = (&self.source, &self.key) else { return false; };
        let project = session.engine.project();
        let Some(layer) = project.layers.iter().find(|layer| layer.id == *object) else { return false; };
        let Some(track) = &layer.video_depth_calibration else { return false; };
        let Some(target) = video_target(project, layer, asset, session.frame) else { return false; };
        match (track.at(key.target_us), track.at(target)) {
            (Some(old), Some(current)) => old == current && current.calibration().geometry() == &key.geometry
                && current.calibration().model() == &key.model,
            _ => false,
        }
    }
    fn document_matches(&self, session: &Session) -> bool {
        session.frame.is_finite()
            && self.document == session.document_generation
            && self.revision == session.engine.revision()
            && self.composition == session.engine.project().composition_id
            && self.root == session.root
            && self.decoded_budget == session.effects.memory_policy().decoded_image_budget_bytes
    }
    fn current_document(&self, session: &Session) -> bool {
        self.document_matches(session) && self.source.current(session, &self.key)
    }
    fn video_track(&self, session: &Session) -> bool {
        let Source::Video { object, .. } = &self.source else { return false; };
        session.engine.project().layers.iter().any(|layer| layer.id == *object && layer.video_depth_calibration.is_some())
    }
    fn cache_current(&self, session: &Session, owns_original: bool, interval: Option<(u64, u64)>) -> bool {
        if self.video_track(session) {
            owns_original && interval.is_some() && self.document_matches(session)
                && self.source.registered(session.engine.project(), &self.key)
        } else { self.retained_current(session, owns_original) }
    }
    fn ready(&self, session: &Session, owns_original: bool, interval: Option<(u64, u64)>) -> bool {
        if !self.video_track(session) { return self.retained_current(session, owns_original); }
        if !self.cache_current(session, owns_original, interval) { return false; }
        let Source::Video { asset, object } = &self.source else { return false; };
        let Some(layer) = session.engine.project().layers.iter().find(|layer| layer.id == *object) else { return false; };
        let Some(target) = video_target(session.engine.project(), layer, asset, session.frame) else { return false; };
        interval.is_some_and(|(start, end)| target >= start && target < end)
    }
    fn retained_current(&self, session: &Session, owns_original: bool) -> bool {
        self.current_document(session)
            && (self.frame == session.frame
                || (owns_original && matches!(self.source, Source::Image(_))))
    }
}
impl Session {
    pub(crate) fn capture_depth_snapshot(&self) -> Result<crate::depth_snapshot::DepthSnapshot> {
        use crate::depth_snapshot::{DepthSnapshot, Entry, VideoEntry};
        let project = self.engine.project();
        let mut snapshot = DepthSnapshot { composition: project.composition_id.clone(), entries: Vec::new(), videos: Vec::new() };
        for composition in project.reachable_compositions()? {
            let view = project.composition_view(&composition)?;
            for layer in view.layers.iter().filter(|layer| layer.three_d) {
                if let Some(track) = &layer.video_depth_calibration {
                    if composition != project.composition_id {
                        return Err(format!("frozen depth object={} phase=capture: nested depth is not supported", layer.id).into());
                    }
                    for authored in track.samples() {
                        let entry = self.depth.as_ref().and_then(|state| state.retained.entries.iter().find(|entry| {
                            matches!(&entry.binding.source, Source::Video { object, .. } if *object == layer.id)
                                && entry.binding.cache_current(self, entry.image.is_some(), entry.video_interval())
                                && entry.video_interval() == Some((authored.start_us(), authored.end_us()))
                                && authored.calibration().rebind(&entry.estimate.resource, entry.input).is_ok()
                        })).ok_or_else(|| crate::Error::from(format!(
                            "frozen indexed video depth export object={} pts_us={} phase=capture: admitted video sample pending or absent", layer.id, authored.start_us())))?;
                        let Source::Video { asset, .. } = &entry.binding.source else {
                            return Err("frozen depth video admission has no registered source".into());
                        };
                        let descriptor = entry.estimate.resource.descriptor();
                        snapshot.videos.push(VideoEntry {
                            object: layer.id, asset: asset.clone(),
                            key: Key { source: descriptor.source.clone(), geometry: descriptor.geometry.clone(), model: descriptor.model.clone() },
                            input: entry.input, sample: authored.clone(), estimate: entry.estimate.clone(),
                            request: entry.binding.request, decoded_budget: entry.binding.decoded_budget,
                        });
                    }
                    continue;
                }
                let Some(calibration) = &layer.depth_calibration else { continue; };
                if composition != project.composition_id {
                    return Err(format!("frozen depth object={} phase=capture: nested depth is not supported", layer.id).into());
                }
                let sample = self.saved_depth_sample(&composition, layer.id, calibration)?
                    .ok_or_else(|| crate::Error::from(format!("frozen depth object={} phase=capture: current source pending or absent", layer.id)))?;
                let (Source::Image(asset), Identity::Sample(key)) = (sample.source, sample.key) else {
                    return Err(format!("frozen depth object={} phase=capture: video depth is not supported", layer.id).into());
                };
                snapshot.entries.push(Entry {
                    object: layer.id, asset, key, input: sample.input,
                    calibration: calibration.clone(), estimate: sample.estimate,
                    request: sample.request,
                    decoded_budget: self.effects.memory_policy().decoded_image_budget_bytes,
                });
            }
        }
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(crate) fn depth_calibration_sample(&mut self) -> Result<Option<CalibrationSample>> {
        self.poll_depth()?;
        let Some(state) = &self.depth else {
            return Ok(None);
        };
        let Some(binding) = &state.binding else {
            return Ok(None);
        };
        let Some((estimate, input, video_end_us)) = &state.installed else {
            return Ok(None);
        };
        Ok(Some(CalibrationSample {
            request: binding.request,
            estimate: estimate.clone(),
            input: *input,
            image: None,
            video_end_us: *video_end_us,
            source: binding.source.clone(),
            key: binding.key.clone(),
            frame: binding.frame,
            composition: binding.composition.clone(),
        }))
    }

    /// Produce a bounded command from the actual current inference result.
    /// Committing it follows ordinary model history and invalidates the old live
    /// result's revision. A fresh request must establish the next live binding.
    pub fn depth_calibration_command(
        &mut self,
        object: u64,
        anchors: [motion_model::depth::DepthAnchor; 2],
        curve: motion_model::depth::CalibrationCurve,
        extrapolation: motion_model::depth::CalibrationExtrapolation,
        pixels_per_metre: f32,
    ) -> Result<motion_core::Command> {
        self.sample()?;
        let (calibration, _) = self.prepare_depth_calibration(object, anchors, curve, extrapolation, pixels_per_metre)?;
        Ok(motion_core::Command::DepthCalibration { object, calibration: Some(calibration) })
    }

    /// Author one independently verified indexed video sample, preserving the
    /// other source times. Its interval end comes from staging, never the caller.
    pub fn video_depth_calibration_command(
        &mut self, object: u64, anchors: [motion_model::depth::DepthAnchor; 2],
        curve: motion_model::depth::CalibrationCurve,
        extrapolation: motion_model::depth::CalibrationExtrapolation, pixels_per_metre: f32,
    ) -> Result<motion_core::Command> {
        let (calibration, end_us) = self.prepare_depth_calibration(object, anchors, curve, extrapolation, pixels_per_metre)?;
        let end_us = end_us.ok_or("indexed video calibration requires a staged video result")?;
        let sample = motion_model::depth::VideoDepthSample::new(calibration, end_us)?;
        Ok(motion_core::Command::VideoDepthCalibration { object, sample })
    }

    fn prepare_depth_calibration(
        &mut self, object: u64, anchors: [motion_model::depth::DepthAnchor; 2],
        curve: motion_model::depth::CalibrationCurve,
        extrapolation: motion_model::depth::CalibrationExtrapolation, pixels_per_metre: f32,
    ) -> Result<(motion_model::depth::SavedDepthCalibration, Option<u64>)> {
        let sample = self
            .depth_calibration_sample()?
            .ok_or("depth calibration requires an installed current result")?;
        let mut calibration = motion_model::depth::SavedDepthCalibration::new(
            &sample.estimate.resource,
            sample.input,
            anchors,
            curve,
            extrapolation,
            pixels_per_metre,
        )?;
        // Observation views are editing aids, never the source photograph's
        // persisted camera. Reference geometry always uses the authored scene.
        // This scene supplies authored camera/world matrices, not rendered
        // pixels. Uncovered video calibration intervals must not prevent the
        // user from authoring the next interval's reference transform.
        let mut authored = self.engine.project().clone();
        for layer in &mut authored.layers { layer.video_depth_calibration = None; }
        for composition in &mut authored.compositions {
            for layer in &mut composition.layers { layer.video_depth_calibration = None; }
        }
        let mut reference = motion_render::Scene::new(&authored)?;
        reference.set_model_source_catalog(self.scene.model_source_catalog());
        reference.sample(
            &authored,
            self.frame,
            None,
            &motion_core::ExpressionEvaluator,
        )?;
        if let Some(layer) = reference
            .layers
            .iter()
            .find(|layer| layer.id == object && layer.three_d)
        {
            calibration = calibration.with_reference(motion_model::depth::DepthReference {
                camera: reference.native_camera(),
                world: layer.model,
                size: layer.size,
            })?;
        }
        sample.validate(
            self.engine.project(),
            &self.engine.project().composition_id,
            object,
            &calibration,
        )?;
        Ok((calibration, sample.video_end_us))
    }

    pub(super) fn render_depth_sample(&mut self, scene: &mut motion_render::Scene) -> Result<bool> {
        if !scene.has_calibrated_depth() {
            return Ok(true);
        }
        if scene
            .nested
            .iter()
            .any(|node| node.scene.has_calibrated_depth())
        {
            return Err(
                "calibrated depth phase=render: nested depth routing is not implemented".into(),
            );
        }
        if self.depth.is_none() {
            return Err("calibrated depth phase=render: configure a depth backend and request the current source".into());
        }
        self.poll_depth()?;
        let required: Vec<_> = scene
            .calibrated_depth_layers()
            .map(|(id, c)| (id, c.clone()))
            .collect();
        for (object, calibration) in required {
            let source_object = scene.source_object(object);
            let Some(sample) = self.saved_depth_sample(
                &scene.composition_id,
                source_object,
                &calibration,
            )?
            else {
                let indexed_video = self.engine.project().layers.iter()
                    .any(|layer| layer.id == source_object && layer.video_depth_calibration.is_some());
                if indexed_video {
                    let already_requested = self.depth.as_ref().and_then(|state| state.binding.as_ref())
                        .is_some_and(|binding| binding.current(self) && matches!(&binding.key,
                            Identity::Video(key) if key.object == source_object
                                && &key.geometry == calibration.geometry() && &key.model == calibration.model()));
                    if !already_requested {
                        self.request_video_depth(source_object, calibration.geometry().clone(), calibration.model().clone())?;
                    }
                }
                return Ok(false);
            };
            let current = match &sample.key {
                Identity::Sample(key) => key.source.clone(),
                Identity::Video(key) => {
                    // saved_depth_sample already guarded the current document,
                    // exact composition frame, object and source target. The
                    // worker independently checked the actual indexed PTS/end.
                    // Preserve that PTS; never substitute the requested target.
                    let decoded = &sample.estimate.resource.descriptor().source;
                    DepthSource { asset: key.asset, revision: key.revision, pts: decoded.pts }
                }
            };
            match &sample.image {
                Some(image) => scene.attach_calibrated_depth_image(
                    object, &current, &sample.estimate.resource, sample.input, image.clone(),
                )?,
                None => scene.attach_calibrated_depth(
                    object, &current, &sample.estimate.resource, sample.input,
                )?,
            }
        }
        Ok(true)
    }

    /// Match independently current admissions to this layer's saved input,
    /// rather than treating the last worker ticket as every layer's source.
    fn saved_depth_sample(
        &self,
        composition: &str,
        object: u64,
        calibration: &motion_model::depth::SavedDepthCalibration,
    ) -> Result<Option<CalibrationSample>> {
        let Some(state) = &self.depth else {
            return Ok(None);
        };
        let mut rejected = None;
        for entry in &state.retained.entries {
            if !entry.binding.ready(self, entry.image.is_some(), entry.video_interval())
                || entry.estimate.resource.descriptor().source.asset != calibration.asset()
                || matches!(entry.binding.source, Source::Video { object: source_object, .. } if source_object != object)
            {
                continue;
            }
            let sample = entry.sample();
            match sample.validate(self.engine.project(), composition, object, calibration) {
                Ok(()) => return Ok(Some(sample)),
                Err(cause) if rejected.is_none() => rejected = Some(cause),
                Err(_) => {}
            }
        }
        if let Some(cause) = rejected {
            return Err(cause);
        }
        Ok(None)
    }

    /// A saved calibration remains authoring data until both the live session
    /// guard and the actual prepared-input identity admit a new immutable binding.
    pub fn bound_depth_calibration(
        &mut self,
        object: u64,
    ) -> Result<Option<motion_model::depth::RelativeDepthCalibration>> {
        self.poll_depth()?;
        if self
            .depth
            .as_ref()
            .is_none_or(|state| state.retained.entries.is_empty())
        {
            return Ok(None);
        }
        let layer = self
            .engine
            .project()
            .layers
            .iter()
            .find(|layer| layer.id == object)
            .ok_or("calibrated layer does not exist")?;
        let selected = if let Some(track) = &layer.video_depth_calibration {
            let Content::Video { video } = &layer.content else { return Err("video depth source is not video".into()); };
            let asset = self.engine.project().video_assets.iter().find(|asset| asset.id == video.asset)
                .ok_or("video depth asset is missing")?;
            let Some(target) = video_target(self.engine.project(), layer, asset, self.frame) else { return Ok(None); };
            Some(track.at(target).ok_or("video depth calibration has no indexed sample at this source time")?.calibration())
        } else { layer.depth_calibration.as_ref() };
        let Some(calibration) = selected else {
            return Ok(None);
        };
        let Some(sample) =
            self.saved_depth_sample(&self.engine.project().composition_id, object, calibration)?
        else {
            return Ok(None);
        };
        Ok(Some(
            calibration.rebind(&sample.estimate.resource, sample.input)?,
        ))
    }

    /// Opt in with a trusted native backend (for example depth::onnx::Onnx).
    /// The default Session creates no inference worker or runtime. Configure
    /// once per Session; cancel does not replace or restart a failed backend.
    pub fn configure_depth(&mut self, backend: impl Backend) -> Result<()> {
        self.check_thread()?;
        if self.depth.is_some() {
            return Err(
                depth::Error::Invalid("depth backend already configured for this session").into(),
            );
        }
        self.depth = Some(State {
            jobs: Jobs::start(backend)?,
            binding: None,
            installed: None,
            retained: retained::Results::default(),
            archive: None,
        });
        Ok(())
    }

    /// Stage an actual registered static image on the depth worker. File IO,
    /// decode, crop, rotation and inference occur after its published start.
    /// Static images use source PTS 0/1; the composition frame stays in the live
    /// installation guard. Video needs a separate decoder-PTS contract.
    pub fn request_image_depth(
        &mut self,
        asset: u64,
        geometry: DepthGeometry,
        model: DepthModel,
    ) -> Result<u64> {
        self.check_thread()?;
        self.synchronize_depth()?;
        geometry.validate()?;
        model.validate()?;
        if !self.frame.is_finite() {
            return Err(depth::Error::Invalid("nonfinite session frame").into());
        }
        let asset = self
            .engine
            .project()
            .assets
            .iter()
            .find(|a| a.id == asset)
            .ok_or(depth::Error::Invalid(
                "registered depth image asset not found",
            ))?
            .clone();
        if geometry.source_size != [asset.width, asset.height] {
            return Err(depth::Error::Invalid(
                "depth geometry differs from registered image dimensions",
            )
            .into());
        }
        if asset.width > 4096 || asset.height > 4096 {
            return Err(depth::Error::Invalid(
                "depth image source requires explicit resize to at most 4096",
            )
            .into());
        }
        let key = Key {
            source: DepthSource {
                asset: asset.id,
                revision: self.engine.revision(),
                pts: DepthTime {
                    ticks: 0,
                    timescale: 1,
                },
            },
            geometry,
            model,
        };
        let depth = self
            .depth
            .as_mut()
            .ok_or(depth::Error::Invalid("depth backend not configured"))?;
        let request = depth.jobs.request_image(ImageInput {
            key: key.clone(),
            root: self.root.clone(),
            relative: asset.path.clone(),
            budget: self.effects.memory_policy().decoded_image_budget_bytes,
        })?;
        depth.binding = Some(Binding {
            request,
            key: Identity::Sample(key.clone()),
            document: self.document_generation,
            revision: self.engine.revision(),
            frame: self.frame,
            composition: self.engine.project().composition_id.clone(),
            root: self.root.clone(),
            source: Source::Image(asset),
            decoded_budget: self.effects.memory_policy().decoded_image_budget_bytes,
        });
        depth.retained.remove_source(key.source.asset);
        depth.installed = None;
        Ok(request)
    }

    /// Infer the current original video layer sample. Source IO/index/decoder,
    /// CPU SDR conversion and inference run on the depth worker. The returned
    /// resource uses the actual indexed decoder PTS, never composition frame
    /// time or the requested source coordinate. Outside the clip is a normal miss.
    pub fn request_video_depth(
        &mut self,
        object: u64,
        geometry: DepthGeometry,
        model: DepthModel,
    ) -> Result<Option<u64>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        geometry.validate()?;
        model.validate()?;
        if !self.frame.is_finite() {
            return Err(depth::Error::Invalid("nonfinite session frame").into());
        }
        let project = self.engine.project();
        let layer = project
            .layers
            .iter()
            .find(|layer| layer.id == object)
            .ok_or(depth::Error::Invalid("depth video layer not found"))?;
        let Content::Video { video } = &layer.content else {
            return Err(depth::Error::Invalid("depth source layer is not video").into());
        };
        let asset = project
            .video_assets
            .iter()
            .find(|asset| asset.id == video.asset)
            .ok_or(depth::Error::Invalid(
                "registered depth video asset not found",
            ))?
            .clone();
        let Some(target_us) = video_target(project, layer, &asset, self.frame) else {
            self.cancel_depth()?;
            return Ok(None);
        };
        if geometry.source_size != [asset.display_width, asset.display_height] {
            return Err(depth::Error::Invalid(
                "depth geometry differs from oriented video dimensions",
            )
            .into());
        }
        let key = VideoKey {
            object,
            asset: asset.id,
            revision: self.engine.revision(),
            target_us,
            geometry,
            model,
        };
        let timeline = layer.video_depth_calibration.is_some();
        let state = self
            .depth
            .as_mut()
            .ok_or(depth::Error::Invalid("depth backend not configured"))?;
        let binding = Binding {
            request: 0,
            key: Identity::Video(key.clone()),
            document: self.document_generation,
            revision: self.engine.revision(),
            frame: self.frame,
            composition: project.composition_id.clone(),
            root: self.root.clone(),
            source: Source::Video { asset: asset.clone(), object },
            decoded_budget: self.effects.memory_policy().decoded_image_budget_bytes,
        };
        let store = if timeline {
            Some(state.archive.get_or_insert_with(|| (binding.clone(), Arc::new(depth::storage::Store::new()))).1.clone())
        } else { None };
        let request = state.jobs.request_video(VideoInput {
            key: key.clone(),
            root: self.root.clone(),
            asset: asset.clone(),
            platform: self.platform.clone(),
            budget: self.effects.memory_policy().decoded_image_budget_bytes,
        }, store)?;
        state.binding = Some(Binding {
            request,
            key: Identity::Video(key.clone()),
            document: self.document_generation,
            revision: self.engine.revision(),
            frame: self.frame,
            composition: project.composition_id.clone(),
            root: self.root.clone(),
            source: Source::Video { asset, object },
            decoded_budget: self.effects.memory_policy().decoded_image_budget_bytes,
        });
        if timeline {
            // Re-requesting one source interval supersedes only that object's
            // interval. Other completed indexed frames remain cache candidates.
            state.retained.retain(|entry| {
                !matches!(entry.binding.source, Source::Video { object: old, .. } if old == object)
                    || !entry.video_interval().is_some_and(|(start, end)| target_us >= start && target_us < end)
            });
        } else { state.retained.remove_source(key.asset); }
        state.installed = None;
        Ok(Some(request))
    }

    /// Install or reuse only a result for the current independently checked
    /// document/revision/frame/source. A pending/canceled/stale result is None.
    /// The resource retains its declared relative/metric meaning; it is never
    /// implicitly bound as renderer hardware depth.
    pub fn poll_depth(&mut self) -> Result<Option<Arc<Estimate>>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        let Some(state) = &mut self.depth else {
            return Ok(None);
        };
        let Some(binding) = &state.binding else {
            return Ok(None);
        };
        let ready = match &binding.key {
            Identity::Sample(key) => state.jobs.take_prepared_ready(key)?,
            Identity::Video(key) => state.jobs.take_video_prepared_ready(key)?,
        };
        let Some(estimate) = ready else {
            return Ok(state.installed.as_ref().map(|(estimate, _, _)| estimate.clone()));
        };
        let binding = binding.clone();
        // Pinning is an admission cost, never an allocation on every healthy
        // render poll. The worker result owns the incoming immutable payload.
        let timeline = binding.video_track(self);
        let pinned: Vec<_> = if timeline { self.depth.as_ref().map(|state| state.retained.entries.iter()
            .filter(|entry| entry.binding.ready(self, entry.image.is_some(), entry.video_interval()))
            .map(|entry| entry.binding.key.clone()).collect()).unwrap_or_default() } else { Vec::new() };
        let state = self.depth.as_mut().ok_or(depth::Error::Invalid("depth owner disappeared during admission"))?;
        let (estimate, input, image, video_end_us) = estimate.into_owned_parts();
        let estimate = Arc::new(estimate);
        let entry = retained::Entry { binding, estimate: estimate.clone(), input, image, video_end_us };
        if timeline { state.retained.admit_video(entry,
            motion_model::depth::MAX_DEPTH_RESOURCE_BYTES, motion_model::MAX_LAYERS,
            |old| pinned.iter().any(|key| key.same_request(&old.binding.key)))?; }
        else { state.retained.admit(entry,
            motion_model::depth::MAX_DEPTH_RESOURCE_BYTES, motion_model::MAX_LAYERS)?; }
        state.installed = Some((estimate.clone(), input, video_end_us));
        Ok(Some(estimate))
    }

    /// Available only with an installed result for this exact live document.
    /// Reading it performs no source IO, hashing or inference.
    pub fn depth_input_fingerprint(&mut self) -> Result<Option<PreparedDepthInput>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        Ok(self
            .depth
            .as_ref()
            .and_then(|state| state.installed.as_ref())
            .map(|(_, input, _)| *input))
    }

    pub fn cancel_depth(&mut self) -> Result<()> {
        self.check_thread()?;
        if let Some(state) = &mut self.depth {
            // Clear ownership even when cancellation finds a poisoned worker.
            state.binding = None;
            state.installed = None;
            state.retained.clear();
            state.archive = None;
            state.jobs.cancel()?;
        }
        Ok(())
    }

    pub fn depth_status(&mut self) -> Result<Option<SessionStatus>> {
        self.check_thread()?;
        self.synchronize_depth()?;
        self.depth
            .as_ref()
            .map(|state| {
                let stored = state.archive.as_ref().map(|(_, store)| store.stats()).transpose()?.unwrap_or_default();
                Ok(SessionStatus {
                    request: state.binding.as_ref().map(|b| b.request),
                    installed: state.installed.is_some(),
                    counts: state.jobs.counts()?,
                    in_flight: state.jobs.in_flight()?,
                    queue_depth: state.jobs.queue_depth()?,
                    retained_samples: state.retained.entries.len(),
                    retained_bytes: state.retained.bytes,
                    retained_image_bytes: state.retained.image_bytes,
                    retention_rejections: state.retained.rejections,
                    stored_samples: stored.samples,
                    stored_bytes: stored.bytes,
                    storage_rejections: stored.rejections,
                })
            })
            .transpose()
    }

    pub(super) fn synchronize_depth(&mut self) -> Result<()> {
        let Some(mut state) = self.depth.take() else { return Ok(()); };
        if state.archive.as_ref().is_some_and(|(scope, _)| !scope.document_matches(self)) {
            state.archive = None;
        }
        // Completed static images retain the exact original pixels. Their
        // immutable source sample is independent of the composition clock.
        // Pending tickets remain frame-bound except within an explicitly saved
        // indexed video interval. Different source intervals are canceled.
        state.retained.retain(|entry| entry.binding.cache_current(self, entry.image.is_some(), entry.video_interval()));
        let result = if state.binding.as_ref().is_some_and(|binding| !binding.current(self)) {
            state.binding = None;
            state.installed = None;
            state.jobs.cancel().map_err(crate::Error::from)
        } else { Ok(()) };
        // Restore ownership even if worker cancellation reports a poisoned lock.
        self.depth = Some(state);
        result
    }
}
