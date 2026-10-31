//! One surface acquisition for a complete group of independently rendered views.
//! The desktop owner supplies geometry and cameras; this layer never mutates
//! the session's global observer or substitutes a camera on resolution failure.
use super::*;
use crate::render_sampling::{ExposureMedia, ExposureMediaPolicy};
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureScenes},
    multiview::{ViewFramePresenter, ViewFrameTargets, ViewInspection, ViewPlacement},
    preview_memory::PreviewMemoryQuality,
    ExposureFrame, ExposurePoll, RenderError, TextureResourceLedger, ViewCamera,
};
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct View<'a> {
    pub camera: ViewCamera<'a>,
    pub placement: ViewPlacement,
}

#[derive(Clone, Debug, PartialEq)]
enum CameraKey {
    Composition,
    Camera(u64),
    Observer([u32; 16]),
}
#[derive(Clone, Debug, PartialEq)]
struct Key {
    document: u64,
    composition: String,
    revision: u64,
    frame: u64,
    owner: u64,
    surface: u64,
    surface_size: [u32; 2],
    view_revision: u64,
    preview: bool,
    quality: PreviewMemoryQuality,
    sizes: Vec<[u32; 2]>,
    cameras: Vec<CameraKey>,
}

struct Request {
    key: Key,
    scenes: Vec<Scene>,
    document: Option<ExposureDocument>,
    tasks: Vec<ExposureFrame<motion_core::ExpressionEvaluator>>,
    failure: Option<Arc<RenderError>>,
    complete: bool,
}

#[derive(Default)]
pub(crate) struct State {
    targets: ViewFrameTargets,
    presenter: ViewFramePresenter,
    request: Option<Request>,
    published: Option<Key>,
}
impl State {
    pub(crate) fn texture_bytes(&self) -> u64 {
        self.targets.texture_bytes()
    }
    pub(crate) fn collect(&self, ledger: &mut TextureResourceLedger) {
        self.targets.collect_texture_resources(ledger);
        self.presenter.collect_texture_resources(ledger);
        if let Some(request) = &self.request {
            for task in &request.tasks {
                ledger.merge(task.texture_resource_ledger());
            }
        }
    }
    fn poll(
        &mut self,
        renderer: &mut Renderer,
        project: &Project,
        root: &std::path::Path,
        videos: &mut VideoFrames,
        proxies: &mut crate::preview_video::PreviewVideoCache,
        reserved: u64,
    ) -> std::result::Result<bool, RenderError> {
        let request = self
            .request
            .as_mut()
            .ok_or_else(|| RenderError::Invalid("multiview phase=poll: request missing".into()))?;
        if let Some(failure) = &request.failure {
            return Err(RenderError::Shared(failure.clone()));
        }
        if request.complete {
            return Ok(true);
        }
        let result = (|| {
            let mut media = ExposureMedia {
                project: request.document.as_deref().unwrap_or(project),
                root,
                videos,
                proxies,
                policy: ExposureMediaPolicy {
                    image_resolution: request.key.quality.image_resolution(),
                    asynchronous_images: request.key.quality.level == 0,
                    video_edge: request.key.quality.video_edge,
                },
            };
            if request.document.is_some() {
                // The host owns one exact-video generation. Finish the current
                // cell before advancing another shutter clock, or pending reads
                // can supersede each other indefinitely. Completed task outputs
                // remain owned until the complete group can be published.
                let slice = Duration::from_millis(8) / request.tasks.len() as u32;
                for task in &mut request.tasks {
                    if matches!(
                        task.poll(renderer, slice, |renderer, scene| media
                            .prepare(renderer, scene))?,
                        ExposurePoll::Pending(_)
                    ) {
                        return Ok(false);
                    }
                }
            } else {
                // Image/video source dependencies are shared by the root views.
                // Prepare them once, then encode each camera before another
                // upload can replace a required source-time pair.
                let scene = request.scenes.first().ok_or_else(|| {
                    RenderError::Invalid("multiview phase=media: scene missing".into())
                })?;
                if !media.prepare(renderer, scene)? {
                    return Ok(false);
                }
            }
            let updated = self.targets.update_with_reserved(
                renderer,
                &request.key.sizes,
                reserved,
                |renderer, index, target, encoder| {
                    if request.document.is_some() {
                        renderer.resolve_exposure(request.tasks[index].target(), target)?;
                    } else if request.key.preview {
                        renderer.encode_preview(
                            &request.scenes[index],
                            &target.view,
                            target.width,
                            target.height,
                            encoder,
                            None,
                        )?;
                    } else {
                        renderer.encode(
                            &request.scenes[index],
                            &target.view,
                            target.width,
                            target.height,
                            encoder,
                            None,
                        )?;
                    }
                    Ok(true)
                },
            )?;
            if updated {
                request.complete = true;
                self.published = Some(request.key.clone());
                // Converted outputs retain the complete group. Task ownership
                // can retire now; native commands retain their allocation leases.
                request.tasks.clear();
            }
            Ok(updated)
        })();
        result.map_err(|error: RenderError| {
            let cause = Arc::new(error);
            request.failure = Some(cause.clone());
            request.tasks.clear();
            RenderError::Shared(cause)
        })
    }
}

impl Session {
    /// Switching the surface back to the original single-view renderer retires
    /// this group's outputs before that renderer admits another target pair.
    /// There is one GPU group per Session, not an unbounded cache per window.
    pub fn clear_view_group(&mut self) -> Result<()> {
        self.check_thread()?;
        if self
            .graphics
            .as_ref()
            .is_some_and(|g| g.views.request.is_some())
        {
            self.video_frames.cancel_pending_sampling()?;
            if let Some(g) = self.graphics.as_mut() {
                g.views = Default::default();
            }
        }
        Ok(())
    }

    fn views_key(
        &self,
        owner: u64,
        frame: f64,
        views: &[View<'_>],
        surface: Option<[u32; 2]>,
    ) -> Result<Option<Key>> {
        self.check_thread()?;
        validate_views(views)?;
        let Some(g) = &self.graphics else {
            return Ok(None);
        };
        let size = surface.unwrap_or([g.config.width, g.config.height]);
        validate_surface(views, size)?;
        let quality = self
            .preview_memory
            .quality(self.preview.mode == PreviewMode::High);
        let project = self.engine.project();
        let cameras = views
            .iter()
            .map(|view| -> Result<CameraKey> {
                Ok(match view.camera {
                    ViewCamera::Composition => CameraKey::Composition,
                    ViewCamera::Camera(id) => CameraKey::Camera(id),
                    ViewCamera::Observer(observer) => CameraKey::Observer(
                        observer
                            .pose(project.width, project.height)?
                            .view_projection
                            .to_cols_array()
                            .map(f32::to_bits),
                    ),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let sizes = views
            .iter()
            .map(|view| {
                let base = self.preview.render_dimensions(
                    project.width,
                    project.height,
                    view.placement.viewport[2].ceil() as u32,
                    view.placement.viewport[3].ceil() as u32,
                );
                let size = quality.dimensions(base.0, base.1);
                [size.0, size.1]
            })
            .collect();
        Ok(Some(Key {
            document: self.document_generation,
            composition: project.composition_id.clone(),
            revision: self.engine.revision(),
            frame: frame.to_bits(),
            owner,
            surface: self.surface_epoch,
            surface_size: size,
            view_revision: self.view_revision,
            preview: self.preview.mode != PreviewMode::High,
            quality,
            sizes,
            cameras,
        }))
    }

    /// A retained group is usable only by the exact document, owner, cameras,
    /// geometry and quality that produced it. Frozen presentation may ignore
    /// the shared clock; it must never rewrite that clock or the sampled scene.
    pub fn view_group_current(&self, owner: u64, views: &[View<'_>], frozen: bool) -> Result<bool> {
        let Some(mut key) = self.views_key(owner, self.frame, views, None)? else {
            return Ok(false);
        };
        let g = self.graphics.as_ref().ok_or("multiview graphics missing")?;
        let Some(published) = &g.views.published else {
            return Ok(false);
        };
        if frozen {
            key.frame = published.frame;
        }
        Ok(key == *published
            && g.views
                .request
                .as_ref()
                .is_some_and(|request| request.complete && request.failure.is_none()))
    }

    pub fn present_retained_views_with_overlay(
        &mut self,
        owner: u64,
        views: &[View<'_>],
        frozen: bool,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        validate_views(views)?;
        let inspections = &[ViewInspection::default(); 4][..views.len()];
        self.present_retained_views_with_inspection(owner, views, inspections, frozen, overlay)
    }

    pub fn present_retained_views_with_inspection(
        &mut self,
        owner: u64,
        views: &[View<'_>],
        inspections: &[ViewInspection<'_>],
        frozen: bool,
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        self.check_thread()?;
        validate_inspections(views, inspections)?;
        self.observe_preview_pressure()?;
        if !self.view_group_current(owner, views, frozen)? {
            return Ok(false);
        }
        let g = self.graphics.as_mut().ok_or("multiview graphics missing")?;
        let size = [g.config.width, g.config.height];
        let Some(output) = acquire_preview_surface(g)? else {
            return Ok(false);
        };
        let target = output.texture.create_view(&Default::default());
        let mut encoder = g.renderer.allocation_command_encoder_for(
            motion_render::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio retained views"),
            },
        )?;
        encode_group(
            g,
            &self.scene,
            views,
            inspections,
            &mut encoder,
            &target,
            g.config.format,
            size,
        )?;
        overlay(&g.renderer, &mut encoder, &target);
        g.renderer
            .submit_allocated(encoder.finish(), |ledger| g.views.collect(ledger))?;
        g.renderer.check_health()?;
        g.renderer.queue.present(output);
        self.presented += 1;
        Ok(true)
    }

    /// Capture uses the completed group, never an unrelated single-view scratch.
    /// The caller submits the encoder and includes view_group_resource_ledger.
    pub fn encode_view_group_capture(
        &self,
        owner: u64,
        views: &[View<'_>],
        encoder: &mut motion_render::AllocationEncoder,
        output: &wgpu::TextureView,
        surface: [u32; 2],
    ) -> Result<()> {
        validate_views(views)?;
        let inspections = &[ViewInspection::default(); 4][..views.len()];
        self.encode_view_group_capture_with_inspection(
            owner,
            views,
            inspections,
            encoder,
            output,
            surface,
        )
    }

    pub fn encode_view_group_capture_with_inspection(
        &self,
        owner: u64,
        views: &[View<'_>],
        inspections: &[ViewInspection<'_>],
        encoder: &mut motion_render::AllocationEncoder,
        output: &wgpu::TextureView,
        surface: [u32; 2],
    ) -> Result<()> {
        validate_inspections(views, inspections)?;
        let key = self.views_key(owner, self.frame, views, Some(surface))?;
        let g = self.graphics.as_ref().ok_or("multiview graphics missing")?;
        if key.as_ref() != g.views.published.as_ref()
            || !g
                .views
                .request
                .as_ref()
                .is_some_and(|request| request.complete && request.failure.is_none())
        {
            return Err("multiview phase=capture: completed group is no longer current".into());
        }
        let placements = placements(&self.scene, views);
        ViewFramePresenter::default().encode_inspected(
            &g.renderer,
            &g.views.targets,
            encoder,
            output,
            output.texture().format(),
            surface,
            &placements,
            inspections,
            self.scene.background,
        )?;
        Ok(())
    }

    /// Preserve the active cell's working image only when its completed group
    /// belongs to this owner and current document/clock/quality. Native floating
    /// surfaces need not be swapped into Graphics to copy their retained image.
    pub fn capture_view_cell(
        &self,
        owner: u64,
        index: usize,
    ) -> Result<motion_render::ViewerSnapshot> {
        self.check_thread()?;
        let g = self
            .graphics
            .as_ref()
            .ok_or("multiview phase=snapshot: graphics missing")?;
        let key = g
            .views
            .published
            .as_ref()
            .ok_or("multiview phase=snapshot: completed group missing")?;
        if key.document != self.document_generation
            || key.composition != self.engine.project().composition_id
            || key.revision != self.engine.revision()
            || key.frame != self.frame.to_bits()
            || key.owner != owner
            || key.surface != self.surface_epoch
            || key.view_revision != self.view_revision
            || key.preview != (self.preview.mode != PreviewMode::High)
            || key.quality
                != self
                    .preview_memory
                    .quality(self.preview.mode == PreviewMode::High)
            || self.preview_memory_held
            || self.last_error.is_some()
            || !g
                .views
                .request
                .as_ref()
                .is_some_and(|request| request.complete && request.failure.is_none())
        {
            return Err("multiview phase=snapshot: completed group is no longer current".into());
        }
        let source = g
            .views
            .targets
            .working_output(index)
            .ok_or("multiview phase=snapshot: cell is unavailable")?;
        Ok(motion_render::ViewerSnapshot::capture(&g.renderer, source)?)
    }

    pub fn view_group_resource_ledger(&self) -> TextureResourceLedger {
        let mut ledger = TextureResourceLedger::default();
        if let Some(g) = &self.graphics {
            g.views.collect(&mut ledger);
        }
        ledger
    }

    /// Preview eyedropper source: an exact completed cell, before channel/ROI
    /// inspection overlays. A superseded frame is a normal unavailable preview.
    pub fn sample_view_cell(
        &self,
        owner: u64,
        index: usize,
        camera: ViewCamera<'_>,
        point: [f64; 2],
    ) -> Result<Option<[f32; 4]>> {
        self.check_thread()?;
        if !point.iter().all(|v| v.is_finite() && *v >= 0. && *v < 1.) {
            return Err("view color pick is outside the composition".into());
        }
        let project = self.engine.project();
        let camera = match camera {
            ViewCamera::Composition => CameraKey::Composition,
            ViewCamera::Camera(id) => CameraKey::Camera(id),
            ViewCamera::Observer(observer) => CameraKey::Observer(
                observer
                    .pose(project.width, project.height)?
                    .view_projection
                    .to_cols_array()
                    .map(f32::to_bits),
            ),
        };
        let Some(g) = self.graphics.as_ref() else {
            return Ok(None);
        };
        let Some(key) = g.views.published.as_ref() else {
            return Ok(None);
        };
        if key.document != self.document_generation
            || key.composition != project.composition_id
            || key.revision != self.engine.revision()
            || key.frame != self.frame.to_bits()
            || key.owner != owner
            || key.surface != self.surface_epoch
            || key.view_revision != self.view_revision
            || key.preview != (self.preview.mode != PreviewMode::High)
            || key.quality
                != self
                    .preview_memory
                    .quality(self.preview.mode == PreviewMode::High)
            || self.preview_memory_held
            || self.last_error.is_some()
            || key.cameras.get(index) != Some(&camera)
            || !g
                .views
                .request
                .as_ref()
                .is_some_and(|request| request.complete && request.failure.is_none())
        {
            return Ok(None);
        }
        let Some(source) = g.views.targets.output(index) else {
            return Ok(None);
        };
        let pixel = [
            (point[0] * source.width as f64).floor() as u32,
            (point[1] * source.height as f64).floor() as u32,
        ];
        Ok(Some(g.renderer.sample_preview_pixel(source, pixel)?))
    }

    /// Prepare all views before acquiring the owning window's surface once.
    /// `false` is normal media/exposure/surface waiting. Poll/task failures use
    /// the existing worker error channel and are terminal for this content key.
    /// Input/model validation still rejects each explicit invalid request.
    /// Panics propagate to the existing worker hook; they are never swallowed.
    pub fn render_views_with_overlay(
        &mut self,
        owner: u64,
        frame: f64,
        views: &[View<'_>],
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        validate_views(views)?;
        let inspections = &[ViewInspection::default(); 4][..views.len()];
        self.render_views_with_inspection(owner, frame, views, inspections, overlay)
    }

    pub fn render_views_with_inspection(
        &mut self,
        owner: u64,
        frame: f64,
        views: &[View<'_>],
        inspections: &[ViewInspection<'_>],
        overlay: impl FnOnce(&Renderer, &mut wgpu::CommandEncoder, &wgpu::TextureView),
    ) -> Result<bool> {
        validate_inspections(views, inspections)?;
        self.check_thread()?;
        validate_views(views)?;
        if !frame.is_finite() || frame < 0. || frame >= f64::from(self.engine.project().frames) {
            return Err("multiview phase=request: invalid frame".into());
        }
        let Some(g) = &self.graphics else {
            return Ok(false);
        };
        let size = [g.config.width, g.config.height];
        validate_surface(views, size)?;
        self.frame = frame;
        self.observe_preview_pressure()?;
        let quality = self
            .preview_memory
            .quality(self.preview.mode == PreviewMode::High);
        let mut key = self
            .views_key(owner, frame, views, None)?
            .ok_or("multiview graphics missing")?;
        let replace = self.graphics.as_ref().is_some_and(|g| {
            g.views
                .request
                .as_ref()
                .is_none_or(|request| request.key != key)
        });
        if replace
            || self.graphics.as_ref().is_some_and(|g| {
                g.views
                    .request
                    .as_ref()
                    .is_some_and(|request| !request.complete)
            })
        {
            self.render_attempts = self.render_attempts.saturating_add(1);
        }
        if replace {
            self.cancel_pending_exposure()?;
            self.video_frames.cancel_pending_sampling()?;
            self.sample()?;
            if !self.prepare_render_depth(None)? {
                return Ok(false);
            }
            self.synchronize_working_precision()?;
            key.view_revision = self.view_revision;
            let scenes = self
                .scene
                .fork_views(&views.iter().map(|v| v.camera).collect::<Vec<_>>())?;
            let g = self
                .graphics
                .as_mut()
                .ok_or("multiview surface detached during request")?;
            // Replace even if task construction fails: an unchanged key must not
            // repeatedly evaluate expressions or allocate a new failed task.
            g.views.request = Some(Request {
                key: key.clone(),
                scenes,
                document: None,
                tasks: vec![],
                failure: None,
                complete: false,
            });
            if let Some(document) = self.sampling.document().cloned() {
                let request = g
                    .views
                    .request
                    .as_mut()
                    .ok_or("multiview request missing")?;
                request.document = Some(document.clone());
                let initialized = (|| -> std::result::Result<_, RenderError> {
                    let mut tasks = Vec::with_capacity(views.len());
                    for (index, view) in views.iter().enumerate() {
                        let scenes = ExposureScenes::new_for_view(
                            document.clone(),
                            &document.composition_id,
                            frame,
                            key.preview,
                            view.camera,
                            motion_core::ExpressionEvaluator,
                        )?;
                        tasks.push(ExposureFrame::new(
                            &mut g.renderer,
                            scenes,
                            key.sizes[index][0],
                            key.sizes[index][1],
                        )?);
                    }
                    Ok(tasks)
                })();
                match initialized {
                    Ok(tasks) => request.tasks = tasks,
                    Err(error) => {
                        let cause = Arc::new(error);
                        request.failure = Some(cause.clone());
                        return Err(RenderError::Shared(cause).into());
                    }
                }
            }
        }
        let g = self
            .graphics
            .as_mut()
            .ok_or("multiview surface detached during render")?;
        g.renderer.device.poll(wgpu::PollType::Poll)?;
        let complete = g.views.poll(
            &mut g.renderer,
            self.engine.project(),
            &self.root,
            &mut self.video_frames,
            &mut self.preview_video,
            g.scratch.texture_bytes(),
        )?;
        // Pending work may retain the last whole group only in the same native
        // owner, document and surface geometry. Never show another owner's view.
        let retained = g.views.published.as_ref().is_some_and(|old| {
            old.document == key.document
                && old.composition == key.composition
                && old.owner == key.owner
                && old.surface == key.surface
                && old.surface_size == key.surface_size
                && old.cameras.len() == views.len()
        });
        if !complete && !retained {
            let painted = present_chrome(g, None, None, overlay)?;
            self.presented += u64::from(painted);
            return Ok(false);
        }
        let Some(output) = acquire_preview_surface(g)? else {
            return Ok(false);
        };
        let view = output.texture.create_view(&Default::default());
        let mut encoder = g.renderer.allocation_command_encoder_for(
            motion_render::gpu_commands::CommandStage::CompositionAndPresentation,
            &wgpu::CommandEncoderDescriptor {
                label: Some("Motion Studio independent views"),
            },
        )?;
        encode_group(
            g,
            &self.scene,
            views,
            inspections,
            &mut encoder,
            &view,
            g.config.format,
            size,
        )?;
        overlay(&g.renderer, &mut encoder, &view);
        g.renderer
            .submit_allocated(encoder.finish(), |ledger| g.views.collect(ledger))?;
        g.renderer.check_health()?;
        g.renderer.queue.present(output);
        self.presented += 1;
        if complete {
            self.last_presented_frame = Some(frame);
            self.last_presented_revision = self.engine.revision();
            self.last_presented_view_revision = self.view_revision;
            g.actual_preview_quality = Some(quality);
        }
        Ok(complete)
    }
}

fn placements(scene: &Scene, views: &[View<'_>]) -> Vec<ViewPlacement> {
    views
        .iter()
        .map(|view| {
            let mut placement = view.placement;
            if placement.image.is_none() {
                placement.viewport =
                    preview_viewport(placement.viewport, [scene.width, scene.height]);
            }
            placement
        })
        .collect::<Vec<_>>()
}
fn encode_group(
    g: &mut Graphics,
    scene: &Scene,
    views: &[View<'_>],
    inspections: &[ViewInspection<'_>],
    encoder: &mut motion_render::AllocationEncoder,
    output: &wgpu::TextureView,
    format: wgpu::TextureFormat,
    surface: [u32; 2],
) -> Result<()> {
    g.views.presenter.encode_inspected(
        &g.renderer,
        &g.views.targets,
        encoder,
        output,
        format,
        surface,
        &placements(scene, views),
        inspections,
        scene.background,
    )?;
    Ok(())
}

fn validate_inspections(views: &[View<'_>], inspections: &[ViewInspection<'_>]) -> Result<()> {
    validate_views(views)?;
    if views.len() != inspections.len() {
        return Err("multiview phase=inspection: cell counts differ".into());
    }
    for (index, (view, inspection)) in views.iter().zip(inspections).enumerate() {
        inspection.validate(view.placement).map_err(|cause| {
            crate::Error::with_context(format!("multiview view={index} phase=inspection"), cause)
        })?;
    }
    Ok(())
}

fn validate_views(views: &[View<'_>]) -> Result<()> {
    if !matches!(views.len(), 1 | 2 | 4) {
        return Err("multiview phase=request: expected 1, 2, or 4 views".into());
    }
    for view in views {
        let [x, y, w, h] = view.placement.viewport;
        if ![x, y, w, h].iter().all(|v| v.is_finite()) || x < 0. || y < 0. || w <= 0. || h <= 0. {
            return Err("multiview phase=request: invalid viewport".into());
        }
        validate_canvas_image(view.placement.image)?;
    }
    for (index, view) in views.iter().enumerate() {
        let [x, y, w, h] = view.placement.viewport;
        if views[..index].iter().any(|other| {
            let [px, py, pw, ph] = other.placement.viewport;
            x < px + pw && x + w > px && y < py + ph && y + h > py
        }) {
            return Err("multiview phase=request: overlapping viewports".into());
        }
    }
    Ok(())
}
fn validate_surface(views: &[View<'_>], size: [u32; 2]) -> Result<()> {
    if views.iter().any(|view| {
        let [x, y, w, h] = view.placement.viewport;
        x + w > size[0] as f32 || y + h > size[1] as f32
    }) {
        return Err("multiview phase=request: viewport exceeds surface".into());
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod tests;

#[cfg(test)]
mod interaction_tests;
