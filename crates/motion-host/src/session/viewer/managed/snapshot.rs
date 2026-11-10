//! An explicit cold copy of an exactly current managed cell.
use super::*;

impl Session {
    pub fn capture_managed_view_cell(
        &self,
        owner: u64,
        index: usize,
    ) -> Result<motion_render::ManagedDisplaySnapshot> {
        self.check_thread()?;
        let result: Result<_> = (|| {
            if self.colors.is_none() {
                return Err(refuse("surface_snapshot", "managed document is unavailable").into());
            }
            let g = self
                .graphics
                .as_ref()
                .ok_or_else(|| refuse("surface_snapshot", "graphics are detached"))?;
            let state =
                g.views.colors.as_ref().ok_or_else(|| {
                    refuse("surface_snapshot", "prepared display owner is missing")
                })?;
            let key = state
                .published()
                .ok_or_else(|| refuse("surface_snapshot", "completed group is unavailable"))?;
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
            {
                return Err(
                    refuse("surface_snapshot", "completed group is no longer current").into(),
                );
            }
            Ok(state.snapshot(&g.renderer, index)?)
        })();
        result.map_err(|cause|crate::Error::with_context(format!(
            "managed snapshot phase=surface_snapshot document={} revision={} owner={owner} cell={index} frame={}",
            self.document_generation,self.engine.revision(),self.frame),cause))
    }
}
