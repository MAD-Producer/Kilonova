//! Package installation stays on the engine worker; the UI polls replies.
use crate::*;
use std::sync::mpsc::{Receiver, TryRecvError};
pub struct Task {
    path: PathBuf,
    receiver: Receiver<Result<Value, String>>,
    catalogue: bool,
}
pub enum Progress {
    Pending,
    LoadingCatalogue,
    Ready(Value),
    Failed(String),
}
impl Task {
    pub fn start(engine: &Engine, path: PathBuf) -> Result<Self, String> {
        if !path
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("msfx"))
        {
            return Err("Choose an .msfx effect package".into());
        }
        let receiver =
            engine.plugin_async(serde_json::json!({"op":"install","path":path}).to_string())?;
        Ok(Self {
            path,
            receiver,
            catalogue: false,
        })
    }
    pub fn poll(&mut self, engine: &Engine) -> Progress {
        match self.receiver.try_recv() {
            Err(TryRecvError::Empty) => Progress::Pending,
            Err(TryRecvError::Disconnected) => {
                self.failed("Effect package import worker disconnected".into())
            }
            Ok(Err(error)) => self.failed(error),
            Ok(Ok(value)) if self.catalogue => Progress::Ready(value),
            Ok(Ok(_)) => {
                log::info!("effect package installed: path={}", self.path.display());
                crate::logging::flush();
                // Installation has already committed. Retain this phase even if
                // dispatching or receiving the catalogue refresh fails.
                self.catalogue = true;
                match engine.plugin_async("{\"op\":\"catalogue\"}".into()) {
                    Ok(receiver) => {
                        self.receiver = receiver;
                        Progress::LoadingCatalogue
                    }
                    Err(error) => self.failed(error),
                }
            }
        }
    }
    fn failed(&self, error: String) -> Progress {
        log::error!(
            "effect package import failed: path={} installed={} phase={} reason={error}",
            self.path.display(),
            self.catalogue,
            if self.catalogue {
                "catalogue"
            } else {
                "install"
            }
        );
        Progress::Failed(error)
    }
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into()
    }
}
impl Shell {
    pub(crate) fn start_effect_import(&mut self, path: PathBuf) -> Result<(), String> {
        if self.importing_effect.is_some() {
            return Err(if self.workspace.locale == Locale::Zh {
                "正在导入效果包"
            } else {
                "An effect package is being imported"
            }
            .into());
        }
        self.importing_effect = Some(Task::start(&self.engine, path)?);
        self.panels.browser.importing_package = true;
        self.panels.effect_controls.importing_package = true;
        self.panels.message = Some(
            if self.workspace.locale == Locale::Zh {
                "正在导入效果包…"
            } else {
                "Importing effect package…"
            }
            .into(),
        );
        self.dirty = true;
        self.redraw();
        Ok(())
    }
    pub(crate) fn poll_effect_import(&mut self) {
        let Some(task) = &mut self.importing_effect else {
            return;
        };
        match task.poll(&self.engine) {
            Progress::Pending | Progress::LoadingCatalogue => {}
            Progress::Ready(catalogue) => {
                let name = task.name();
                let catalogue = std::sync::Arc::new(catalogue);
                self.panels.browser.refresh_catalogue(catalogue.clone());
                self.panels.effect_controls.refresh_catalogue(catalogue);
                self.importing_effect = None;
                self.actions.push_back(panels::Action::RefreshState);
                self.panels.browser.importing_package = false;
                self.panels.effect_controls.importing_package = false;
                self.panels.message = Some(format!(
                    "{}: {name}",
                    if self.workspace.locale == Locale::Zh {
                        "效果包已导入"
                    } else {
                        "Effect package imported"
                    }
                ));
                self.dirty = true;
                self.redraw();
            }
            Progress::Failed(error) => {
                self.importing_effect = None;
                self.panels.browser.importing_package = false;
                self.panels.effect_controls.importing_package = false;
                self.panels.message = Some(format!(
                    "{}: {}",
                    if self.workspace.locale == Locale::Zh {
                        "效果包导入失败"
                    } else {
                        "Effect package import failed"
                    },
                    self.workspace.locale.error(&error)
                ));
                self.dirty = true;
                self.redraw();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package(folder: &std::path::Path) -> PathBuf {
        let original = motion_effects::builtin::package().unwrap();
        let mut manifest = original.manifest.clone();
        manifest.id = "test.effect.library".into();
        manifest.name = "Import Test".into();
        manifest.effects.retain(|e| e.id == "glow");
        manifest.effects[0].category = "导入分类".into();
        let source = folder.join("source");
        std::fs::create_dir_all(&source).unwrap();
        for (name, data) in &original.files {
            let path = source.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, data).unwrap();
        }
        std::fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let path = folder.join("效果包.msfx");
        std::fs::write(&path, motion_effects::package_directory(&source).unwrap()).unwrap();
        path
    }
    #[test]
    fn real_package_import_refreshes_catalogue_and_bad_archive_keeps_previous_registry() {
        let folder = tempfile::tempdir().unwrap();
        let path = package(folder.path());
        let engine = Engine::start(
            folder.path().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap()["project"].clone();
        let mut task = Task::start(&engine, path).unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let catalogue = loop {
            assert!(Instant::now() < deadline);
            match task.poll(&engine) {
                Progress::Ready(v) => break v,
                Progress::Failed(e) => panic!("{e}"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        assert!(catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["manifest"]["id"] == "test.effect.library"
                && p["manifest"]["effects"][0]["category"] == "导入分类"));
        assert_eq!(engine.state().unwrap()["project"], before);
        let invalid = folder.path().join("invalid.msfx");
        std::fs::write(&invalid, b"not an effect archive").unwrap();
        let mut task = Task::start(&engine, invalid).unwrap();
        loop {
            assert!(Instant::now() < deadline);
            match task.poll(&engine) {
                Progress::Failed(_) => break,
                Progress::Ready(_) => panic!("invalid archive accepted"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        assert_eq!(
            engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap()["packages"],
            catalogue["packages"]
        );
        assert_eq!(engine.state().unwrap()["project"], before);
    }

    #[test]
    fn installed_package_is_logged_before_a_failed_catalogue_refresh() {
        const CHILD: &str = "MOTION_EFFECT_REFRESH_TEST_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let _logger = crate::logging::init(root.join("logs")).unwrap();
            let path = package(&root);
            let project = root.join("project");
            let engine =
                Engine::start(project.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
            let mut task = Task::start(&engine, path).unwrap();
            // The serial State request proves that the installation committed;
            // leave its successful reply unpolled until the worker is stopped.
            engine.state().unwrap();
            let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
            assert!(catalogue["packages"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["manifest"]["id"] == "test.effect.library"));
            let panic = engine
                .request(|reply| crate::engine::Command::TestPanic {
                    during_prepare: false,
                    reply,
                })
                .unwrap();
            assert!(panic.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
            assert!(matches!(task.poll(&engine), Progress::Failed(_)));
            drop(engine);
            let reopened =
                Engine::start(project, motion_desktop_media::platform(), 8 << 30).unwrap();
            assert!(
                reopened.plugin("{\"op\":\"catalogue\"}".into()).unwrap()["packages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["manifest"]["id"] == "test.effect.library")
            );
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "effect_import::tests::installed_package_is_logged_before_a_failed_catalogue_refresh"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        let installed = text.find("effect package installed: path=").unwrap();
        let failed = text.find("installed=true phase=catalogue reason=").unwrap();
        assert!(installed < failed);
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("installed=true phase=catalogue reason="));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("effect package installed:"));
    }
}
