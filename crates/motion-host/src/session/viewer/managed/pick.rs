//! One explicit pick from a complete managed working image, before display.
use super::*;

impl Session {
    pub(in crate::session::viewer) fn sample_color_view_cell(
        &self,
        owner: u64,
        index: usize,
        camera: CameraKey,
        point: [f64; 2],
    ) -> Result<Option<[f32; 4]>> {
        let result: Result<_> = (|| {
            let Some(g) = self.graphics.as_ref() else {
                return Ok(None);
            };
            let Some(prepared) = self.colors.as_ref() else {
                return Ok(None);
            };
            let Some(state) = g
                .views
                .colors
                .as_ref()
                .filter(|state| state.matches(prepared, self.engine.project()))
            else {
                return Ok(None);
            };
            let Some(key) = state.published() else {
                return Ok(None);
            };
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
                || key.cameras.get(index) != Some(&camera)
            {
                return Ok(None);
            }
            let Some((contract, rgba)) = state.sample_working_pixel(&g.renderer, index, point)?
            else {
                return Ok(None);
            };
            prepared
                .authored
                .working_color_to_authored(prepared.colors.config(), contract, rgba)
                .map(Some)
                .map_err(RenderError::from)
                .map_err(crate::Error::from)
        })();
        result.map_err(|cause| crate::Error::with_context(format!(
            "managed color pick document={} revision={} owner={owner} cell={index} frame={} phase=readback_to_authored",
            self.document_generation, self.engine.revision(), self.frame), cause))
    }
}
