//! Independent screen-space inputs never replace a source's visible geometry.
use super::*;
use std::ops::{Deref, DerefMut};

pub(super) fn bindings(
    renderer: &Renderer,
    scene: &mut Scene,
    source_scene: &Scene,
    projected: &HashMap<u64, RenderTarget>,
) -> Result<Vec<(TextureKey, GpuImage)>> {
    let needed: std::collections::BTreeSet<_> = scene
        .layers
        .iter()
        .filter_map(|layer| layer.track_matte.map(|matte| matte.source))
        .filter(|source| {
            source_scene.depth_surface(*source).is_some() && projected.contains_key(source)
        })
        .collect();
    if needed.is_empty() {
        return Ok(Vec::new());
    }
    if scene.layers.len().saturating_add(needed.len()) > MAX_LAYERS {
        return Err(RenderError::Invalid(format!(
            "depth matte phase=input_alias: {} sampled layers plus {} independent inputs exceed draw capacity {MAX_LAYERS}",
            scene.layers.len(), needed.len())));
    }
    // Scan at most the bounded occupied set plus one. User IDs may be u64::MAX;
    // arithmetic based on the largest authored ID would overflow or collide.
    let mut occupied: std::collections::HashSet<_> = scene.layers.iter().map(|l| l.id).collect();
    occupied.extend(source_scene.layers.iter().map(|l| l.id));
    occupied.extend(projected.keys().copied());
    occupied.extend(renderer.images.keys().filter_map(|key| match key {
        TextureKey::Model(id) => Some(*id),
        _ => None,
    }));
    let mut bindings = Vec::with_capacity(needed.len());
    for source in needed {
        let alias = (1..=occupied.len() as u64 + 1)
            .find(|id| !occupied.contains(id))
            .ok_or_else(|| {
                RenderError::Invalid("depth matte phase=input_alias: no free input ID".into())
            })?;
        occupied.insert(alias);
        let target = &projected[&source];
        // The output may have removed a hidden source or grouped its visible
        // geometry. Always derive the input from the original sampled source.
        let mut layer = source_scene
            .layers
            .iter()
            .find(|l| l.id == source)
            .cloned()
            .ok_or_else(|| {
                RenderError::Invalid(format!(
                    "depth matte source={source} phase=input_alias: no layer context"
                ))
            })?;
        layer.id = alias;
        layer.order = scene.layers.len();
        layer.asset = None;
        layer.video = None;
        layer.vector = None;
        layer.model_source = None;
        layer.model_animation = None;
        layer.masks = Default::default();
        layer.track_matte = None;
        layer.adjustment = false;
        layer.composition = false;
        layer.composite_visible = false;
        layer.blend = Default::default();
        layer.sampling = Default::default();
        layer.size = [scene.width as f32, scene.height as f32];
        layer.source_size = layer.size;
        layer.source_rect = [0., 0., layer.size[0], layer.size[1]];
        layer.model = glam::Mat4::IDENTITY;
        layer.color = [1.; 4];
        layer.opacity = 1.;
        layer.screen_overlay = true;
        layer.three_d = false;
        layer.view_projection = glam::Mat4::from_scale(glam::Vec3::new(
            2. / scene.width as f32,
            2. / scene.height as f32,
            1.,
        ));
        for receiver in &mut scene.layers {
            if let Some(matte) = &mut receiver.track_matte {
                if matte.source == source {
                    matte.source = alias;
                }
            }
        }
        scene.layers.push(layer);
        scene.shared_rasters.insert(alias);
        let bind_group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("independent projected matte input"),
                layout: &renderer.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                    },
                ],
            });
        bindings.push((
            TextureKey::Model(alias),
            GpuImage {
                _texture: target.texture.clone(),
                view: target.view.view.clone(),
                bind_group,
                bytes: target.texture_bytes(),
                size: (target.width, target.height),
                video_stamp: None,
                plane_image: None,
            },
        ));
    }
    Ok(bindings)
}

/// The cache owns aliases only during this encode, including error/unwind exits.
/// Encoded GPU commands retain their admitted textures until submit/abandonment.
pub(super) struct Inputs<'a> {
    renderer: &'a mut Renderer,
    previous: Vec<(TextureKey, Option<GpuImage>)>,
}
impl<'a> Inputs<'a> {
    pub(super) fn install(
        renderer: &'a mut Renderer,
        bindings: Vec<(TextureKey, GpuImage)>,
    ) -> Self {
        let mut guard = Self {
            renderer,
            previous: Vec::with_capacity(bindings.len()),
        };
        for (key, image) in bindings {
            let bytes = image.bytes;
            let previous = guard.renderer.images.insert(key, image);
            guard.renderer.texture_bytes = guard.renderer.texture_bytes
                - previous.as_ref().map_or(0, |image| image.bytes)
                + bytes;
            guard.previous.push((key, previous));
        }
        if !guard.previous.is_empty() {
            guard.renderer.effect_gpu.invalidate();
        }
        guard
    }
}
impl Deref for Inputs<'_> {
    type Target = Renderer;
    fn deref(&self) -> &Self::Target {
        self.renderer
    }
}
impl DerefMut for Inputs<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.renderer
    }
}
impl Drop for Inputs<'_> {
    fn drop(&mut self) {
        if self.previous.is_empty() {
            return;
        }
        for (key, previous) in self.previous.drain(..).rev() {
            if let Some(image) = self.renderer.images.remove(&key) {
                self.renderer.texture_bytes -= image.bytes;
            }
            if let Some(image) = previous {
                self.renderer.texture_bytes += image.bytes;
                self.renderer.images.insert(key, image);
            }
        }
        self.renderer.effect_gpu.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(renderer: &Renderer, target: &RenderTarget) -> GpuImage {
        let bind_group = renderer
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("test admitted matte alias"),
                layout: &renderer.image_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&target.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&renderer.sampler),
                    },
                ],
            });
        GpuImage {
            _texture: target.texture.clone(),
            view: target.view.view.clone(),
            bind_group,
            bytes: target.texture_bytes(),
            size: (target.width, target.height),
            video_stamp: None,
            plane_image: None,
        }
    }

    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn temporary_inputs_restore_original_texture_and_accounting_on_encode_error_and_unwind() {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let original = renderer.render_target(2, 2).unwrap();
        let replacement = renderer.render_target(3, 3).unwrap();
        let key = TextureKey::Model(17);
        let original_image = image(&renderer, &original);
        renderer.texture_bytes += original_image.bytes;
        renderer.images.insert(key, original_image);
        let booked = renderer.texture_bytes;
        let count = renderer.images.len();
        let scene = Scene::new(&Project::new(8, 8, 30, 1).unwrap()).unwrap();
        {
            let input = image(&renderer, &replacement);
            let extra = image(&renderer, &replacement);
            let mut inputs = Inputs::install(
                &mut renderer,
                vec![(key, input), (TextureKey::Model(18), extra)],
            );
            assert_eq!(
                inputs.texture_bytes,
                inputs.images.values().map(|i| i.bytes).sum::<u64>()
            );
            let mut encoder = inputs
                .allocation_command_encoder(&Default::default())
                .unwrap();
            let error = inputs
                .encode_local(&scene, &replacement.view, 0, 8, &mut encoder, None, false)
                .unwrap_err();
            assert!(
                error.to_string().contains("invalid render target"),
                "{error}"
            );
        }
        assert_eq!(renderer.texture_bytes, booked);
        assert_eq!(renderer.images.len(), count);
        assert_eq!(
            renderer.images[&key].view.texture(),
            original.view.texture()
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let input = image(&renderer, &replacement);
            let _inputs = Inputs::install(&mut renderer, vec![(key, input)]);
            panic!("controlled unwind with a temporary admitted matte input");
        }));
        assert!(result.is_err());
        assert_eq!(renderer.texture_bytes, booked);
        assert_eq!(renderer.images.len(), count);
        assert_eq!(
            renderer.images[&key].view.texture(),
            original.view.texture()
        );
        assert_eq!(
            renderer.texture_bytes,
            renderer.images.values().map(|i| i.bytes).sum::<u64>()
        );
    }
}
