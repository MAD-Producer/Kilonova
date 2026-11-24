//! Frozen working-space sources. Preparation creates bindings once; frame draws
//! borrow them without uploading pixels, reopening files or touching legacy caches.
use super::*;
use crate::{
    AuthoredSceneError, AuthoredSceneFrame, ColorImageContract, ColorImageTarget,
    TextureResourceLedger,
};
use motion_effects::{
    color_processor::CodeValueRange, image_contract::ChannelPrecision, AlphaMode,
};
use std::collections::BTreeMap;

pub(super) struct BoundImage {
    pub(super) bind_group: wgpu::BindGroup,
    declaration: motion_model::Asset,
    source: Arc<ColorImageTarget>,
}

/// Immutable source revision: the declaration and original prepared GPU target
/// travel together. A file watcher must explicitly prepare a replacement set;
/// drawing never stats or rereads a file. Shared targets count once in its ledger.
pub struct AuthoredImageSet {
    pub(super) entries: BTreeMap<u64, BoundImage>,
    pub(super) ledger: TextureResourceLedger,
    contract: ColorImageContract,
}
impl AuthoredImageSet {
    pub fn texture_resource_ledger(&self) -> TextureResourceLedger {
        self.ledger.clone()
    }
}
fn invalid(asset: Option<u64>, reason: &'static str) -> RenderError {
    AuthoredSceneError::Admission {
        phase: "source_binding",
        object: asset,
        reason,
    }
    .into()
}
impl Renderer {
    /// Run in the caller's observed non-Present source preparation operation.
    /// Every descriptor/owner/contract is checked before creating any binding.
    pub fn prepare_authored_images(
        &self,
        contract: ColorImageContract,
        sources: impl IntoIterator<Item = (motion_model::Asset, Arc<ColorImageTarget>)>,
    ) -> Result<AuthoredImageSet> {
        self.check_health()?;
        if self.target_format != wgpu::TextureFormat::Rgba32Float
            || contract.precision != ChannelPrecision::Float32
            || contract.alpha != AlphaMode::Premultiplied
            || contract.range != CodeValueRange::FULL
        {
            return Err(invalid(
                None,
                "working image bindings require full-range premultiplied Float32 storage",
            ));
        }
        let mut pending = BTreeMap::new();
        let mut ledger = TextureResourceLedger::default();
        for (declaration, source) in sources {
            if pending.len() >= MAX_LAYERS
                || declaration.id == 0
                || pending.contains_key(&declaration.id)
            {
                return Err(invalid(
                    Some(declaration.id),
                    "image binding capacity, identity or uniqueness is invalid",
                ));
            }
            source.image().check_owner(self)?;
            if source.contract() != contract {
                return Err(AuthoredSceneError::SourceContract {
                    asset: Some(declaration.id),
                    expected: contract,
                    actual: source.contract(),
                }
                .into());
            }
            if [source.image().width(), source.image().height()]
                != [declaration.width, declaration.height]
            {
                return Err(invalid(
                    Some(declaration.id),
                    "working image extent differs from its source declaration",
                ));
            }
            ledger.merge(source.texture_resource_ledger());
            pending.insert(declaration.id, (declaration, source));
        }
        self.check_authored_image_budget(&ledger)?;
        self.allocation_budget.check()?;
        let entries = pending
            .into_iter()
            .map(|(id, (declaration, source))| {
                let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("prepared OCIO working image"),
                    layout: &self.image_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(source.image().view()),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                (
                    id,
                    BoundImage {
                        bind_group,
                        declaration,
                        source,
                    },
                )
            })
            .collect();
        Ok(AuthoredImageSet {
            entries,
            ledger,
            contract,
        })
    }
    fn check_authored_image_budget(&self, ledger: &TextureResourceLedger) -> Result<()> {
        let requested = self
            .resident_source_texture_bytes()
            .saturating_add(ledger.total_known_bytes());
        let limit = self.effect_gpu.builder.source_texture_budget;
        if requested > limit {
            return Err(RenderError::budget(
                ResourceBudgetScope::SourceTextures,
                requested,
                limit,
                "prepared working images exceed source texture budget",
            ));
        }
        Ok(())
    }
    pub(super) fn validate_authored_images(
        &self,
        frame: &AuthoredSceneFrame,
        target: &ColorImageTarget,
        images: Option<&AuthoredImageSet>,
    ) -> Result<()> {
        if let Some(images) = images {
            if images.contract != frame.contract() {
                return Err(AuthoredSceneError::SourceContract {
                    asset: None,
                    expected: frame.contract(),
                    actual: images.contract,
                }
                .into());
            }
            self.check_authored_image_budget(&images.ledger)?;
            // Validate even unused entries: no cross-device set may be installed.
            for entry in images.entries.values() {
                entry.source.image().check_owner(self)?;
                if entry.source.image().view().texture() == target.image().view().texture() {
                    return Err(invalid(
                        Some(entry.declaration.id),
                        "source and destination textures alias",
                    ));
                }
            }
        }
        for layer in &frame.scene.layers {
            let Some(asset) = layer.asset else {
                continue;
            };
            let entry = images
                .and_then(|images| images.entries.get(&asset))
                .ok_or_else(|| {
                    invalid(
                        Some(asset),
                        "prepared working image is missing; legacy cache fallback is forbidden",
                    )
                })?;
            if frame.source(asset) != Some(&entry.declaration) {
                return Err(invalid(
                    Some(asset),
                    "prepared working image declaration is stale",
                ));
            }
        }
        Ok(())
    }
}
