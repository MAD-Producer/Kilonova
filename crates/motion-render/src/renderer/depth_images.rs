use super::*;
use crate::image_resources::{decoded_image_bytes, preview_rgba, Resolution};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

impl Renderer {
    pub(super) fn prepare_owned_depth_images(
        &mut self,
        scene: &Scene,
        resolution: Resolution,
        budget: u64,
    ) -> Result<()> {
        let mut images = BTreeMap::new();
        for (asset, image) in scene.calibrated_source_images() {
            if let Some(previous) = images.insert(asset, image) {
                if previous.fingerprint() != image.fingerprint() {
                    return Err(RenderError::Invalid(format!("depth image source={asset} phase=source_admission: conflicting original pixels")));
                }
            }
            let source = self.image_sources.get(&asset).ok_or_else(|| {
                RenderError::Invalid(format!("depth image source={asset} is not registered"))
            })?;
            if [source.width, source.height] != image.size() {
                return Err(RenderError::Invalid(format!("depth image source={asset} phase=source_admission: registered dimensions differ")));
            }
        }
        for (asset, image) in images {
            self.upload_owned_depth_image(TextureKey::Static(asset), image, resolution, budget)?;
            self.touch_image(asset);
        }
        // Key video colors by layer instance, not asset: two clips of one asset
        // may refer to different PTS and must never overwrite each other.
        for (object, image) in scene.calibrated_video_images() {
            self.upload_owned_depth_image(TextureKey::Video(object), image, resolution, budget)?;
        }
        Ok(())
    }

    fn upload_owned_depth_image(
        &mut self,
        key: TextureKey,
        image: &crate::DepthImage,
        resolution: Resolution,
        budget: u64,
    ) -> Result<()> {
        let [w, h] = image.size();
        let (rw, rh) = resolution.dimensions(w, h);
        if self.images.get(&key).is_some_and(|i| {
            i.size == (rw, rh)
                && i.plane_image
                    .is_some_and(|state| state.depth_image == Some(image.fingerprint()))
        }) {
            return Ok(());
        }
        decoded_image_bytes(rw, rh, budget)?;
        if (rw, rh) == (w, h) {
            self.upload_pixels(key, w, h, image.rgba(), true)?;
        } else {
            // Same bounded linear-alpha box filter as decoded previews.
            // Account the output plus its row accumulator before allocation.
            let required = u64::from(rw) * u64::from(rh) * 4 + u64::from(rw) * 16;
            if required > budget {
                return Err(RenderError::budget(
                    ResourceBudgetScope::DecodedImages,
                    required,
                    budget,
                    "owned depth image preview exceeds decoded image budget",
                ));
            }
            let Resolution::Preview(edge) = resolution else {
                return Err(RenderError::Invalid(
                    "owned image full resolution dimensions differ".into(),
                ));
            };
            let pixels = preview_rgba(w, h, image.rgba(), edge, &AtomicBool::new(false))?;
            self.upload_pixels(key, rw, rh, &pixels.rgba, false)?;
        }
        let uploaded = self.images.get_mut(&key).ok_or_else(|| {
            RenderError::Invalid(format!(
                "depth image key={key:?} phase=upload: missing revision"
            ))
        })?;
        uploaded.plane_image = Some(PlaneImageState {
            depth_image: Some(image.fingerprint()),
            opaque: image.rgba().chunks_exact(4).all(|p| p[3] == 255),
            frozen: false,
        });
        // Clear any old ordinary-video stamp: returning to the decoder path
        // must not mistake these owned pixels for an earlier codec upload.
        uploaded.video_stamp = None;
        if let TextureKey::Video(object) = key {
            if let Some(video) = &mut self.video_gpu {
                video.remove(object);
            }
            self.invalidate_video_blend(object);
        }
        self.image_upload_bytes = self
            .image_upload_bytes
            .saturating_add(u64::from(rw) * u64::from(rh) * 4);
        Ok(())
    }
}
