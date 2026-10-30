//! Inspectable source/decode flight. A native call that does not return retains
//! its phase and start time; a panic clears the flight after recording abortion.
use std::{
    path::Path,
    sync::atomic::{AtomicU8, Ordering},
    time::Instant,
};
pub(super) struct Preparation {
    pub phase: AtomicU8,
    pub began: Instant,
    source: String,
}
impl Preparation {
    pub fn new(path: &Path) -> Self {
        Self {
            phase: AtomicU8::new(0),
            began: Instant::now(),
            source: crate::fallback_observation::short(path.display()).to_string(),
        }
    }
    fn enter(&self, phase: u8, name: &str) {
        self.phase.store(phase, Ordering::Release);
        log::info!(
            "plugin.preview start request=1 phase={name} in_flight=true source={} elapsed_ms={}",
            self.source,
            self.began.elapsed().as_millis()
        );
        crate::logging::flush();
    }
    pub fn prepare(&self, path: &Path) -> Result<super::window::Prepared, String> {
        self.prepare_with(path, crate::node_preview::PreparedPlugin::new)
    }
    pub fn prepare_with(
        &self,
        path: &Path,
        decode: impl FnOnce(
            motion_graph::plugin_project::CompiledLogic,
        )
            -> Result<crate::node_preview::PreparedPlugin, motion_render::RenderError>,
    ) -> Result<super::window::Prepared, String> {
        struct Scope<'a>(&'a Preparation, bool);
        impl Drop for Scope<'_> {
            fn drop(&mut self) {
                let phase = self.0.phase.swap(0, Ordering::AcqRel);
                if !self.1 {
                    log::error!("plugin.preview aborted request=1 phase={phase} source={} elapsed_ms={} cause=panic; original cause recorded by process panic hook",self.0.source,self.0.began.elapsed().as_millis());
                }
            }
        }
        let mut scope = Scope(self, false);
        self.enter(1, "read_compile");
        let result = (|| {
            let (project, compiled) =
                crate::plugin_check::load_compiled(path).map_err(|e| e.to_string())?;
            self.enter(2, "resource_decode");
            let prepared = decode(compiled).map_err(|e| format!("plugin resources: {e}"))?;
            Ok((project, prepared))
        })();
        if let Err(cause) = &result {
            log::error!(
                "plugin.preview error request=1 phase={} source={} elapsed_ms={} cause={}",
                self.phase.load(Ordering::Acquire),
                self.source,
                self.began.elapsed().as_millis(),
                crate::fallback_observation::error_cause(cause)
            );
        }
        log::debug!(
            "plugin.preview done request=1 source={} elapsed_ms={}",
            self.source,
            self.began.elapsed().as_millis()
        );
        scope.1 = true;
        result
    }
}
