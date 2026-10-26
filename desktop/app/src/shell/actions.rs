use crate::*;

fn selected_candidate(result: &Value) -> Option<u64> {
    let Some(candidates) = result["candidates"].as_array() else {
        log::warn!("hit selection response invalid reason=candidates_not_array");
        return None;
    };
    let candidate = candidates.first()?; // Empty means no hit, which is normal.
    candidate["id"].as_u64().or_else(|| {
        log::warn!("hit selection response invalid reason=first_candidate_id_not_unsigned");
        None
    })
}

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
    fn hit_response_distinguishes_no_hit_from_malformed_data_in_default_logs() {
        const CHILD: &str = "MOTION_HIT_RESPONSE_OBSERVATION_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            assert_eq!(
                selected_candidate(&serde_json::json!({"candidates":[]})),
                None
            );
            assert_eq!(
                selected_candidate(&serde_json::json!({"candidates":[{"id":17}]})),
                Some(17)
            );
            assert!(std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .is_empty());
            for response in [
                serde_json::json!({}),
                serde_json::json!({"candidates":{}}),
                serde_json::json!({"candidates":[{"id":"17"}]}),
            ] {
                assert_eq!(selected_candidate(&response), None);
            }
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            assert_eq!(
                text.lines()
                    .filter(|line| line.contains("hit selection response invalid"))
                    .count(),
                3
            );
            assert!(
                text.contains("candidates_not_array")
                    && text.contains("first_candidate_id_not_unsigned")
            );
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "shell::actions::tests::hit_response_distinguishes_no_hit_from_malformed_data_in_default_logs"])
            .env_remove("MOTION_LOG").env(CHILD, temp.path()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
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
        self.panels.select_only(Some(id));
        Ok(())
    }
    fn save_preview_preferences(&self, operation: &str) -> Result<(), String> {
        let began = Instant::now();
        log::info!("preview preferences start operation={operation} phase=save path={}", self.layout_path.display());
        let result = self.workspace.save(&self.layout_path);
        match &result {
            Ok(()) => log::debug!("preview preferences done operation={operation} elapsed_ms={}", began.elapsed().as_millis()),
            Err(error) => log::error!("preview preferences failed operation={operation} phase=save path={} elapsed_ms={} cause={error}", self.layout_path.display(), began.elapsed().as_millis()),
        }
        result
    }
    pub(crate) fn action(
        &mut self,
        action: panels::Action,
        event_loop: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        use panels::Action;
        let mut context = crate::diagnostics::Context::from_state(&self.state);
        context.playing = self.playback.is_some();
        context.pointer_dragging = self.input.held == Some(MouseButton::Left);
        context.panel = self.panels.focused.map(|p| p.dock().0);
        context.action = match &action {
            Action::Edit(command) => motion_core::CommandRegistry::id(command),
            Action::EditBatch(_) => "command_batch",
            Action::OrderLayers { .. } => "reorder_layers",
            Action::AlignLayers { .. } => "align_layers",
            Action::DistributeLayers { .. } => "distribute_layers",
            Action::Seek(_) => "seek",
            Action::History(_) => "history",
            Action::Plugin(_) => "plugin",
            Action::TogglePlay => "toggle_play",
            Action::PreviewDrag { .. } => "preview_drag",
            Action::SelectAt(_) => "select_at",
            Action::ExportDiagnostics => "export_diagnostics",
            _ => "menu_action",
        };
        self.engine.diagnostics().set_context(context);
        let boundary = match &action {
            Action::ApplyWorkspace { .. } => Some("apply_workspace"),
            Action::Workspace(_) => Some("workspace"),
            Action::ResetWorkspace => Some("reset_workspace"),
            Action::ShowPanel(_) => Some("show_panel"),
            Action::ClosePanel(_) => Some("close_panel"),
            Action::Activate { .. } => Some("activate_tab"),
            _ => None,
        };
        if let Some(operation) = boundary {
            panels::observe_interaction_boundary(&self.panels, operation);
        }

        if self.project_modal() && !matches!(&action, Action::ProjectChoice(_)) {
            return Ok(());
        }
        if self.importing_image.is_some() && !matches!(&action, Action::Node(_)) {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        if self.importing_media.is_some()
            && matches!(&action, Action::New | Action::Open | Action::OpenPath(_))
        {
            return Err(self.workspace.locale.t(T::Importing).into());
        }
        if self.importing_effect.is_some()
            && matches!(
                &action,
                Action::New
                    | Action::Open
                    | Action::OpenPath(_)
                    | Action::ImportEffectPackage
                    | Action::ImportEffectPath(_)
            )
        {
            return Err(if self.workspace.locale == Locale::Zh {
                "正在导入效果包，请稍候"
            } else {
                "Wait for the effect package import to finish"
            }
            .into());
        }
        match action {
            Action::MediaAsset { source, intent } => self.asset_action(source, intent, event_loop)?,
            Action::AnalyzeAudio(object) => self.analyze_audio(object)?,
            Action::CancelAudioAnalysis => self.cancel_audio_analysis(),
            Action::ProjectChoice(choice) => self.choose_project_change(choice)?,
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
            Action::NewComposition => {
                self.stop();
                self.dialog = Some(dialogs::Dialog::new_composition(1920, 1080));
            }
            Action::CompositionPreset(w, h) => {
                self.stop();
                self.dialog = Some(dialogs::Dialog::new_composition(w, h));
            }
            Action::Preferences => self.open_preferences(),
            Action::CompositionSettings => {
                self.stop();
                self.dialog = Some(dialogs::Dialog::settings(&self.state));
            }
            Action::ProjectPrecisionSettings => {
                self.stop();
                self.dialog = Some(dialogs::Dialog::project_precision(&self.state)?);
            }
            Action::KeyframeTimingSettings { source, selections } => {
                self.open_keyframe_timing(source, selections)?;
            }
            Action::ExportMovie => {
                if self.freezing_movie.is_some()
                    || self.exporting_frame.is_some()
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
                        Some((self.engine.freeze_movie(control.clone())?, path, control, self.workspace.movie_export_scope));
                    self.export_notified = false;
                    self.panels.message = Some(self.workspace.locale.t(T::Exporting).into());
                }
            }
            Action::CancelExport => {
                if let Some((_, control)) = &self.exporting_frame {
                    control.cancel();
                }
                if let Some(task) = &self.exporting_movie {
                    task.cancel();
                }
                if let Some((_, _, control, _)) = self.freezing_movie.take() {
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
            Action::OrderLayers { objects, order } => {
                match panels::layer_navigation::plan(&self.state, &objects, order) {
                    Ok(commands) if !commands.is_empty() => {
                        let composition = self.state["composition"].as_str()
                            .or_else(|| self.state["project"]["composition_id"].as_str())
                            .ok_or("reorder_layers: missing composition identity")?.to_owned();
                        let revision = self.state["revision"].as_u64()
                            .ok_or("reorder_layers: missing project revision")?;
                        let commands: Vec<_> = commands.into_iter().map(|command|
                            motion_core::Command::InComposition { composition: composition.clone(), command: Box::new(command) }).collect();
                        self.stop();
                        self.editing = Some((Instant::now(), self.engine.edit_async_at_revision(
                            serde_json::to_string(&commands).map_err(|error| error.to_string())?, revision
                        )?));
                    }
                    Ok(_) => {},
                    Err(panels::layer_navigation::Unavailable::InvalidSnapshot) =>
                        return Err("reorder_layers: invalid layer ordering snapshot".into()),
                    Err(reason) => self.panels.message = Some(reason.message(self.workspace.locale).into()),
                }
            }
            Action::DistributeLayers {objects,distribution} => {
                self.stop();
                self.editing=Some((Instant::now(),self.engine.distribute_layers_async(objects,distribution)?));
            }
            Action::AlignLayers {objects,edge,target} => {
                self.stop();
                self.editing=Some((Instant::now(),self.engine.align_layers_async(objects,edge,target)?));
            }
            Action::NewNodeEffect(object) => {
                self.open_node_effect(object, None, false, event_loop)?
            }
            Action::EditNodeEffect { object, effect } => {
                self.open_node_effect(object, Some(effect), false, event_loop)?
            }
            Action::ApplyNodeGraph => {
                let object = self
                    .panels
                    .selected
                    .filter(|id| *id != 0)
                    .ok_or("select a layer to apply the node graph")?;
                self.open_node_effect(object, None, true, event_loop)?;
            }
            Action::ReturnToNodeDraft => self.restore_node_draft()?,
            Action::ReturnToEffects => {
                if let Some(target) = &self.panels.node_editor.layer_target {
                    self.panels.select_only(Some(target.object));
                }
                self.switch_workspace(workspace::Preset::Default, event_loop)?;
                self.action(
                    Action::ShowPanel(dock::Panel::EffectControls.dock()),
                    event_loop,
                )?;
                self.panels.focused = Some(dock::Panel::EffectControls);
            }
            Action::Plugin(request) => {
                self.stop();
                let request = self.panels.effect_request(request, &self.state);
                match self.engine.plugin(request.to_string()) {
                    Ok(next) => {
                        if request["op"] == "add" {
                            self.reveal_effect_application(&request, &next);
                        }
                        self.state = next;
                    }
                    Err(error) => {
                        if request["op"] == "add" {
                            self.panels.message = Some(format!("{}: {error}",
                                if self.workspace.locale == Locale::Zh { "添加效果失败" }
                                else { "Could not add effect" }));
                        }
                        return Err(error);
                    }
                }
            }
            Action::ImportEffectPackage => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(if self.workspace.locale == Locale::Zh {
                        "导入效果包"
                    } else {
                        "Import effect package"
                    })
                    .add_filter("Motion Studio Effects", &["msfx"])
                    .pick_file()
                {
                    self.start_effect_import(path)?;
                }
            }
            Action::ImportEffectPath(path) => self.start_effect_import(path)?,
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
                    let cancelled=self.engine.media(
                        serde_json::json!({"op":"cancel_media_import","request_id":request})
                            .to_string(),
                    );
                    let released=self.engine.media(
                        serde_json::json!({"op":"release_media_task","request_id":request})
                            .to_string(),
                    );
                    self.replacing_asset=None;
                    self.relinking_assets=false;
                    if let Err(primary)=cancelled {
                        if let Err(secondary)=released { log::error!("media cancel cleanup failed request={request} cause={secondary} primary={primary}"); }
                        return Err(primary);
                    }
                    released?;
                    self.panels.message=Some(if self.workspace.locale==Locale::Zh {"已取消，原素材保持不变"}else{"Cancelled; original asset retained"}.into());
                    self.dirty=true;
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
                self.panels.select_only(Some(id));
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
                self.panels.select_only(Some(id));
            }
            action @ (Action::AddShape(_) | Action::PlaceShape { .. }) => {
                self.stop();
                let (shape, size, position) = match action {
                    Action::PlaceShape { shape, size, position } => (shape, size, position),
                    Action::AddShape(shape) => (shape, [320., 180.], [
                        self.state["project"]["width"].as_f64().unwrap_or(1920.) as f32 / 2.,
                        self.state["project"]["height"].as_f64().unwrap_or(1080.) as f32 / 2., 0.]),
                    _ => return Err("shape action changed during dispatch".into()),
                };
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
                        size,
                        position,
                    })
                    .map_err(|e| e.to_string())?,
                )?;
                self.panels.select_only(Some(id));
            }
            Action::ExportPng => {
                if self.exporting_frame.is_some() || self.freezing_movie.is_some()
                    || self.exporting_movie.as_ref().is_some_and(|task| !task.finished()) {
                    return Err(self.workspace.locale.t(T::Exporting).into());
                }
                self.stop();
                if let Some(destination) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::ExportPng))
                    .add_filter("PNG", &["png"])
                    .set_file_name("frame.png")
                    .save_file()
                {
                    self.start_frame_export(destination)?;
                }
            }
            Action::Favorites(favorites) => {
                self.workspace.favorites = favorites.clone();
                self.panels.browser.favorites = favorites.clone();
                self.panels.effect_controls.favorites = favorites;
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::ReloadStyle => {
                self.reload_styles()?;
            }
            Action::RefreshState => {
                self.stop();
                self.state = self.engine.state()?;
                self.frame = self.state["frame"].as_f64().unwrap_or(0.0);
            }
            Action::Language(locale) => {
                let sheet = self
                    .panels
                    .styles
                    .clone()
                    .unwrap_or_else(motion_ui::style::default_stylesheet_arc);
                self.atlas = TextAtlas::from_family(
                    sheet.font_family(),
                    sheet.base_font_size(),
                    locale == Locale::Zh,
                )?;
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
            Action::ApplyWorkspace { visible, name } => {
                for panel in dock::Panel::available() {
                    let id = panel.dock();
                    let action = if visible.contains(&id) {
                        Action::ShowPanel(id)
                    } else {
                        Action::ClosePanel(id)
                    };
                    self.action(action, event_loop)?;
                }
                if !name.is_empty() {
                    self.workspace.save_named(name);
                }
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::ManageWorkspace => {
                self.workspace.restore_panel();
                self.workspace_manager = Some(workspace::Manager::new(&self.workspace));
            }
            Action::WindowDrag => {
                if let Some(window) = &self.window {
                    if let Err(error) = window.drag_window() { log::warn!("window drag rejected reason={error}"); }
                }
            }
            Action::WindowClose => self.request_close(),
            Action::WindowMinimize => {
                if let Some(window) = &self.window {
                    window.set_minimized(true);
                }
            }
            Action::WindowMaximize => {
                if let Some(window) = &self.window {
                    window.set_maximized(!window.is_maximized());
                }
            }
            Action::Workspace(preset) => {
                self.switch_workspace(preset, event_loop)?;
            }
            Action::ResetWorkspace => {
                self.switch_workspace(self.workspace.preset, event_loop)?;
                self.workspace.reset_preset();
            }
            Action::MaskTimeline { object, mask } => {
                self.stop();
                let actions = panels::reveal_mask_keys(&mut self.panels, &self.state, object, mask);
                for action in actions {
                    self.action(action, event_loop)?;
                }
                self.action(Action::ShowPanel(dock::Panel::Timeline.dock()), event_loop)?;
            }
            Action::ShowPanel(id) => {
                self.workspace.restore_panel();
                let id = if id == dock::Panel::Masks.dock() || id == dock::Panel::Expression.dock()
                {
                    dock::Panel::Properties.dock()
                } else {
                    id
                };
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
            Action::ExportDiagnostics => {
                if self.diagnostics_export.is_none() {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(self.workspace.locale.t(T::ExportDiagnostics))
                        .add_filter("JSON diagnostics", &["json"])
                        .set_file_name("motion-studio-stalls.json")
                        .save_file()
                    {
                        self.diagnostics_export = Some(self.engine.diagnostics().export(path)?);
                        self.panels.message =
                            Some(self.workspace.locale.t(T::DiagnosticsExporting).into());
                    }
                }
            }
            Action::ToggleDiagnostics => {
                let enabled = !self.engine.diagnostics().enabled();
                self.engine.diagnostics().set_enabled(enabled)?;
                self.workspace.send_diagnostics = enabled;
                self.workspace.save(&self.layout_path)?;
                self.state["desktop"]["diagnostics"] = self.engine.diagnostics().status();
            }
            Action::TimelineColumnVisibility(visibility) => {
                self.panels.timeline.column_visibility = visibility;
                self.workspace.set_column_visibility(visibility);
                if self.smoke.is_none() {
                    self.workspace.save(&self.layout_path)?;
                }
            }
            Action::SamplingColumnVisibility(visibility) => {
                self.panels.timeline.sampling_visibility = visibility;
                self.workspace.timeline_sampling_columns = visibility;
                if self.smoke.is_none() { self.workspace.save(&self.layout_path)?; }
            }
            Action::HideShyLayers(hidden) => {
                self.panels.timeline.hide_shy = hidden;
                self.workspace.timeline_hide_shy = hidden;
                if self.smoke.is_none() { self.workspace.save(&self.layout_path)?; }
            }
            Action::PreviewMode(mode) => {
                self.set_preview_preference(mode)?;
            }
            Action::PreviewPlayback(options) => {
                let before = self.workspace.preview_playback;
                self.workspace.preview_playback = options;
                if self.smoke.is_none() {
                    if let Err(error) = self.save_preview_preferences("preview_playback") {
                        self.workspace.preview_playback = before;
                        return Err(error);
                    }
                }
                self.stop();
                self.panels.preview_playback = options;
            }
            Action::MovieExportScope(scope) => {
                let before = self.workspace.movie_export_scope;
                self.workspace.movie_export_scope = scope;
                if self.smoke.is_none() {
                    if let Err(error) = self.save_preview_preferences("movie_export_scope") {
                        self.workspace.movie_export_scope = before;
                        return Err(error);
                    }
                }
                self.panels.movie_export_scope = scope;
            }
            Action::ViewKind(kind) => {
                self.editing = Some((Instant::now(), self.engine.view_async(kind)?));
            }
            Action::CameraGesture(action) => {
                self.editing = Some((Instant::now(), self.engine.camera_gesture_async(action)?));
            }
            Action::ViewZoom { ratio, size } => {
                self.editing = Some((
                    Instant::now(),
                    self.engine
                        .navigate_async(0., 0., ratio, true, size[0], size[1])?,
                ));
            }
            Action::SelectAt(point) => {
                self.stop();
                let result = self.engine.hit_candidates(point[0], point[1])?;
                let selected = selected_candidate(&result);
                self.panels.select_only(selected);
            }
            Action::PreviewDrag {
                object,
                delta,
                size,
            } => {
                self.stop();
                self.editing = Some((
                    Instant::now(),
                    self.engine.drag_async(object as i64, delta, size)?,
                ));
            }
            Action::Float(panel) => {
                self.float_panel(panel, event_loop.ok_or("window event loop is unavailable")?)?;
            }
            Action::ClosePanel(panel) => {
                self.workspace.restore_panel();
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
                let target = if path.is_dir() {
                    project_lifecycle::Target::Open(path)
                } else if path.extension().is_some_and(|e| {
                    e.eq_ignore_ascii_case("msproj") || e.eq_ignore_ascii_case("aem")
                }) {
                    project_lifecycle::Target::Import(path)
                } else if path.is_file()
                    || path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"))
                {
                    project_lifecycle::Target::Open(
                        path.parent()
                            .ok_or("project file has no parent")?
                            .to_path_buf(),
                    )
                } else {
                    // A stale recent-project directory is still a directory
                    // target; failed metadata must not turn it into its parent.
                    project_lifecycle::Target::Open(path)
                };
                self.request_project_change(target)?;
            }
            Action::Open => {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(self.workspace.locale.t(T::OpenProject))
                    .add_filter(
                        "Motion Studio Project (.msproj; legacy .aem)",
                        &["json", "msproj", "aem"],
                    )
                    .pick_file()
                {
                    self.action(Action::OpenPath(path), event_loop)?;
                }
            }
            Action::New => {
                self.stop();
                let mut project = Project::new(1920, 1080, 30, 300).map_err(|e| e.to_string())?;
                project.name = self.workspace.locale.t(T::Untitled).into();
                self.request_project_change(project_lifecycle::Target::New(
                    serde_json::to_string(&project).map_err(|e| e.to_string())?,
                ))?;
            }
            Action::Save => {
                if self.panels.node_editor.layer_target.is_some() {
                    let save = self
                        .panels
                        .node_editor
                        .request(node_editor::Request::Save)
                        .map_err(|e| e.to_string())?;
                    self.node_action(save)?;
                    return Ok(());
                }
                self.state = self.engine.save()?;
                self.panels.message = Some(format!(
                    "{}: {}",
                    self.workspace.locale.t(T::Saved),
                    self.state["root"].as_str().unwrap_or("")
                ));
            }
            Action::Pack => {
                self.flush_bound_source()?;
                let result = self.engine.pack()?;
                self.panels.message = Some(format!(
                    "{}: {}",
                    self.workspace.locale.t(T::ProjectPackage),
                    result["path"].as_str().unwrap_or("")
                ));
            }
            Action::LayerClipboard(intent) => self.request_layer_clipboard(intent)?,
            Action::KeyframeClipboard { intent, source, selections, targets } => self.request_keyframe_clipboard(intent, source, selections, targets)?,
            Action::PathClipboard { intent, source, selections, target } => self.request_path_clipboard(intent, source, selections, target)?,
            Action::CancelLayerClipboard => self.cancel_layer_clipboard(),
            Action::History(op) => {
                if matches!(op,0|1) && panels::tools::camera::owns_history(&self.panels) {
                    self.editing = Some((Instant::now(),self.engine.camera_gesture_async(if op==0 {motion_host::ops::preview::CameraGesture::Undo} else {motion_host::ops::preview::CameraGesture::Redo})?));
                } else if matches!(
                    self.panels.focused,
                    Some(dock::Panel::Nodes | dock::Panel::NodeProperties)
                ) && self.panels.node_editor.focused()
                    && matches!(op, 0 | 1)
                {
                    let changed = if op == 0 {
                        self.panels.node_editor.undo()
                    } else {
                        self.panels.node_editor.redo()
                    };
                    if changed {
                        self.schedule_node_compile()?;
                    }
                } else {
                    self.flush_bound_source()?;
                    self.node_tasks.cancel(&mut self.panels.node_editor);
                    self.node_compile_due = None;
                    self.engine.node_expect((u64::MAX, u64::MAX))?;
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
                if std::env::var_os("MOTION_TRACE_UI").is_some() {
                    log::debug!(
                        "[motion-ui] toggle playback, previously {}",
                        self.playback.is_some()
                    );
                }
                if self.playback.is_some() {
                    self.stop();
                } else {
                    let project = serde_json::from_value::<Project>(self.state["project"].clone())
                        .map_err(|error| error.to_string())?;
                    let plan = crate::preview_playback::Plan::new(&project, self.workspace.preview_playback, self.frame)?;
                    self.frame = plan.origin;
                    if self.state["has_audio"].as_bool().unwrap_or(false) {
                        match audio_output::AudioOutput::start_in_range(
                            project,
                            std::path::Path::new(
                                self.state["root"].as_str().ok_or("project root missing")?,
                            ),
                            self.frame,
                            plan.range,
                            plan.looping,
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
                    self.playback_plan = Some(plan);
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
            Action::EditChecked { command, revision } => {
                self.stop();
                self.editing = Some((Instant::now(), self.engine.edit_async_at_revision(
                    serde_json::to_string(&command).map_err(|e| e.to_string())?, revision)?));
            }
            Action::EditorAction { source, commands } => {
                self.stop();
                self.editing = Some((Instant::now(), self.engine.request(|reply|
                    engine::Command::EditorAction { source, commands, reply })?));
            }
            Action::PathOperation { source, operation } => {
                self.stop();
                self.editing = Some((Instant::now(), self.engine.request(|reply|
                    engine::Command::PathOperation { source, operation, reply })?));
            }
        }
        self.dirty = true;
        Ok(())
    }
    #[track_caller]
    pub(crate) fn stop(&mut self) {
        if self.playback.is_some() && std::env::var_os("MOTION_TRACE_UI").is_some() {
            log::debug!("[motion-ui] stopped by {}", std::panic::Location::caller());
        }
        if let Some(mut audio) = self.audio.take() {
            audio.stop();
        }
        self.playback = None;
        self.playback_plan = None;
        self.next_composition = Instant::now();
        self.panels.playing = false;
    }
}
