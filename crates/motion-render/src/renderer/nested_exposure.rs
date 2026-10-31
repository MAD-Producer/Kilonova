//! Resumable child-composition exposure, resolved entirely on the GPU.
use super::exposure_frame::PollBudget;
use super::*;
use crate::exposure_scene::{ExposureDocument, ExposureScenes, ExposureScope};
use motion_model::FrameEvaluator;
use std::sync::Arc;

struct Child {
    reference: u64,
    source_reference: u64,
    composition: String,
    frame: f64,
    source_rate: f64,
    size: [u32; 2],
}
struct Image {
    reference: u64,
    composition: String,
    frame: f64,
    target: RenderTarget,
    reservation: super::exposure::Reservation,
}

pub(super) struct NestedPreparation<E> {
    pub context: Scene,
    children: Vec<Child>,
    next: usize,
    active: Option<Box<ExposureFrame<E>>>,
    images: Vec<Image>,
    pub stats: RenderStats,
}
impl<E: FrameEvaluator> NestedPreparation<E> {
    pub fn new(scene: &Scene, size: [u32; 2]) -> Self {
        Self {
            context: scene.local_context(),
            children: scene
                .nested
                .iter()
                .map(|node| {
                    let (w, h) = nested_size(scene, &node.scene, size[0], size[1]);
                    Child {
                        reference: node.layer,
                        source_reference: scene.source_object(node.layer),
                        composition: node.composition.clone(),
                        frame: node.scene.frame,
                        source_rate: node.scene.source_rate,
                        size: [w, h],
                    }
                })
                .collect(),
            next: 0,
            active: None,
            images: vec![],
            stats: RenderStats::default(),
        }
    }
    pub fn poll(
        &mut self,
        renderer: &mut Renderer,
        factory: &(ExposureDocument, Arc<E>, ExposureScope),
        preview: bool,
        budget: &mut PollBudget,
        progress: &mut ExposureProgress,
        prepare: &mut dyn FnMut(&mut Renderer, &Scene) -> Result<bool>,
    ) -> Result<bool> {
        while let Some(child) = self.children.get(self.next) {
            let context = format!(
                "exposure composition={} reference={} sample_frame={} phase=nested_exposure",
                child.composition, child.reference, child.frame
            );
            let result = (|| {
                if self.active.is_none() {
                    if !budget.take() {
                        return Ok(false);
                    }
                    let scenes = ExposureScenes::new_shared(
                        factory.0.clone(),
                        &child.composition,
                        child.frame,
                        preview,
                        None,
                        factory.1.clone(),
                        Some(factory.2.child(child.source_reference, child.source_rate)),
                    )
                    .map_err(|error| RenderError::Invalid(error.to_string()))?;
                    self.active = Some(Box::new(ExposureFrame::new(
                        renderer,
                        scenes,
                        child.size[0],
                        child.size[1],
                    )?));
                    progress.work_units = progress.work_units.saturating_add(1);
                }
                let task = self.active.as_mut().ok_or_else(|| {
                    RenderError::Invalid("nested exposure active task missing".into())
                })?;
                let previous = task.progress().work_units;
                let result = task.poll_budget(renderer, budget, prepare);
                progress.work_units = progress
                    .work_units
                    .saturating_add(task.progress().work_units.saturating_sub(previous));
                match result? {
                    ExposurePoll::Pending(_) => return Ok(false),
                    ExposurePoll::Ready(stats) => {
                        if !budget.take() {
                            return Ok(false);
                        }
                        let pixel_bytes = crate::image_formats::color_storage(
                            renderer.effect_gpu.builder.precision()).bytes_per_pixel();
                        let bytes = u64::from(child.size[0]) * u64::from(child.size[1]) * pixel_bytes;
                        let reservation = renderer.reserve_exposure_bytes(bytes, &context)?;
                        let target = renderer.render_target_in(child.size[0], child.size[1], crate::TextureCategory::NestedCompositions)?;
                        renderer.resolve_exposure_view(task.target(), &target.view, |ledger| {
                            ledger.retained(
                                &target.texture,
                                crate::TextureCategory::ExposureTargets,
                                format!("exposure/nested/{}/resolved", child.reference),
                            );
                        })?;
                        self.images.push(Image {
                            reference: child.reference,
                            composition: child.composition.clone(),
                            frame: child.frame,
                            target,
                            reservation,
                        });
                        super::exposure_frame::add_stats(&mut self.stats, stats);
                        self.stats.texture_bytes = self
                            .stats
                            .texture_bytes
                            .max(renderer.texture_bytes() + renderer.exposure_texture_bytes());
                    }
                }
                // Only one child task is live; completed siblings keep only
                // their working-format resolved image, not another sample/mean pair.
                self.active = None;
                self.next += 1;
                progress.work_units = progress.work_units.saturating_add(1);
                Ok(true)
            })();
            if !result.map_err(|error: RenderError| {
                let message = format!("{context}: {error}");
                error.with_diagnostic(message)
            })? {
                return Ok(false);
            }
            if self.next < self.children.len() && !budget.available() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Transfer ownership from the task's temporal allowance to the ordinary
    /// source cache, after the parent media callback has retained its instances.
    /// There is no pixel copy and no uncharged surviving texture owner.
    pub fn install(&mut self, renderer: &mut Renderer) -> Result<()> {
        renderer.retain_video_instances(&self.context);
        for image in self.images.drain(..) {
            renderer.install_nested_exposure(image)?;
        }
        Ok(())
    }
    pub fn texture_resource_ledger(&self) -> crate::TextureResourceLedger {
        let mut ledger = crate::TextureResourceLedger::default();
        if let Some(task) = &self.active {
            ledger.merge(task.texture_resource_ledger());
        }
        for image in &self.images {
            ledger.retained(
                &image.target.texture,
                crate::TextureCategory::ExposureTargets,
                format!("exposure/nested/{}/resolved", image.reference),
            );
        }
        ledger
    }
}

impl Renderer {
    fn install_nested_exposure(&mut self, image: Image) -> Result<()> {
        self.check_health()?;
        let key = TextureKey::Video(image.reference);
        let bytes = image.reservation.bytes;
        let previous = self.images.get(&key).map_or(0, |image| image.bytes);
        self.trim_image_cache(bytes.saturating_sub(previous));
        let required = self.texture_bytes.saturating_sub(previous)
            + self.video_plane_bytes()
            + self.effect_gpu.state.resource_bytes
            + bytes;
        if required > self.effect_gpu.builder.source_texture_budget {
            return Err(RenderError::budget(ResourceBudgetScope::SourceTextures,required,
                self.effect_gpu.builder.source_texture_budget,
                format!("exposure composition={} object={} sample_frame={} phase=nested_install: resolved child exceeds source texture budget",
                    image.composition,image.reference,image.frame)));
        }
        let target = image.target;
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolved child exposure"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&target.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.images.insert(
            key,
            GpuImage {
                _texture: target.texture,
                view: target.view.view,
                bind_group,
                bytes,
                size: (target.width, target.height),
                video_stamp: None, plane_image: None,
            },
        );
        self.texture_bytes = self.texture_bytes.saturating_sub(previous) + bytes;
        self.composition_images.insert(image.reference);
        self.effect_gpu.invalidate();
        // Dropping reservation completes the ownership transfer. The ordinary
        // source cache now owns/accounts for this exact allocation identity.
        drop(image.reservation);
        Ok(())
    }
}
