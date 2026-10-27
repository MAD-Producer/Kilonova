//! Native model layers rasterize in composition space with their sampled camera
//! and world transforms. This isolated layer pass does not share mesh depth with
//! another layer or with the plane compositor.
use super::*;
use crate::scene_gpu::{GpuScene, SceneFrame, SceneGpuLimits, SceneRequest};
use motion_model::ModelAsset;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf};

const MAX_CATALOG_BYTES: u64 = 320 * 1024 * 1024;

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
    pub fn clear_gpu(&mut self) { self.gpu.clear(); self.targets.clear(); }
    pub fn clear_targets(&mut self) { self.targets.clear(); }
    pub fn retain_targets(&mut self, live: &std::collections::HashSet<u64>) { self.targets.retain(|id, _| live.contains(id)); }
    pub fn texture_bytes(&self) -> u64 { self.gpu.values().map(|gpu| gpu.texture_resource_ledger().total_known_bytes()).sum() }
    pub fn install(&mut self, sources: HashMap<u64, Source>) {
        self.gpu.retain(|id, _| self.sources.get(id).zip(sources.get(id))
            .is_some_and(|(old, new)| old.asset == new.asset && old.root == new.root));
        self.targets.clear();
        self.sources = sources;
    }
    pub fn collect_textures(&self, ledger: &mut crate::TextureResourceLedger) {
        for gpu in self.gpu.values() { ledger.merge(gpu.texture_resource_ledger()); }
    }
}

pub(super) fn prepare_sources(project: &Project, root: &Path, previous: &Models) -> Result<HashMap<u64, Source>> {
    let began = Instant::now();
    let result = (|| {
        let mut sources = HashMap::new();
        if project.model_assets.is_empty() { return Ok(sources); }
        let total = project.model_assets.iter().try_fold(0u64, |total, a| total.checked_add(a.bytes))
            .ok_or_else(|| RenderError::Invalid("model catalog size overflow".into()))?;
        if total > MAX_CATALOG_BYTES {
            return Err(RenderError::Invalid(format!("model catalog encoded_bytes={total} limit={MAX_CATALOG_BYTES}")));
        }
        let root = root.canonicalize()?;
        for asset in &project.model_assets {
            asset.validate().map_err(|e| RenderError::Invalid(e.to_string()))?;
            if let Some(source) = previous.sources.get(&asset.id)
                .filter(|source| source.asset == *asset && source.root == root) {
                sources.insert(asset.id, source.clone());
                continue;
            }
            log::info!("start model_source_open source={} phase=frozen bytes={}", asset.id, asset.bytes);
            let path = root.join(&asset.path).canonicalize()?;
            if !path.starts_with(&root) {
                return Err(RenderError::Invalid(format!("model source={} resolves outside project", asset.id)));
            }
            let file = File::open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || metadata.len() != asset.bytes {
                return Err(RenderError::Invalid(format!("model source={} frozen byte count mismatch", asset.id)));
            }
            let mut bytes = Vec::new();
            file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != asset.bytes || format!("{:x}", Sha256::digest(&bytes)) != asset.sha256 {
                return Err(RenderError::Invalid(format!("model source={} frozen SHA-256 mismatch", asset.id)));
            }
            let scene = motion_scene::import_frozen(motion_scene::ImportContext { request: 0, source: asset.id },
                &bytes, motion_model::model_assets::MAX_MODEL_ASSET_BYTES as usize,
                &motion_scene::ImportLimits::default()).map_err(|e| RenderError::Invalid(e.to_string()))?;
            sources.insert(asset.id, Source { asset: asset.clone(), root: root.clone(), scene: Arc::new(scene) });
        }
        // Geometry scene indices are checked before committing a candidate catalog.
        for layer in project.layers.iter().chain(project.compositions.iter().flat_map(|c| &c.layers)) {
            if let motion_model::Content::Model { model } = &layer.content {
                if sources.get(&model.asset).is_none_or(|s| model.scene as usize >= s.scene.scenes().len()) {
                    return Err(RenderError::Invalid(format!("model object={} source={} scene={} missing", layer.id, model.asset, model.scene)));
                }
            }
        }
        Ok(sources)
    })();
    if let Err(error) = &result {
        let cause: String = error.to_string().chars().take(1024).collect();
        log::error!("model catalog phase=prepare elapsed_ms={} cause={cause}", began.elapsed().as_millis());
    }
    result
}

impl Renderer {
    pub(super) fn prepare_models(&mut self, scene: &Scene, width: u32, height: u32) -> Result<()> {
        // Nested references have distinct render instance IDs. Their images are
        // retained while rendering a parent; source catalog edits clear all targets.
        for layer in scene.layers.iter().filter(|l| l.model_source.is_some()) {
            let clip = layer.model_source.as_ref().ok_or_else(|| RenderError::Invalid("model clip missing".into()))?;
            let source = self.models.sources.get(&clip.asset)
                .ok_or_else(|| RenderError::Invalid(format!("model object={} source={} not configured", layer.id, clip.asset)))?;
            let mut gpu = match self.models.gpu.remove(&clip.asset) {
                Some(gpu) => gpu,
                None => GpuScene::upload(self, SceneRequest { request: 0, source: clip.asset },
                    source.scene.clone(), SceneGpuLimits::default()).map_err(scene_error)?,
            };
            let result = self.prepare_model_layer(scene, layer, width, height, &mut gpu);
            // Retain immutable ownership and pipeline state on either result.
            self.models.gpu.insert(clip.asset, gpu);
            result?;
        }
        Ok(())
    }
    fn prepare_model_layer(&mut self, scene: &Scene, layer: &crate::DrawLayer, width: u32, height: u32,
        gpu: &mut GpuScene) -> Result<()> {
        let clip = layer.model_source.as_ref().ok_or_else(|| RenderError::Invalid("model clip missing".into()))?;
        let [_, _, w, h] = crate::fit_viewport([0., 0., width as f32, height as f32], [scene.width, scene.height]);
        let (w, h) = (w.ceil().max(1.) as u32, h.ceil().max(1.) as u32);
        let key = TextureKey::Model(layer.id);
        let previous = self.images.get(&key).map_or(0, |image| image.bytes);
        let bytes = u64::from(w) * u64::from(h) * crate::image_formats::color_storage(self.working_precision()).bytes_per_pixel();
        let required = self.texture_bytes.saturating_sub(previous) + self.video_plane_bytes() + self.effect_gpu.state.resource_bytes + bytes;
        if required > self.effect_gpu.builder.source_texture_budget {
            return Err(RenderError::budget(ResourceBudgetScope::SourceTextures, required,
                self.effect_gpu.builder.source_texture_budget, format!("model object={} raster exceeds source texture budget", layer.id)));
        }
        let target = match self.models.targets.remove(&layer.id) {
            Some(target) if target.width == w && target.height == h && target.texture.format() == self.target_format => target,
            _ => self.render_target_in(w, h, crate::TextureCategory::ModelSources)?,
        };
        let mut frame = SceneFrame::new(scene.camera, clip.scene as usize, clip.pixels_per_metre);
        frame.world = layer.model * glam::Mat4::from_translation(-glam::Vec3::from_array(clip.pivot) * clip.pixels_per_metre);
        frame.samples = clip.samples;
        let result = if frame.world.determinant().abs() <= 1e-12 {
            // A zero-scale key is an ordinary empty visual, not a corrupt scene.
            let mut encoder = self.allocation_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("empty model layer") })?;
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("empty model clear"), color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view, depth_slice: None, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                    })], ..Default::default()
                });
            }
            self.submit_allocated(encoder.finish(), |ledger| target.collect_texture_resources(ledger, "model/empty", crate::TextureState::Retained)).map(|_| ())
        } else {
            gpu.draw(self, SceneRequest { request: 0, source: clip.asset }, &target, &frame).map(|_| ()).map_err(scene_error)
        };
        if result.is_ok() {
            if self.images.get(&key).is_none_or(|image| image.view.texture() != &*target.texture) {
                let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Model layer raster"), layout: &self.image_layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&target.view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
                });
                self.images.insert(key, GpuImage { _texture: target.texture.clone(), view: target.view.view.clone(),
                    bind_group: group, bytes, size: (w,h), video_stamp: None });
                self.texture_bytes = self.texture_bytes - previous + bytes;
                self.effect_gpu.invalidate();
            }
        }
        self.models.targets.insert(layer.id, target);
        result
    }
}
fn scene_error(error: crate::scene_gpu::SceneGpuError) -> RenderError {
    match error {
        crate::scene_gpu::SceneGpuError::Render(error) => error,
        other => RenderError::Invalid(other.to_string()),
    }
}
