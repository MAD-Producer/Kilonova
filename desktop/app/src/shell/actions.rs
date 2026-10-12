use crate::*;

// Absolute seeks can replace consecutive pending seeks. Never cross an edit,
// save or transaction boundary, and never drop relative numeric edits.
pub(crate) fn pop_action(
    queue: &mut std::collections::VecDeque<panels::Action>,
) -> Option<panels::Action> {
    let mut action = queue.pop_front()?;
    if matches!(action, panels::Action::Seek(_)) {
        while matches!(queue.front(), Some(panels::Action::Seek(_))) {
            action = queue.pop_front().unwrap();
        }
    }
    Some(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seek_coalescing_preserves_edit_save_and_history_boundaries() {
        use panels::Action;
        let mut queue = std::collections::VecDeque::from([
            Action::Seek(1.),
            Action::Seek(2.),
            Action::History(2),
            Action::Seek(3.),
            Action::Seek(4.),
            Action::Save,
            Action::Seek(5.),
            Action::History(3),
        ]);
        assert!(matches!(pop_action(&mut queue), Some(Action::Seek(2.))));
        assert!(matches!(pop_action(&mut queue), Some(Action::History(2))));
        assert!(matches!(pop_action(&mut queue), Some(Action::Seek(4.))));
        assert!(matches!(pop_action(&mut queue), Some(Action::Save)));
        assert!(matches!(pop_action(&mut queue), Some(Action::Seek(5.))));
        assert!(matches!(pop_action(&mut queue), Some(Action::History(3))));
        assert!(pop_action(&mut queue).is_none());
    }
}

impl Shell {
    pub(crate) fn add_solid(&mut self) -> Result<(), String> {
        let layers = self.state["project"]["layers"]
            .as_array()
            .ok_or("missing layers")?;
        let id = layers
            .iter()
            .filter_map(|l| l["id"].as_u64())
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("layer id overflow")?;
        let w = self.state["project"]["width"].as_f64().unwrap_or(1920.0) as f32;
        let h = self.state["project"]["height"].as_f64().unwrap_or(1080.0) as f32;
        let layer = Layer::solid(
            id,
            &format!("{} {id}", self.workspace.locale.t(T::Solid)),
            [w * 0.3, h * 0.3],
            [w * 0.5, h * 0.5, 0.0],
            [0.22, 0.6, 0.96, 1.0],
        );
        self.state = self
            .engine
            .edit(serde_json::to_string(&Command::Add { layer }).map_err(|e| e.to_string())?)?;
        self.panels.selected = Some(id);
        Ok(())
    }
    pub(crate) fn action(
        &mut self,
        action: panels::Action,
        event_loop: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        use panels::Action;
        if self.importing_image.is_some() && !matches!(&action, Action::Node(_)) {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        if self.importing_media.is_some()
            && matches!(&action, Action::New | Action::Open | Action::OpenPath(_))
        {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        match action {
            Action::Node(action) => self.node_action(action)?,
            Action::NewText => {
                self.stop();
                if self.system_fonts.is_none() {
                    self.system_fonts = Some(dialogs::system_fonts());
                }
                self.dialog = Some(dialogs::Dialog::text(
                    self.system_fonts.clone().unwrap_or_default(),
                ));
            }
            Action::CompositionSettings => {
                self.stop();
                self.dialog = Some(dialogs::Dialog::settings(&self.state));
            }
            Action::ExportMovie => {
                if self.freezing_movie.is_some()
                    || self.exporting_movie.as_ref().is_some_and(|e| !e.finished())
                {
                    return Err(self.workspace.locale.t(T::Exporting).into());
                }
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::ExportMovie))
                    .add_filter("MP4", &["mp4"])
                    .set_file_name("motion.mp4")
                    .save_file()
                {
                    self.stop();
                    let control = motion_desktop_media::movie::ExportControl::default();
                    self.freezing_movie =
                        Some((self.engine.freeze_movie(control.clone())?, path, control));
                    self.export_notified = false;
                    self.panels.message = Some(self.workspace.locale.t(T::Exporting).into());
                }
            }
            Action::CancelExport => {
                if let Some(task) = &self.exporting_movie {
                    task.cancel();
                }
                if let Some((_, _, control)) = self.freezing_movie.take() {
                    control.cancel();
                }
            }
            Action::EditBatch(commands) => {
                self.stop();
                self.editing = Some((
                    Instant::now(),
                    self.engine
                        .edit_async(serde_json::to_string(&commands).map_err(|e| e.to_string())?)?,
                ));
            }
            Action::Plugin(request) => {
                self.stop();
                self.state = self.engine.plugin(request.to_string())?;
            }
            Action::ImportMedia => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::ImportMedia))
                    .add_filter(
                        "Media",
                        &[
                            "png", "jpg", "jpeg", "mp4", "mov", "mkv", "webm", "m4a", "aac", "mp3",
                            "wav", "flac", "ogg", "opus", "aiff",
                        ],
                    )
                    .pick_file()
                {
                    self.import_file(path)?;
                }
            }
            Action::ImportPath(path) => self.import_file(path)?,
            Action::CancelImport => {
                if let Some((request, _)) = self.importing_media.take() {
                    self.engine.media(
                        serde_json::json!({"op":"cancel_media_import","request_id":request})
                            .to_string(),
                    )?;
                    self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    )?;
                }
            }
            Action::AddNull => {
                self.stop();
                self.state = self.engine.edit(
                    serde_json::to_string(&workflow::null_layer(
                        &self.state,
                        self.workspace.locale.t(T::NewNull),
                    )?)
                    .map_err(|e| e.to_string())?,
                )?;
            }
            Action::AddAdjustment => {
                self.stop();
                let id = workflow::next_layer_id(&self.state)?;
                self.state = self.engine.edit(
                    serde_json::to_string(&Command::AddAdjustment {
                        id,
                        name: self.workspace.locale.t(T::AdjustmentLayer).into(),
                    })
                    .map_err(|e| e.to_string())?,
                )?;
                self.panels.selected = Some(id);
                self.actions.push_back(panels::Action::ShowPanel(
                    dock::Panel::EffectControls.dock(),
                ));
            }
            Action::AddPath(path) => {
                self.stop();
                let id = workflow::next_layer_id(&self.state)?;
                let w = self.state["project"]["width"].as_f64().unwrap_or(1920.) as f32;
                let h = self.state["project"]["height"].as_f64().unwrap_or(1080.) as f32;
                let mut layer = Layer::solid(
                    id,
                    self.workspace.locale.t(T::PathLayer),
                    [w, h],
                    [w * 0.5, h * 0.5, 0.],
                    [1.; 4],
                );
                layer.content = motion_core::Content::Vector {
                    vector: crate::panels::path_controls::new_vector(path),
                };
                self.state = self.engine.edit(
                    serde_json::to_string(&Command::Add { layer }).map_err(|e| e.to_string())?,
                )?;
                self.panels.selected = Some(id);
            }
            Action::AddShape(shape) => {
                self.stop();
                let id = workflow::next_layer_id(&self.state)?;
                let name = if shape == motion_core::vector::ShapeKind::Rectangle {
                    T::NewRectangle
                } else {
                    T::NewEllipse
                };
                self.state = self.engine.edit(
                    serde_json::to_string(&Command::AddShape {
                        id,
                        name: self.workspace.locale.t(name).into(),
                        shape,
                        size: [320., 180.],
                        position: [
                            self.state["project"]["width"].as_f64().unwrap_or(1920.) as f32 / 2.,
                            self.state["project"]["height"].as_f64().unwrap_or(1080.) as f32 / 2.,
                            0.,
                        ],
                    })
                    .map_err(|e| e.to_string())?,
                )?;
                self.panels.selected = Some(id);
            }
            Action::ExportPng => {
                self.stop();
                let result = self.engine.capture()?;
                if let Some(destination) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::ExportPng))
                    .add_filter("PNG", &["png"])
                    .set_file_name("frame.png")
                    .save_file()
                {
                    std::fs::copy(
                        result["path"].as_str().ok_or("capture path missing")?,
                        &destination,
                    )
                    .map_err(|e| e.to_string())?;
                    self.panels.message = Some(format!(
                        "{}: {}",
                        self.workspace.locale.t(T::Saved),
                        destination.display()
                    ));
                }
            }
            Action::RefreshState => {
                self.stop();
                self.state = self.engine.state()?;
                self.frame = self.state["frame"].as_f64().unwrap_or(0.0);
            }
            Action::Language(locale) => {
                let sheet=motion_ui::style::default_stylesheet();
                self.atlas = TextAtlas::from_family(sheet.font_family(),sheet.base_font_size(),locale==Locale::Zh)?;
                self.atlas.set_pixel_ratio(self.scale);
                self.workspace.locale = locale;
                self.panels.locale = locale;
                for f in self.floating.values() {
                    f.window
                        .set_title(&format!("{} — Motion Studio", locale.panel(f.panel.dock())));
                }
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
                self.panels.message = Some(locale.t(T::Ready).into());
            }
            Action::ResetWorkspace => {
                self.panels.node_editor.cancel_interaction();
                let ids: Vec<_> = self.floating.values().map(|f| f.panel.dock()).collect();
                for id in ids {
                    self.redock_panel(id)?;
                }
                self.workspace.root = dock::editor_layout();
                self.workspace.floating.clear();
                self.workspace.hidden.clear();
                self.workspace.cancel();
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::ShowPanel(id) => {
                self.panels.node_editor.cancel_interaction();
                if let Some(f) = self.floating.values().find(|f| f.panel.dock() == id) {
                    f.window.focus_window();
                } else if let Some(dock::Node::Dock { id: dock, .. }) = self.workspace.root.find(id)
                {
                    let dock = *dock;
                    self.workspace.root.activate(dock, id);
                } else if panels::panel(id).is_some() {
                    self.workspace.redock(id);
                    if self.smoke.is_none() {
                        self.workspace.save(&self.layout_path)?;
                    }
                }
            }
            Action::About => {
                self.panels.message = Some(self.workspace.locale.t(T::AboutDescription).into())
            }
            Action::PreviewMode(mode) => {
                self.state["preview"] = self.engine.preview_mode(mode, 0)?
            }
            Action::SelectAt(point) => {
                self.stop();
                let result = self.engine.hit_candidates(point[0], point[1])?;
                self.panels.selected = result["candidates"]
                    .as_array()
                    .and_then(|c| c.first())
                    .and_then(|c| c["id"].as_u64());
            }
            Action::PreviewDrag { object, delta, size } => {
                self.stop();
                self.editing = Some((Instant::now(), self.engine.drag_async(object as i64, delta, size)?));
            }
            Action::Float(panel) => {
                self.float_panel(panel, event_loop.ok_or("window event loop is unavailable")?)?;
            }
            Action::ClosePanel(panel) => {
                for action in panels::cancel_interaction(&mut self.panels) {
                    self.action(action, event_loop)?;
                }
                if self.floating.values().any(|f| f.panel.dock() == panel) {
                    self.redock_panel(panel)?;
                }
                self.workspace.close(panel);
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::Redock(panel) => {
                self.redock_panel(panel)?;
            }
            Action::Activate { dock, tab } => {
                self.panels.node_editor.cancel_interaction();
                self.workspace.root.activate(dock, tab);
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::OpenPath(path) => {
                self.stop();
                self.state = if path.is_dir() {
                    self.engine.open_directory(path)?
                } else if path.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("msproj") || e.eq_ignore_ascii_case("aem")
                }) {
                    self.engine.import_project(path)?
                } else {
                    self.engine.open_directory(
                        path.parent()
                            .ok_or("project file has no parent")?
                            .to_path_buf(),
                    )?
                };
                self.frame = 0.0;
                self.panels.selected = None;
            }
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::OpenProject))
                    .add_filter("Motion Studio Project", &["json", "msproj", "aem"])
                    .pick_file()
                {
                    self.stop();
                    self.state = if path.extension().is_some_and(|e| {
                        e.eq_ignore_ascii_case("msproj") || e.eq_ignore_ascii_case("aem")
                    }) {
                        self.engine.import_project(path)?
                    } else {
                        self.engine.open_directory(
                            path.parent()
                                .ok_or("project file has no parent")?
                                .to_path_buf(),
                        )?
                    };
                    self.frame = self.state["frame"].as_f64().unwrap_or(0.0);
                    self.panels.selected = None;
                    self.panels.message = Some(self.workspace.locale.t(T::ProjectOpened).into());
                }
            }
            Action::New => {
                self.stop();
                let mut project = Project::new(1920, 1080, 30, 300).map_err(|e| e.to_string())?;
                project.name = self.workspace.locale.t(T::Untitled).into();
                self.state = self
                    .engine
                    .new_project(serde_json::to_string(&project).map_err(|e| e.to_string())?)?;
                self.frame = 0.0;
                self.panels.selected = None;
                self.panels.message = Some(self.workspace.locale.t(T::NewComposition).into());
            }
            Action::Save => {
                self.state = self.engine.save()?;
                self.panels.message = Some(format!(
                    "{}: {}",
                    self.workspace.locale.t(T::Saved),
                    self.state["root"].as_str().unwrap_or("")
                ));
            }
            Action::Pack => {
                let result = self.engine.pack()?;
                self.panels.message = Some(format!(
                    "{}: {}",
                    self.workspace.locale.t(T::ProjectPackage),
                    result["path"].as_str().unwrap_or("")
                ));
            }
            Action::History(op) => {
                if self.panels.node_editor.focused() && matches!(op, 0 | 1) {
                    let changed = if op == 0 {
                        self.panels.node_editor.undo()
                    } else {
                        self.panels.node_editor.redo()
                    };
                    if changed {
                        self.schedule_node_compile()?;
                    }
                } else {
                    self.stop();
                    self.editing = Some((Instant::now(), self.engine.history_async(op)?));
                }
            }
            Action::Seek(frame) => {
                self.stop();
                self.frame = frame;
                self.editing = Some((Instant::now(), self.engine.seek_async(frame)?));
            }
            Action::TogglePlay => {
                if std::env::var_os("MOTION_TRACE_UI").is_some(){eprintln!("[motion-ui] toggle playback, previously {}",self.playback.is_some());}
                if self.playback.is_some() {
                    self.stop();
                } else {
                    if self.state["has_audio"].as_bool().unwrap_or(false) {
                        let project =
                            serde_json::from_value::<Project>(self.state["project"].clone())
                                .map_err(|e| e.to_string())?;
                        match audio_output::AudioOutput::start(
                            project,
                            std::path::Path::new(
                                self.state["root"].as_str().ok_or("project root missing")?,
                            ),
                            self.frame,
                        ) {
                            Ok(audio) => self.audio = Some(audio),
                            Err(error) => {
                                self.panels.message = Some(format!(
                                    "{}: {error}",
                                    self.workspace.locale.t(T::AudioUnavailable)
                                ))
                            }
                        }
                    }
                    self.playback = Some((Instant::now(), self.frame));
                    self.panels.playing = true;
                }
            }
            Action::AddSolid => {
                self.stop();
                self.add_solid()?;
            }
            Action::Edit(command) => {
                self.stop();
                self.editing = Some((
                    Instant::now(),
                    self.engine
                        .edit_async(serde_json::to_string(&command).map_err(|e| e.to_string())?)?,
                ));
            }
        }
        self.dirty = true;
        Ok(())
    }
    #[track_caller]
    pub(crate) fn stop(&mut self) {
        if self.playback.is_some() && std::env::var_os("MOTION_TRACE_UI").is_some() {
            eprintln!("[motion-ui] stopped by {}", std::panic::Location::caller());
        }
        if let Some(mut audio) = self.audio.take() {
            audio.stop();
        }
        self.playback = None;
        self.next_composition=Instant::now();
        self.panels.playing = false;
    }
}
