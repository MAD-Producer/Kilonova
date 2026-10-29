//! Headless source validation. This is also the observable operation boundary
//! for the authoring document; it does not start a GPU, host session or window.
#[cfg(test)]
mod tests;
use crate::fallback_observation::{recovered, short, Counter};
use motion_graph::plugin_project::{
    CompiledLogic, Error, Phase, PluginLogic, PluginProject, Result,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::Instant,
};

// Lifetime lock-recovery evidence at this fixed site; healthy checks do not
// reset poison recoveries. The shared helper owns suppression/threshold policy.
static FLIGHT_LOCK: Counter = Counter::new();

#[derive(Clone, Debug)]
struct Flight {
    request: u64,
    source: String,
    phase: &'static str,
    began: Instant,
}
#[derive(Default)]
struct Checker {
    next: AtomicU64,
    flight: Mutex<Option<Flight>>,
}
#[derive(Debug, Serialize)]
pub struct Report {
    ok: bool,
    logic: &'static str,
    parameters: usize,
    layout_components: usize,
    shader_bytes: usize,
}

pub fn run(path: &Path) -> Result<Report> {
    Checker::default().check(path)
}
pub fn load_compiled(path: &Path) -> Result<(PluginProject, CompiledLogic)> {
    Checker::default().prepare_with(
        path,
        || PluginProject::load(path),
        PluginProject::compile_logic,
    )
}
impl Checker {
    fn check(&self, path: &Path) -> Result<Report> {
        self.check_with(
            path,
            || PluginProject::load(path),
            PluginProject::compile_logic,
        )
    }
    fn current(&self) -> Option<Flight> {
        recovered(
            &self.flight,
            &FLIGHT_LOCK,
            "plugin.check.flight",
            |flight| flight.clone(),
        )
    }
    fn check_with(
        &self,
        path: &Path,
        read: impl FnOnce() -> Result<PluginProject>,
        compile: impl FnOnce(&PluginProject) -> Result<CompiledLogic>,
    ) -> Result<Report> {
        let (project, compiled) = self.prepare_with(path, read, compile)?;
        fn components(node: &motion_effects::native_layout::LayoutNode) -> usize {
            let tabs = match &node.content {
                motion_effects::native_layout::LayoutContent::Tabs { tabs } => tabs.len(),
                _ => 0,
            };
            1 + tabs + node.children().map(components).sum::<usize>()
        }
        Ok(Report {
            ok: true,
            logic: match project.logic {
                PluginLogic::Graph { .. } => "graph",
                PluginLogic::Wgsl { .. } => "wgsl",
            },
            parameters: compiled.manifest().effects[0].params.len(),
            layout_components: components(&project.interface.root),
            shader_bytes: compiled.source().len(),
        })
    }
    fn prepare_with(
        &self,
        path: &Path,
        read: impl FnOnce() -> Result<PluginProject>,
        compile: impl FnOnce(&PluginProject) -> Result<CompiledLogic>,
    ) -> Result<(PluginProject, CompiledLogic)> {
        let flight = Flight {
            request: self.next.fetch_add(1, Ordering::Relaxed) + 1,
            source: short(path.display()).to_string(),
            phase: "read",
            began: Instant::now(),
        };
        let admitted = recovered(&self.flight, &FLIGHT_LOCK, "plugin.check.flight", |slot| {
            if slot.is_some() {
                return false;
            }
            *slot = Some(flight.clone());
            true
        });
        if !admitted {
            let error = Error::Invalid {
                phase: Phase::Read,
                field: "operation",
                message: "another plugin source check is still running",
            };
            log::error!(
                "plugin.check error request={} source={} phase=admission cause={error}",
                flight.request,
                flight.source
            );
            return Err(error);
        }
        let mut scope = Scope {
            checker: self,
            flight,
            completed: false,
        };
        scope.start_record();
        let result = (|| {
            let project = read()?;
            scope.phase("compile");
            let compiled = compile(&project)?;
            Ok((project, compiled))
        })();
        match &result {
            Ok(_) => log::debug!("plugin.check done request={} source={} elapsed_ms={}",
                scope.flight.request, scope.flight.source, scope.flight.began.elapsed().as_millis()),
            Err(error) => log::error!("plugin.check error request={} source={} operation_phase={} elapsed_ms={} cause={error}",
                scope.flight.request, scope.flight.source, scope.flight.phase, scope.flight.began.elapsed().as_millis()),
        }
        scope.completed = true;
        result
    }
}
struct Scope<'a> {
    checker: &'a Checker,
    flight: Flight,
    completed: bool,
}
impl Scope<'_> {
    fn start_record(&self) {
        // Publication and durable default-level start precede filesystem or
        // compiler entry. Logging occurs after releasing the flight mutex.
        let published = self.checker.current();
        log::info!(
            "plugin.check start request={} source={} phase={} in_flight={}",
            self.flight.request,
            self.flight.source,
            self.flight.phase,
            published
                .as_ref()
                .is_some_and(|flight| flight.request == self.flight.request)
        );
        crate::logging::flush();
    }
    fn phase(&mut self, phase: &'static str) {
        self.flight.phase = phase;
        recovered(
            &self.checker.flight,
            &FLIGHT_LOCK,
            "plugin.check.flight",
            |slot| {
                *slot = Some(self.flight.clone());
            },
        );
        self.start_record();
    }
}
impl Drop for Scope<'_> {
    fn drop(&mut self) {
        recovered(
            &self.checker.flight,
            &FLIGHT_LOCK,
            "plugin.check.flight",
            |slot| {
                *slot = None;
            },
        );
        if !self.completed {
            log::error!(
                "plugin.check aborted request={} source={} phase={} elapsed_ms={} cause={}",
                self.flight.request,
                self.flight.source,
                self.flight.phase,
                self.flight.began.elapsed().as_millis(),
                if std::thread::panicking() {
                    "panic; original cause recorded by process panic hook"
                } else {
                    "operation scope exited without a result"
                }
            );
        }
    }
}
