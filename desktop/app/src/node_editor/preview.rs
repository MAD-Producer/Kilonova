//! A preview target is view state; it never edits the document or export output.
use super::*;

impl NodeEditor {
    pub fn preview_target(&self) -> Option<&PortRef> {
        self.preview_target.as_ref()
    }
    pub fn toggle_preview(&mut self, target: PortRef) -> Result<bool, Diagnostic> {
        self.port_type(&target, true)?;
        if self.preview_target.as_ref() == Some(&target) {
            self.preview_target = None;
        } else {
            self.preview_target = Some(target);
        }
        self.advance_preview();
        self.notice = None;
        Ok(true)
    }
    pub fn restore_preview(&mut self) -> bool {
        if self.preview_target.take().is_none() {
            return false;
        }
        self.advance_preview();
        self.notice = None;
        true
    }
    pub(super) fn advance_preview(&mut self) {
        if self
            .preview_target
            .as_ref()
            .is_some_and(|p| self.port_type(p, true).is_err())
        {
            self.preview_target = None;
        }
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.pending_compile = None;
    }
}
