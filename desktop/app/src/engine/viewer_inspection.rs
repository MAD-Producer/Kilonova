//! Viewer-only snapshots. They never modify the project or its edit history.
use super::{CompositionStamp, UiFrame};
use motion_host::session::Session;
use motion_render::{Presenter, Renderer, ViewChannel, ViewerSnapshot};
use serde_json::Value;
use std::collections::HashMap;

struct Snapshot {
    generation: u64,
    image: ViewerSnapshot,
}

#[derive(Default)]
pub(super) struct State {
    context: Option<(u64, u64, String)>,
    snapshots: HashMap<u64, Snapshot>,
    next_generation: u64,
    bound: Option<(u64, u64, ViewChannel)>,
}

impl State {
    /// A snapshot from an old document, composition or GPU device is never reused.
    pub(super) fn reconcile(&mut self, session: &mut Session) {
        let context = (session.document_generation, session.surface_epoch, session.engine.project().composition_id.clone());
        if self.context.as_ref() != Some(&context) {
            if let Some(g) = session.graphics.as_mut() { g.presenter.clear_comparison_source(); }
            self.snapshots.clear();
            self.bound = None;
            self.context = Some(context);
        }
    }

    pub(super) fn close_window(&mut self, session: &mut Session, window: u64) {
        if self.bound.is_some_and(|bound| bound.0 == window) {
            if let Some(g) = session.graphics.as_mut() { g.presenter.clear_comparison_source(); }
            self.bound = None;
        }
        self.snapshots.remove(&window);
    }

    pub(super) fn capture(&mut self, session: &mut Session, stamp: Option<&CompositionStamp>, window: u64) -> Result<Value, String> {
        self.reconcile(session);
        let stamp = stamp.ok_or("no completed viewer frame is available")?;
        if stamp.revision != session.engine.revision() || stamp.view != session.view_revision
            || stamp.surface != session.surface_epoch || stamp.frame != session.frame.to_bits()
            || stamp.mode != session.preview.mode || stamp.tier != session.preview.tier()
            || stamp.quality != session.preview_memory.quality(session.preview.mode == motion_render::PreviewMode::High)
            || session.preview_memory_held || session.last_error.is_some() {
            return Err("the completed viewer frame is no longer current".into());
        }
        let generation = self.next_generation.checked_add(1).ok_or("viewer snapshot generation exhausted")?;
        let graphics = session.graphics.as_mut().ok_or("desktop surface is not attached")?;
        let source = graphics.scratch.working_front().ok_or("the working viewer frame is unavailable")?;
        let image = ViewerSnapshot::capture(&graphics.renderer, source).map_err(|error| error.to_string())?;
        let width = image.native().width;
        let height = image.native().height;
        if self.bound.is_some_and(|bound| bound.0 == window) {
            graphics.presenter.clear_comparison_source();
            self.bound = None;
        }
        self.snapshots.insert(window, Snapshot { generation, image });
        self.next_generation = generation;
        Ok(serde_json::json!({"window": window, "generation": generation, "width": width, "height": height}))
    }

    fn source(&self, ui: &UiFrame) -> Result<Option<(&Snapshot, &motion_render::RenderTarget)>, String> {
        if ui.inspection.comparison_split.is_none() { return Ok(None); }
        let snapshot = self.snapshots.get(&ui.window).ok_or("viewer comparison snapshot is unavailable")?;
        let target = if ui.inspection.channel == ViewChannel::Color { snapshot.image.display() } else { snapshot.image.native() };
        Ok(Some((snapshot, target)))
    }

    pub(super) fn configure(&mut self, session: &mut Session, ui: &UiFrame) -> Result<(), String> {
        self.reconcile(session);
        let selection = if ui.composition.is_some() { self.source(ui)? } else { None };
        let desired = selection.as_ref().map(|(snapshot, _)| (ui.window, snapshot.generation, ui.inspection.channel));
        let graphics = session.graphics.as_mut().ok_or("desktop surface is not attached")?;
        if desired != self.bound {
            if let Some((_, target)) = selection {
                graphics.presenter.set_comparison_source(&graphics.renderer.device, target);
            } else {
                graphics.presenter.clear_comparison_source();
            }
            self.bound = desired;
        }
        graphics.presenter.set_inspection_geometry(
            if ui.composition.is_some() { ui.inspection.region } else { None },
            if ui.composition.is_some() { ui.inspection.comparison_split } else { None },
        );
        Ok(())
    }

    pub(super) fn configure_capture(&self, ui: &UiFrame, renderer: &Renderer, presenter: &mut Presenter) -> Result<(), String> {
        if ui.composition.is_some() {
            if let Some((_, target)) = self.source(ui)? { presenter.set_comparison_source(&renderer.device, target); }
            presenter.set_inspection_geometry(ui.inspection.region, ui.inspection.comparison_split);
        }
        Ok(())
    }
}
