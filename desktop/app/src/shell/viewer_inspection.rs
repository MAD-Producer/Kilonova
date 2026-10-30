//! Asynchronous viewer-only snapshot actions.
use crate::{panels::viewer_inspection::{identity, Identity}, Shell};
use serde_json::Value;
use std::sync::mpsc::{Receiver, TryRecvError};

pub(crate) struct Pending {
    window: u64,
    floating: bool,
    source: Identity,
    capture: bool,
    receive: Receiver<Result<Value, String>>,
}

impl Shell {
    fn viewer_window(&self, floating: bool) -> Result<u64, String> {
        if !floating { return Ok(0); }
        self.floating.values().find(|f| f.panel == motion_ui::dock::Panel::Composition)
            .map(|f| f.key).ok_or_else(|| "floating viewer is no longer open".into())
    }

    pub(crate) fn capture_viewer_inspection(&mut self, floating: bool) -> Result<(), String> {
        let source = identity(&self.state).ok_or("viewer document identity is unavailable")?;
        let window = self.viewer_window(floating)?;
        if self.viewer_pending.iter().any(|pending| pending.window == window) {
            return Err("a viewer snapshot operation is still in progress".into());
        }
        let receive = self.engine.capture_viewer_snapshot_async(window)?;
        self.panels.viewer_inspection.set_busy(floating, true);
        self.viewer_pending.push(Pending { window, floating, source, capture: true, receive });
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn save_viewer_inspection(&mut self, floating: bool) -> Result<(), String> {
        let source = identity(&self.state).ok_or("viewer document identity is unavailable")?;
        let window = self.viewer_window(floating)?;
        if self.viewer_pending.iter().any(|pending| pending.window == window) {
            return Err("a viewer snapshot operation is still in progress".into());
        }
        let Some(mut path) = rfd::FileDialog::new().add_filter("PNG", &["png"])
            .set_file_name("snapshot.png").save_file() else { return Ok(()) };
        if path.extension().is_none() { path.set_extension("png"); }
        let receive = self.engine.save_viewer_snapshot_async(window, path, self.panels.viewer_guides.channel(floating))?;
        self.panels.viewer_inspection.set_busy(floating, true);
        self.viewer_pending.push(Pending { window, floating, source, capture: false, receive });
        self.dirty = true;
        Ok(())
    }

    pub(crate) fn poll_viewer_inspection(&mut self) {
        self.panels.viewer_inspection.synchronize(&self.state);
        let mut completed = Vec::new();
        self.viewer_pending.retain(|pending| match pending.receive.try_recv() {
            Ok(result) => { completed.push((pending.window, pending.floating, pending.source.clone(), pending.capture, result)); false }
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => {
                completed.push((pending.window, pending.floating, pending.source.clone(), pending.capture,
                    Err("viewer snapshot worker disconnected".into()))); false
            }
        });
        for (window, floating, source, capture, result) in completed {
            if self.viewer_window(floating).ok() != Some(window) { continue; }
            if capture { self.panels.viewer_inspection.capture_done(floating, &source, result.is_ok()); }
            else { self.panels.viewer_inspection.save_done(floating, &source); }
            if identity(&self.state).as_ref() != Some(&source) { continue; }
            match result {
                Ok(_) => self.panels.message = Some(if self.workspace.locale == crate::i18n::Locale::Zh {
                    if capture { "快照已捕获" } else { "快照已保存" }
                } else if capture { "Snapshot captured" } else { "Snapshot saved" }.into()),
                Err(error) => {
                    log::error!("viewer snapshot action failed: operation={} window={} reason={}",
                        if capture { "capture" } else { "save" }, window, error.chars().take(512).collect::<String>());
                    self.panels.message = Some(self.workspace.locale.error(&error));
                }
            }
            self.dirty = true;
        }
    }
}
