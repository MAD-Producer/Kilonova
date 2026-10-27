//! Native model layers rasterize in composition space with their sampled camera
//! and world transforms. This isolated layer pass does not share mesh depth with
//! another layer or with the plane compositor.
use super::*;
use crate::scene_gpu::{GpuScene, SceneFrame, SceneGpuLimits, SceneRenderStats, SceneRequest};
use motion_model::ModelAsset;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};

const MAX_CATALOG_BYTES: u64 = 320 * 1024 * 1024;
// Accounted retained payload, including owned encoded resources and decoded
// geometry arrays. Temporary decoder allocations are not a process RAM cap.
const MAX_CATALOG_RETAINED_BYTES: usize = 320 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct Source {
    asset: ModelAsset,
    root: PathBuf,
    scene: Arc<motion_scene::Scene>,
}
#[derive(Default)]
pub(super) struct Models {
    sources: HashMap<u64, Source>,
    gpu: HashMap<u64, GpuScene>,
    targets: HashMap<u64, RenderTarget>,
}
impl Models {
    pub fn clear_gpu(&mut self) {
        self.gpu.clear();
        self.targets.clear();
    }
    pub fn clear_targets(&mut self) {
        self.targets.clear();
    }
    pub fn retain_targets(&mut self, live: &std::collections::HashSet<u64>) {
        self.targets.retain(|id, _| live.contains(id));
    }
    pub fn texture_bytes(&self) -> u64 {
        self.gpu.values().map(GpuScene::texture_bytes).sum()
    }
    pub fn source_texture_bytes(&self) -> u64 {
        self.gpu.values().map(GpuScene::source_texture_bytes).sum()
    }
    pub fn install(&mut self, sources: HashMap<u64, Source>) {
        self.gpu.retain(|id, _| {
            self.sources
                .get(id)
                .zip(sources.get(id))
                .is_some_and(|(old, new)| old.asset == new.asset && old.root == new.root)
        });
        self.targets.clear();
        self.sources = sources;
    }
    pub fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        for gpu in self.gpu.values() {
            ledger.merge(gpu.texture_resource_ledger());
        }
    }
}

pub(super) fn prepare_sources(
    project: &Project,
    root: &Path,
    previous: &Models,
) -> Result<HashMap<u64, Source>> {
    prepare_sources_with_limit(project, root, previous, MAX_CATALOG_RETAINED_BYTES)
}
fn prepare_sources_with_limit(
    project: &Project,
    root: &Path,
    previous: &Models,
    retained_limit: usize,
) -> Result<HashMap<u64, Source>> {
    let began = Instant::now();
    let result = (|| {
        let mut sources = HashMap::new();
        if project.model_assets.is_empty() {
            return Ok(sources);
        }
        let total = project
            .model_assets
            .iter()
            .try_fold(0u64, |total, a| total.checked_add(a.bytes))
            .ok_or_else(|| RenderError::Invalid("model catalog size overflow".into()))?;
        if total > MAX_CATALOG_BYTES {
            return Err(RenderError::Invalid(format!(
                "model catalog encoded_bytes={total} limit={MAX_CATALOG_BYTES}"
            )));
        }
        log::info!(
            "start model_catalog phase=prepare sources={} encoded_bytes={total}",
            project.model_assets.len()
        );
        let root = root.canonicalize()?;
        let mut retained = 0usize;
        for asset in &project.model_assets {
            asset
                .validate()
                .map_err(|e| RenderError::Invalid(e.to_string()))?;
            if let Some(source) = previous
                .sources
                .get(&asset.id)
                .filter(|source| source.asset == *asset && source.root == root)
            {
                retained = charge_retained(
                    retained,
                    source.scene.retained_bytes(),
                    retained_limit,
                    asset.id,
                )?;
                sources.insert(asset.id, source.clone());
                continue;
            }
            log::info!(
                "start model_source_open source={} phase=frozen bytes={}",
                asset.id,
                asset.bytes
            );
            let path = root.join(&asset.path).canonicalize()?;
            if !path.starts_with(&root) {
                return Err(RenderError::Invalid(format!(
                    "model source={} resolves outside project",
                    asset.id
                )));
            }
            let file = File::open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() != asset.bytes {
                return Err(RenderError::Invalid(format!(
                    "model source={} frozen byte count mismatch",
                    asset.id
                )));
            }
            let mut bytes = Vec::new();
            file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != asset.bytes
                || format!("{:x}", Sha256::digest(&bytes)) != asset.sha256
            {
                return Err(RenderError::Invalid(format!(
                    "model source={} frozen SHA-256 mismatch",
                    asset.id
                )));
            }
            let mut limits = motion_scene::ImportLimits::default();
            limits.max_retained_bytes = limits
                .max_retained_bytes
                .min(retained_limit.saturating_sub(retained));
            let scene = motion_scene::import_frozen(
                motion_scene::ImportContext {
                    request: 0,
                    source: asset.id,
                },
                &bytes,
                motion_model::model_assets::MAX_MODEL_ASSET_BYTES as usize,
                &limits,
            )
            .map_err(|e| RenderError::Invalid(e.to_string()))?;
            retained = charge_retained(retained, scene.retained_bytes(), retained_limit, asset.id)?;
            sources.insert(
                asset.id,
                Source {
                    asset: asset.clone(),
                    root: root.clone(),
                    scene: Arc::new(scene),
                },
            );
        }
        // Geometry scene indices are checked before committing a candidate catalog.
        for layer in project
            .layers
            .iter()
            .chain(project.compositions.iter().flat_map(|c| &c.layers))
        {
            if let motion_model::Content::Model { model } = &layer.content {
                if sources
                    .get(&model.asset)
                    .is_none_or(|s| model.scene as usize >= s.scene.scenes().len())
                {
                    return Err(RenderError::Invalid(format!(
                        "model object={} source={} scene={} missing",
                        layer.id, model.asset, model.scene
                    )));
                }
                if let Some(animation) = &model.animation {
                    if sources
                        .get(&model.asset)
                        .is_none_or(|s| animation.clip as usize >= s.scene.animations().len())
                    {
                        return Err(RenderError::Invalid(format!(
                            "model object={} source={} animation={} missing",
                            layer.id, model.asset, animation.clip
                        )));
                    }
                }
            }
        }
        Ok(sources)
    })();
    if let Err(error) = &result {
        let cause: String = error.to_string().chars().take(1024).collect();
        log::error!(
            "model catalog phase=prepare elapsed_ms={} cause={cause}",
            began.elapsed().as_millis()
        );
    }
    result
}
fn charge_retained(current: usize, bytes: usize, limit: usize, source: u64) -> Result<usize> {
    current
        .checked_add(bytes)
        .filter(|total| *total <= limit)
        .ok_or_else(|| {
            RenderError::Invalid(format!(
                "model source={source} catalog retained payload exceeds limit={limit}"
            ))
        })
}

impl Renderer {
    pub(super) fn prepare_models(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
    ) -> Result<RenderStats> {
        let began = Instant::now();
        let mut stats = RenderStats::default();
        // Nested references have distinct render instance IDs. Their images are
        // retained while rendering a parent; source catalog edits clear all targets.
        for layer in scene.layers.iter().filter(|l| l.model_source.is_some()) {
            let clip = layer
                .model_source
                .as_ref()
                .ok_or_else(|| RenderError::Invalid("model clip missing".into()))?;
            let source = self.models.sources.get(&clip.asset).ok_or_else(|| {
                RenderError::Invalid(format!(
                    "model object={} source={} not configured",
                    layer.id, clip.asset
                ))
            })?;
            let mut gpu = match self.models.gpu.remove(&clip.asset) {
                Some(gpu) => gpu,
                None => {
                    let source = source.scene.clone();
                    self.trim_image_cache(0);
                    let available = self
                        .effect_gpu
                        .builder
                        .source_texture_budget
                        .saturating_sub(self.resident_source_texture_bytes());
                    GpuScene::upload(
                        self,
                        SceneRequest {
                            request: 0,
                            source: clip.asset,
                        },
                        source,
                        SceneGpuLimits {
                            source_texture_bytes: available,
                            ..Default::default()
                        },
                    )
                    .map_err(scene_error)?
                }
            };
            let result = self.prepare_model_layer(scene, layer, width, height, &mut gpu);
            // Retain immutable ownership and pipeline state on either result.
            self.models.gpu.insert(clip.asset, gpu);
            let model = result?;
            stats.draw_calls += model.draws as u32;
            stats.parameter_upload_bytes += model.parameter_bytes;
        }
        stats.cpu_prepare_us = began.elapsed().as_micros() as u64;
        Ok(stats)
    }
    fn prepare_model_layer(
        &mut self,
        scene: &Scene,
        layer: &crate::DrawLayer,
        width: u32,
        height: u32,
        gpu: &mut GpuScene,
    ) -> Result<SceneRenderStats> {
        let clip = layer
            .model_source
            .as_ref()
            .ok_or_else(|| RenderError::Invalid("model clip missing".into()))?;
        let [_, _, w, h] = crate::fit_viewport(
            [0., 0., width as f32, height as f32],
            [scene.width, scene.height],
        );
        let (w, h) = (w.ceil().max(1.) as u32, h.ceil().max(1.) as u32);
        let key = TextureKey::Model(layer.id);
        let previous = self.images.get(&key).map_or(0, |image| image.bytes);
        let bytes = u64::from(w)
            * u64::from(h)
            * crate::image_formats::color_storage(self.working_precision()).bytes_per_pixel();
        // The current scene is temporarily removed from Models during drawing.
        let required = self
            .resident_source_texture_bytes()
            .saturating_sub(previous)
            + gpu.source_texture_bytes()
            + bytes;
        if required > self.effect_gpu.builder.source_texture_budget {
            return Err(RenderError::budget(
                ResourceBudgetScope::SourceTextures,
                required,
                self.effect_gpu.builder.source_texture_budget,
                format!(
                    "model object={} raster exceeds source texture budget",
                    layer.id
                ),
            ));
        }
        let target = match self.models.targets.remove(&layer.id) {
            Some(target)
                if target.width == w
                    && target.height == h
                    && target.texture.format() == self.target_format =>
            {
                target
            }
            _ => self.render_target_in(w, h, crate::TextureCategory::ModelSources)?,
        };
        let mut frame = SceneFrame::new(scene.camera, clip.scene as usize, clip.pixels_per_metre);
        frame.world = layer.model
            * glam::Mat4::from_translation(
                -glam::Vec3::from_array(clip.pivot) * clip.pixels_per_metre,
            );
        frame.samples = clip.samples;
        frame.animation = layer.model_animation;
        let result = if frame.world.determinant().abs() <= 1e-12 {
            // A zero-scale key is an ordinary empty visual, not a corrupt scene.
            let mut encoder = self.allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("empty model layer"),
            })?;
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("empty model clear"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
            }
            self.submit_allocated(encoder.finish(), |ledger| {
                target.collect_texture_resources(
                    ledger,
                    "model/empty",
                    crate::TextureState::Retained,
                )
            })
            .map(|_| SceneRenderStats::default())
        } else {
            gpu.draw(
                self,
                SceneRequest {
                    request: 0,
                    source: clip.asset,
                },
                &target,
                &frame,
            )
            .map_err(scene_error)
        };
        if result.is_ok() {
            if self
                .images
                .get(&key)
                .is_none_or(|image| image.view.texture() != &*target.texture)
            {
                let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Model layer raster"),
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
                        _texture: target.texture.clone(),
                        view: target.view.view.clone(),
                        bind_group: group,
                        bytes,
                        size: (w, h),
                        video_stamp: None,
                    },
                );
                self.texture_bytes = self.texture_bytes - previous + bytes;
                self.effect_gpu.invalidate();
            }
        }
        self.models.targets.insert(layer.id, target);
        result
    }
}
fn scene_error(error: crate::scene_gpu::SceneGpuError) -> RenderError {
    let message = error.to_string();
    match error {
        crate::scene_gpu::SceneGpuError::Render(error) => error,
        crate::scene_gpu::SceneGpuError::Deformation { cause, .. }
        | crate::scene_gpu::SceneGpuError::SkinDeformation { cause, .. }
        | crate::scene_gpu::SceneGpuError::PassInstance { cause, .. } => {
            scene_error(*cause).with_diagnostic(message)
        }
        _ => RenderError::Invalid(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct NoResources;
    impl motion_scene::AssetResolver for NoResources {
        fn read(
            &mut self,
            _: &str,
            _: motion_scene::ResourceKind,
            _: usize,
        ) -> std::io::Result<Vec<u8>> {
            Err(std::io::Error::other("fixture has no external resources"))
        }
    }

    #[test]
    fn wrapped_native_deformation_keeps_context_and_original_resource_refusal() {
        use crate::scene_gpu::SceneGpuError;
        let original = RenderError::budget(
            ResourceBudgetScope::RenderAllocations,
            216,
            215,
            "original native vertex refusal",
        );
        let wrapped = SceneGpuError::PassInstance {
            instance: 5,
            request: 8704,
            source_id: 7,
            cause: Box::new(SceneGpuError::Deformation {
                node: 2,
                mesh: 3,
                primitive: 4,
                cause: Box::new(SceneGpuError::Render(original)),
            }),
        };
        let error = scene_error(wrapped);
        assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: 216,
                budget_bytes: 215,
            })
        );
        let message = error.to_string();
        for part in [
            "instance=5",
            "request=#8704",
            "source=7",
            "node=2 mesh=3 primitive=4 phase=morph",
            "original native vertex refusal",
        ] {
            assert!(message.contains(part), "missing {part}: {message}");
        }
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        let mut original_retained = false;
        while let Some(error) = cause {
            // Thiserror exposes boxed sources as Box<RenderError>, as at the
            // existing mask/composition error boundaries.
            let render_error = error
                .downcast_ref::<RenderError>()
                .or_else(|| error.downcast_ref::<Box<RenderError>>().map(Box::as_ref));
            if let Some(RenderError::Resource { message, .. }) = render_error {
                original_retained |= message == "original native vertex refusal";
            }
            cause = error.source();
        }
        assert!(
            original_retained,
            "context must retain the original typed error, not reconstruct only its fields"
        );
    }

    #[test]
    fn wrapped_native_skin_keeps_original_typed_resource_refusal() {
        use crate::scene_gpu::SceneGpuError;
        let wrapped = SceneGpuError::PassInstance {
            instance: 6,
            request: 8709,
            source_id: 8,
            cause: Box::new(SceneGpuError::SkinDeformation {
                node: 4,
                mesh: 2,
                primitive: 1,
                cause: Box::new(SceneGpuError::Render(RenderError::budget(
                    ResourceBudgetScope::RenderAllocations,
                    216,
                    215,
                    "original skin vertex refusal",
                ))),
            }),
        };
        let error = scene_error(wrapped);
        assert_eq!(
            error.resource_rejection(),
            Some(&ResourceRejection::Budget {
                scope: ResourceBudgetScope::RenderAllocations,
                required_bytes: 216,
                budget_bytes: 215,
            })
        );
        let message = error.to_string();
        for part in [
            "instance=6",
            "request=#8709",
            "source=8",
            "node=4 mesh=2 primitive=1 phase=skin",
            "original skin vertex refusal",
        ] {
            assert!(message.contains(part), "missing {part}: {message}");
        }
        let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&error);
        let mut original = false;
        while let Some(error) = cause {
            let render_error = error
                .downcast_ref::<RenderError>()
                .or_else(|| error.downcast_ref::<Box<RenderError>>().map(Box::as_ref));
            if let Some(RenderError::Resource { message, .. }) = render_error {
                original |= message == "original skin vertex refusal";
            }
            cause = error.source();
        }
        assert!(
            original,
            "skin context must retain the original typed source"
        );
    }

    #[test]
    fn model_catalog_bounds_new_and_cached_retained_payloads_together() {
        let root = tempfile::tempdir().unwrap();
        let source = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let scene = motion_scene::obj::import_obj(
            motion_scene::ImportContext {
                request: 8706,
                source: 7,
            },
            source,
            &mut NoResources,
            &Default::default(),
            &Default::default(),
        )
        .unwrap();
        let frozen = scene.freeze(source, 1 << 20).unwrap();
        let path = root.path().join("assets/models/a.mscn");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &frozen).unwrap();
        let mut project = Project::new(96, 64, 30, 1).unwrap();
        let asset = ModelAsset {
            id: 7,
            path: "assets/models/a.mscn".into(),
            bytes: frozen.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&frozen)),
        };
        project.model_assets = vec![asset.clone(), ModelAsset { id: 8, ..asset }];
        let sources = prepare_sources(&project, root.path(), &Models::default()).unwrap();
        let retained: usize = sources
            .values()
            .map(|source| source.scene.retained_bytes())
            .sum();
        assert!(retained > 0);
        assert!(
            prepare_sources_with_limit(&project, root.path(), &Models::default(), retained - 1)
                .err()
                .unwrap()
                .to_string()
                .contains("limit"),
            "new imports must share the remaining payload allowance"
        );
        let previous = Models {
            sources,
            ..Default::default()
        };
        std::fs::remove_file(path).unwrap();
        assert!(
            prepare_sources_with_limit(&project, root.path(), &previous, retained - 1)
                .err()
                .unwrap()
                .to_string()
                .contains("limit"),
            "cache hits must not bypass the catalog allowance"
        );
        assert_eq!(
            prepare_sources_with_limit(&project, root.path(), &previous, retained)
                .unwrap()
                .len(),
            2,
            "an unchanged, admitted catalog remains owned without reopening deleted files"
        );
    }
}
