//! Project imports register sources without implicitly creating layers. Media
//! prepare/status/publication all enter the existing observable engine queue.
use crate::*;
use std::sync::mpsc::{Receiver, TryRecvError};

pub(super) fn media_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|extension| {
            [
                "png", "jpg", "jpeg", "mp4", "mov", "mkv", "webm", "3gp", "m4a", "aac", "mp3",
                "wav", "flac", "ogg", "opus", "aiff",
            ]
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Prepare,
    Status,
}
pub(crate) struct Import {
    source: crate::layer_clipboard::KeyframeSource,
    folder: Option<u64>,
    request: String,
    pending: Option<(Phase, Receiver<Result<Value, String>>)>,
    last: Instant,
}
pub(crate) struct ImageImport {
    source: crate::layer_clipboard::KeyframeSource,
    folder: Option<u64>,
    job: workflow::library_image::Task,
}
impl Import {
    fn queue(&mut self, engine: &engine::Engine, phase: Phase, value: Value) -> Result<(), String> {
        let text = value.to_string();
        self.pending = Some((
            phase,
            engine.request(|reply| engine::Command::Media { text, reply })?,
        ));
        Ok(())
    }
    fn release(&self, engine: &engine::Engine, cancel: bool) -> Result<(), String> {
        let mut primary = None;
        for op in if cancel {
            &["cancel_media_import", "release_media_task"][..]
        } else {
            &["release_media_task"][..]
        } {
            let text = serde_json::json!({"op":op,"request_id":self.request}).to_string();
            // Task replies after an explicit release are best effort; actual
            // worker failures retain the normal file/stderr error channel.
            if let Err(cause) = engine.request(|reply| engine::Command::Media { text, reply }) {
                log::error!("project import cleanup dispatch failed request={} operation={op} cause={cause}", self.request);
                if primary.is_none() {
                    primary = Some(cause);
                }
            }
        }
        primary.map_or(Ok(()), Err)
    }
}
impl Shell {
    pub(crate) fn project_import_folder(&self) -> Option<u64> {
        let view = &self.panels.project.browser.view;
        (self.state["root"].as_str() == Some(view.root.as_str()))
            .then_some(view.folder)
            .flatten()
    }
    pub(crate) fn main_project_media_drop(&mut self, path: &std::path::Path) -> bool {
        if self.home.is_some() || !media_path(path) {
            return false;
        }
        let Some(size) = self.size() else {
            return false;
        };
        let Some(frame) = panels::frames(&self.workspace.root, size)
            .into_iter()
            .find(|frame| frame.panel == dock::Panel::Project)
        else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };
        let position = self.pointer_position.dropped(window, self.scale);
        self.drop_project_media(path.to_owned(), position, frame.content)
    }
    pub(crate) fn drop_project_media(
        &mut self,
        path: PathBuf,
        position: std::io::Result<[f32; 2]>,
        bounds: motion_ui::Rect,
    ) -> bool {
        if !media_path(&path) {
            return false;
        }
        let point = match position {
            Ok(point) => point,
            Err(cause) => {
                log::error!("project media drop failed phase=client_position cause={cause}");
                self.project_import_message(
                    "无法确定素材拖入位置",
                    "Cannot determine media drop position",
                );
                return true;
            }
        };
        if !bounds.contains(point) {
            return false;
        }
        self.actions.push_back(panels::Action::ProjectImport {
            source: crate::layer_clipboard::KeyframeSource::of(&self.state),
            folder: self.project_import_folder(),
            path: Some(path),
        });
        true
    }
    fn project_import_message(&mut self, zh: &str, en: &str) {
        self.panels.message = Some(
            if self.workspace.locale == Locale::Zh {
                zh
            } else {
                en
            }
            .into(),
        );
        self.dirty = true;
    }
    pub(crate) fn start_project_import(
        &mut self,
        source: crate::layer_clipboard::KeyframeSource,
        folder: Option<u64>,
        path: Option<PathBuf>,
    ) -> Result<(), String> {
        if source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
            self.project_import_message(
                "工程已变化，已取消导入",
                "Project changed; import cancelled",
            );
            return Ok(());
        }
        if self.library_import.is_some()
            || self.project_image.is_some()
            || self.importing_image.is_some()
            || self.importing_media.is_some()
        {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        let path = match path {
            Some(path) => path,
            None => {
                log::info!(
                    "project import picker start folder={folder:?} revision={} phase=select",
                    self.state["revision"]
                );
                let Some(path) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::ImportMedia))
                    .add_filter(
                        "Media",
                        &[
                            "png", "jpg", "jpeg", "mp4", "mov", "mkv", "webm", "3gp", "m4a", "aac",
                            "mp3", "wav", "flac", "ogg", "opus", "aiff",
                        ],
                    )
                    .pick_file()
                else {
                    self.project_import_message("已取消导入", "Import cancelled");
                    return Ok(());
                };
                path
            }
        };
        self.stop();
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if matches!(extension.as_str(), "png" | "jpg" | "jpeg") {
            let root = PathBuf::from(self.state["root"].as_str().ok_or("project root missing")?);
            let job=workflow::library_image::Task::start(&self.project_image_pool,root,path).map_err(|cause|{
                log::error!("project image dispatch failed operation=prepare_library_image folder={folder:?} cause={cause}");cause
            })?;
            self.project_image = Some(ImageImport {
                source,
                folder,
                job,
            });
        } else {
            let kind = if matches!(extension.as_str(), "mp4" | "mov" | "mkv" | "webm" | "3gp") {
                "video"
            } else {
                "audio"
            };
            let request = format!(
                "project-import-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            let mut import = Import {
                source,
                folder,
                request,
                pending: None,
                last: Instant::now() - Duration::from_secs(1),
            };
            let value = serde_json::json!({"op":"import_media","request_id":import.request,
                "path":path,"kind":kind,"at_frame":0,"name":path.file_stem().and_then(|s|s.to_str()).unwrap_or("Media")});
            import.queue(&self.engine, Phase::Prepare, value)?;
            self.library_import = Some(import);
        }
        self.project_import_message("正在导入到项目…", "Importing into project…");
        Ok(())
    }
    pub(crate) fn cancel_project_import(&mut self) -> Result<(), String> {
        if self.project_image.take().is_some() {
            self.project_import_message(
                "已取消导入，工程保持不变",
                "Import cancelled; project retained",
            );
            return Ok(());
        }
        let Some(import) = self.library_import.take() else {
            return Ok(());
        };
        import.release(&self.engine, true)?;
        self.project_import_message(
            "已取消导入，工程保持不变",
            "Import cancelled; project retained",
        );
        Ok(())
    }
    pub(crate) fn poll_project_import(&mut self) -> Result<(), String> {
        if self.editing.is_some() {
            return Ok(());
        }
        if let Some(image) = self.project_image.take() {
            if image.source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
                self.project_import_message(
                    "工程已变化，已取消导入",
                    "Project changed; import cancelled",
                );
                return Ok(());
            }
            let result = image.job.poll().map_err(|cause| {
                log::error!(
                    "project image reply failed request={} phase={} cause={cause}",
                    image.job.request(),
                    image.job.phase()
                );
                cause
            })?;
            match result {
                None => self.project_image = Some(image),
                Some(Err(cause)) => return Err(cause),
                Some(Ok(None)) => self.project_import_message(
                    "已取消导入，工程保持不变",
                    "Import cancelled; project retained",
                ),
                Some(Ok(Some(prepared))) => {
                    let operation = workflow::library_media::Request::PreparedImage {
                        prepared,
                        folder: image.folder,
                    };
                    let source = image.source.clone();
                    self.editing = Some((
                        Instant::now(),
                        self.engine.request(|reply| engine::Command::ProjectAsset {
                            source,
                            operation,
                            reply,
                        })?,
                    ));
                }
            }
            return Ok(());
        }
        let Some(mut import) = self.library_import.take() else {
            return Ok(());
        };
        if import.source != crate::layer_clipboard::KeyframeSource::of(&self.state) {
            import.release(&self.engine, true)?;
            self.project_import_message(
                "工程已变化，已取消导入",
                "Project changed; import cancelled",
            );
            return Ok(());
        }
        let result = self.poll_project_import_inner(&mut import);
        match result {
            Ok(false) => self.library_import = Some(import),
            Ok(true) => {}
            Err(cause) => {
                if let Err(cleanup) = import.release(&self.engine, true) {
                    log::error!(
                        "project import cleanup failed request={} primary={cause} cause={cleanup}",
                        import.request
                    );
                }
                log::error!(
                    "project import failed request={} folder={:?} phase={:?} cause={cause}",
                    import.request,
                    import.folder,
                    import.pending.as_ref().map(|p| p.0)
                );
                return Err(cause);
            }
        }
        Ok(())
    }
    fn poll_project_import_inner(&mut self, import: &mut Import) -> Result<bool, String> {
        let Some((phase, receiver)) = &import.pending else {
            if import.last.elapsed() >= Duration::from_millis(100) {
                import.last = Instant::now();
                import.queue(
                    &self.engine,
                    Phase::Status,
                    serde_json::json!({"op":"media_status","request_id":import.request}),
                )?;
            }
            return Ok(false);
        };
        let phase = *phase;
        let value = match receiver.try_recv() {
            Ok(value) => value?,
            Err(TryRecvError::Empty) => return Ok(false),
            Err(TryRecvError::Disconnected) => {
                return Err(format!(
                    "project import worker disconnected request={} phase={phase:?}",
                    import.request
                ))
            }
        };
        import.pending = None;
        match super::media::checked_task_state(&value, &import.request)? {
            "running" => {
                self.panels.message = Some(format!(
                    "{} {:.0}%",
                    self.workspace.locale.t(T::Importing),
                    value["progress"].as_f64().unwrap_or(0.) * 100.
                ));
                self.dirty = true;
                Ok(false)
            }
            "ready" => {
                let source = import.source.clone();
                let operation = workflow::library_media::Request::Media {
                    request: import.request.clone(),
                    folder: import.folder,
                };
                self.editing = Some((
                    Instant::now(),
                    self.engine.request(|reply| engine::Command::ProjectAsset {
                        source,
                        operation,
                        reply,
                    })?,
                ));
                // Publication is queued before release; cancellation cannot
                // retract a transaction that has already entered the edit queue.
                if let Err(cause) = import.release(&self.engine, false) {
                    log::error!("project import release dispatch failed after publication request={} cause={cause}", import.request);
                }
                Ok(true)
            }
            "cancelled" => {
                import.release(&self.engine, false)?;
                self.project_import_message(
                    "已取消导入，工程保持不变",
                    "Import cancelled; project retained",
                );
                Ok(true)
            }
            "failed" => Err(value["error"]
                .as_str()
                .unwrap_or("project import failed without a cause")
                .into()),
            state => Err(format!(
                "unexpected project import state request={} phase={phase:?} state={state}",
                import.request
            )),
        }
    }
}

#[cfg(test)]
mod tests;
