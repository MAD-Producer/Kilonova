//! Exact native composition pixels, without chrome or an implicit output conversion.
//!
//! The host must supply authoritative content/output digests and invalidate the
//! context before accepting edited/source/device generations. This cache does not
//! create a scheduler, render future scenes or advertise a prerender capability.
use super::*;
use std::collections::VecDeque;

const MAX_FRAMES: usize = 8;

/// Digests include immutable source/plugin/model content and output interpretation.
/// A project revision or a path alone is not an authoritative content digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionFrameContext {
    pub document_generation: u64,
    pub device_generation: u64,
    pub content: [u8; 32],
    pub output: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompositionFrameKey {
    context: CompositionFrameContext,
    frame_bits: u64,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}
impl CompositionFrameKey {
    pub fn new(
        context: CompositionFrameContext,
        frame: f64,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) -> Result<Self> {
        if !frame.is_finite()
            || frame < 0.
            || width == 0
            || height == 0
            || u64::from(width) * u64::from(height) > CAPTURE_PIXEL_LIMIT
            || crate::image_formats::color_precision(format).is_none()
        {
            return Err(RenderError::Invalid(
                "invalid composition frame cache key".into(),
            ));
        }
        Ok(Self {
            context,
            frame_bits: if frame == 0. {
                0f64.to_bits()
            } else {
                frame.to_bits()
            },
            width,
            height,
            format,
        })
    }
    pub fn frame(self) -> f64 {
        f64::from_bits(self.frame_bits)
    }
}

/// Native pixels copied into separate storage before publication. A caller may
/// retain this handle across eviction; its allocation stays admitted until every
/// handle, binding and consuming GPU submission has retired.
pub struct CachedCompositionFrame {
    key: CompositionFrameKey,
    target: RenderTarget,
}
impl CachedCompositionFrame {
    /// Read-only source for existing native color-output and presentation APIs.
    /// Its storage intentionally has no render-attachment usage.
    pub fn source(&self) -> &RenderTarget {
        &self.target
    }
    pub fn key(&self) -> CompositionFrameKey {
        self.key
    }
    pub fn view(&self) -> &crate::AdmittedTextureView {
        &self.target.view
    }
    pub fn size(&self) -> [u32; 2] {
        [self.key.width, self.key.height]
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.key.format
    }
    pub fn texture_bytes(&self) -> u64 {
        self.target.texture_bytes()
    }
    pub fn collect_texture_resources(&self, ledger: &mut crate::TextureResourceLedger) {
        self.target.collect_texture_resources(
            ledger,
            "composition/cache",
            crate::TextureState::Retained,
        );
    }
}

/// Counters saturate over this cache instance's lifetime, including invalidation.
/// Resident bytes count cache-owned entries, not evicted handles held by callers
/// or pending GPU work. The shared allocation budget accounts for those owners.
#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct CompositionFrameCacheInfo {
    pub resident_frames: usize,
    pub resident_bytes: u64,
    pub quota_bytes: u64,
    pub max_frames: usize,
    pub hits: u64,
    pub misses: u64,
    pub installed: u64,
    pub evicted: u64,
    pub refused: u64,
    pub stale: u64,
}

pub struct CompositionFrameCache {
    context: CompositionFrameContext,
    owner: Option<Arc<()>>,
    entries: VecDeque<Arc<CachedCompositionFrame>>,
    info: CompositionFrameCacheInfo,
}
impl CompositionFrameCache {
    pub fn new(
        context: CompositionFrameContext,
        quota_bytes: u64,
        max_frames: usize,
    ) -> Result<Self> {
        validate_limit(max_frames)?;
        Ok(Self {
            context,
            owner: None,
            entries: VecDeque::new(),
            info: CompositionFrameCacheInfo {
                quota_bytes,
                max_frames,
                ..Default::default()
            },
        })
    }
    pub fn info(&self) -> CompositionFrameCacheInfo {
        CompositionFrameCacheInfo {
            resident_frames: self.entries.len(),
            resident_bytes: self.bytes(),
            ..self.info
        }
    }
    pub fn set_limits(&mut self, quota_bytes: u64, max_frames: usize) -> Result<()> {
        validate_limit(max_frames)?;
        self.info.quota_bytes = quota_bytes;
        self.info.max_frames = max_frames;
        while self.entries.len() > max_frames || self.bytes() > quota_bytes {
            self.evict();
        }
        Ok(())
    }
    pub fn invalidate(&mut self, context: CompositionFrameContext) {
        self.clear();
        self.context = context;
        self.owner = None;
    }
    pub fn clear(&mut self) {
        while !self.entries.is_empty() {
            self.evict();
        }
    }
    pub fn get(
        &mut self,
        renderer: &Renderer,
        key: CompositionFrameKey,
    ) -> Result<Option<Arc<CachedCompositionFrame>>> {
        if key.context != self.context {
            self.info.misses = self.info.misses.saturating_add(1);
            return Ok(None);
        }
        self.check_owner(renderer)?;
        let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.key == key && key.context == self.context)
        else {
            self.info.misses = self.info.misses.saturating_add(1);
            return Ok(None);
        };
        let entry = self
            .entries
            .remove(index)
            .ok_or_else(|| RenderError::Invalid("composition cache index changed".into()))?;
        self.entries.push_back(entry.clone());
        self.info.hits = self.info.hits.saturating_add(1);
        Ok(Some(entry))
    }
    /// Copy completed composition pixels, preserving exact storage and alpha.
    /// False means a stale context or an explicit quota/disabled refusal; it is
    /// not a rendering failure. Native allocation/submission faults retain their
    /// original error. This operation never reads pixels back or writes a file.
    pub fn store(
        &mut self,
        renderer: &Renderer,
        key: CompositionFrameKey,
        source: &RenderTarget,
    ) -> Result<bool> {
        if key.context != self.context {
            self.info.stale = self.info.stale.saturating_add(1);
            return Ok(false);
        }
        self.check_owner(renderer)?;
        source.check_owner(renderer)?;
        renderer.check_health()?;
        if (key.width, key.height, key.format)
            != (source.width, source.height, source.texture().format())
        {
            return Err(RenderError::Invalid(
                "composition cache key differs from native source layout".into(),
            ));
        }
        let bytes = source.texture_bytes();
        if self.info.max_frames == 0 || bytes > self.info.quota_bytes {
            self.info.refused = self.info.refused.saturating_add(1);
            return Ok(false);
        }
        if self.entries.iter().any(|entry| entry.key == key) {
            return Ok(true);
        }
        // Evict before allocation. Outstanding consumer handles remain charged
        // to the real shared budget and cannot be mistaken for released capacity.
        while self.entries.len() >= self.info.max_frames
            || self.bytes() > self.info.quota_bytes - bytes
        {
            self.evict();
        }
        let target = match copy(renderer, source) {
            Ok(target) => target,
            Err(cause) => {
                self.info.refused = self.info.refused.saturating_add(1);
                return Err(
                    cause.with_diagnostic("composition frame cache phase=native-copy".into())
                );
            }
        };
        self.owner = Some(renderer.allocation_owner.clone());
        self.entries
            .push_back(Arc::new(CachedCompositionFrame { key, target }));
        self.info.installed = self.info.installed.saturating_add(1);
        Ok(true)
    }
    fn check_owner(&self, renderer: &Renderer) -> Result<()> {
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| !Arc::ptr_eq(owner, &renderer.allocation_owner))
        {
            Err(RenderError::Invalid(
                "composition cache belongs to another renderer; invalidate its device generation"
                    .into(),
            ))
        } else {
            Ok(())
        }
    }
    fn bytes(&self) -> u64 {
        self.entries.iter().map(|entry| entry.texture_bytes()).sum()
    }
    fn evict(&mut self) {
        if self.entries.pop_front().is_some() {
            self.info.evicted = self.info.evicted.saturating_add(1);
        }
    }
}

fn validate_limit(frames: usize) -> Result<()> {
    if frames > MAX_FRAMES {
        Err(RenderError::Invalid(
            "composition cache exceeds eight frames".into(),
        ))
    } else {
        Ok(())
    }
}

fn copy(renderer: &Renderer, source: &RenderTarget) -> Result<RenderTarget> {
    let texture = renderer.device.create_admitted_texture(
        &wgpu::TextureDescriptor {
            label: Some("Motion Studio cached native composition"),
            size: wgpu::Extent3d {
                width: source.width,
                height: source.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: source.texture().format(),
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        &renderer.allocation_budget,
        crate::TextureCategory::PreviewTargets,
    )?;
    let target = RenderTarget {
        view: texture.admitted_view()?,
        texture,
        width: source.width,
        height: source.height,
        owner: renderer.allocation_owner.clone(),
    };
    let mut encoder = renderer.allocation_command_encoder_for(
        crate::gpu_commands::CommandStage::Composition,
        &wgpu::CommandEncoderDescriptor {
            label: Some("Motion Studio composition cache copy"),
        },
    )?;
    encoder.copy_texture_to_texture(
        source.texture().as_image_copy(),
        target.texture().as_image_copy(),
        wgpu::Extent3d {
            width: source.width,
            height: source.height,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |ledger| {
        source.collect_texture_resources(
            ledger,
            "composition/cache_source",
            crate::TextureState::Retained,
        );
        target.collect_texture_resources(
            ledger,
            "composition/cache_candidate",
            crate::TextureState::Retained,
        );
    })?;
    renderer.check_health()?;
    Ok(target)
}

#[cfg(test)]
mod tests;
