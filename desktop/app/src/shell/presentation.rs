use crate::*;
use motion_ui::dock::Panel;

#[cfg(test)]
#[path = "curve_playback_tests.rs"]
mod curve_playback_tests;

impl Shell {
    pub(crate) fn present_ui_async(&self, ui: UiFrame)
        -> Result<std::sync::mpsc::Receiver<Result<bool, String>>, String> {
        if self.audio_only_preview { self.engine.present_frozen_async(ui) }
        else { self.engine.present_async(ui) }
    }
    pub(crate) fn accept_edit_state(&mut self, state: Value) {
        if state["editorAction"]["operation"] == "effect_pick" && state["editorAction"]["cancelled"] == true {
            let text = match (self.workspace.locale, state["editorAction"]["reason"].as_str()) {
                (Locale::Zh, Some("preview_pending")) => "合成预览尚未就绪，请画面更新后重新取色",
                (Locale::Zh, Some("out_of_range")) => "所选值超出参数范围，未作修改",
                (Locale::Zh, _) => "目标或工程已变化，已取消拾取",
                (_, Some("preview_pending")) => "Preview is not ready; pick again after it updates",
                (_, Some("out_of_range")) => "Picked value is outside the parameter range; no change made",
                _ => "Target or project changed; pick cancelled",
            };
            self.panels.message = Some(text.into());
        }
        if let Some(object) = state["assetPlacement"]["object"].as_u64() {
            self.panels.select_only(Some(object));
        }
        if state["assetRegistration"]["cancelled"] == false {
            self.panels.message = Some(if self.workspace.locale == Locale::Zh {
                "已导入项目；选择“放入当前合成”创建图层"
            } else {
                "Imported into project; choose Place in composition to create a layer"
            }.into());
        }
        if state["assetOperation"]["cancelled"] == true {
            self.panels.message = Some(if self.workspace.locale == Locale::Zh {
                "工程已变化，已取消素材操作"
            } else {
                "Project changed; asset operation cancelled"
            }.into());
        }
        if state["editorAction"]["operation"]=="project_open" && state["editorAction"]["cancelled"]==false {
            self.frame=state["frame"].as_f64().unwrap_or(0.);
            self.panels.normalize_selection(&state);
            self.panels.select_only(None);
        }
        if state["editorAction"]["cancelled"]==true && state["editorAction"]["operation"]=="keyframe_timing" {
            self.reject_stale_keyframe_timing();
        }
        if state["edit_result"]["op"] == "create_camera" {
            if let Some(object) = state["edit_result"]["object"].as_u64() {
                self.panels.select_only(Some(object));
            }
        }
        self.state = state;
        // Finish the commands emitted by one interaction pass before presenting.
        // Begin/change/commit do not need GPU round trips between each step.
        self.present_due = self.actions.is_empty();
    }
    pub(crate) fn playback_ready(&self, now: Instant) -> bool {
        self.playback.is_some()
            && !playback_input_blocks(&self.input)
            && self
                .floating
                .values()
                .all(|window| !playback_input_blocks(&window.input))
            && now >= self.next_composition
            && self.editing.is_none()
            && !self.layer_clipboard.committing()
            && self.actions.is_empty()
            && (self.audio_only_preview || (self.attached
                && self.rendering.is_empty()
                && self.exposure_presentations.is_empty()
                && !self.present_due
                && self.resizing.is_none()
                && self.resize_queue.is_empty()))
    }
    // A chrome-only reply in another native window cannot acknowledge a
    // partially accumulated composition. Explicit user seeks still supersede
    // the host request; only the automatic playback clock waits here.
    pub(crate) fn track_exposure_present(&mut self, window:u64, composition:bool) {
        if composition && self.state["preview"]["exposureRequired"] == true {
            self.exposure_presentations.insert(window);
        } else {
            self.exposure_presentations.remove(&window);
        }
    }
    pub(crate) fn finish_exposure_present(&mut self, window:u64, result:&Result<bool,String>) {
        if !matches!(result,Ok(false)) { self.exposure_presentations.remove(&window); }
    }
    pub(crate) fn presentation_pending(&self) -> bool {
        self.pending || !self.exposure_presentations.is_empty()
    }
    pub(crate) fn lose_focus(&mut self) {
        if self.panels.project.browser.preview_in(None) {
            self.panels.project.browser.cancel();
            self.panels.project.preview.close();
        }
        self.workspace.cancel();
        self.actions
            .extend(panels::cancel_for_focus_loss(&mut self.panels));
        self.translate(Event::FocusLost);
    }
    pub(crate) fn pointer_left(&mut self) {
        if self.input.held.is_some() {
            self.workspace.cancel();
            self.actions
                .extend(panels::cancel_interaction(&mut self.panels));
        }
        self.translate(Event::PointerLeft);
    }
    pub(crate) fn ui_interval(&self) -> Duration {
        let rate = self
            .window
            .as_ref()
            .and_then(|w| w.current_monitor())
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|v| v as f64 / 1000.)
            .unwrap_or(60.)
            .clamp(30., 240.);
        Duration::from_secs_f64(1. / rate)
    }
    pub(crate) fn translate(&mut self, event: Event) {
        motion_ui::ui::accumulate(&mut self.input, event);
        self.dirty = true;
        if Instant::now() >= self.next_redraw {
            self.redraw();
        }
    }
    pub(crate) fn redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        for f in self.floating.values() {
            f.window.request_redraw();
        }
    }
    pub(crate) fn size(&self) -> Option<[f32; 2]> {
        self.window.as_ref().map(|w| {
            let s = w.inner_size();
            [s.width as f32 / self.scale, s.height as f32 / self.scale]
        })
    }
    pub(crate) fn draw_commands(&mut self, size: [f32; 2]) -> (PaintList, Vec<panels::Action>) {
        self.input.text_clipboard = Some(self.text_clipboard.clone());
        self.panels.clipboard_available = self.layer_clipboard.available();
        self.panels.keyframe_clipboard_available = self.layer_clipboard.keyframes_available();
        self.panels.path_clipboard_available = self.layer_clipboard.paths_available();
        self.panels.clipboard_busy = self.layer_clipboard.busy();
        self.panel_maximize_shortcut(size);
        self.atlas.set_pixel_ratio(self.scale);
        let mut paint = self.engine.take_paint();
        paint.stylesheet = self.panels.styles.clone();
        let neutral = Input::default();
        let project_modal = self.project_modal();
        let began = Instant::now();
        let mut actions = if let Some(home) = &mut self.home {
            home.draw(
                &mut paint,
                &mut self.atlas,
                size,
                &self.workspace.recent_projects,
                if self.dialog.is_some() || project_modal || self.workspace_manager.is_some() {
                    &neutral
                } else {
                    &self.input
                },
                self.workspace.locale,
            )
        } else {
            panels::chrome(
                &mut paint,
                &mut self.atlas,
                size,
                self.workspace.display_root(),
                &mut self.panels,
                &self.state,
                if self.dialog.is_some()
                    || self.workspace_manager.is_some()
                    || project_modal
                    || self.node_close_prompt.is_some()
                    || self.node_examples.is_some()
                {
                    &neutral
                } else {
                    &self.input
                },
            )
        };
        if self.home.is_some() {
            panels::titlebar::render(
                &mut paint,
                &mut self.atlas,
                size,
                &self.state,
                &self.input,
                &mut actions,
            );
        }
        let mut context = self.engine.diagnostics().context();
        context.panel = self.panels.focused.map(|p| p.dock().0);
        context.playing = self.playback.is_some();
        context.pointer_dragging = self.input.held == Some(MouseButton::Left);
        context.frame = self.frame;
        self.engine.diagnostics().set_context(context.clone());
        self.engine.diagnostics().record(
            0,
            "ui_build",
            "panels",
            began.elapsed(),
            None,
            Some(began.elapsed()),
            context,
        );
        if let Some(dialog) = self.dialog.as_mut().filter(|d|d.timing_owner().is_none_or(|owner|owner==0)) {
            if let Some(event) = dialog.render(
                &mut paint,
                &mut self.atlas,
                size,
                &self.input,
                self.workspace.locale,
            ) {
                let result = (|| -> Result<(), String> {
                    match event {
                        dialogs::Event::Cancelled => {
                            self.dialog = None;
                        }
                        dialogs::Event::Preferences(values) => {
                            self.apply_preferences(values, true)?;
                            self.dialog = None;
                        }
                        dialogs::Event::PreferencesImportKeys => self.shortcut_preset_picker(false)?,
                        dialogs::Event::PreferencesExportKeys => self.shortcut_preset_picker(true)?,
                        dialogs::Event::PreferencesStyle => {
                            log::info!("preferences start operation=choose_style");
                            crate::logging::flush();
                            let began = Instant::now();
                            let path = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file();
                            log::debug!("preferences done operation=choose_style elapsed_ms={} selected={}", began.elapsed().as_millis(), path.is_some());
                            if let Some(path) = path {
                                if let Some(editor) = self.dialog.as_mut().and_then(|dialog| dialog.preferences.as_mut()) {
                                    editor.draft.style_path = Some(path);
                                }
                            }
                        }
                        dialogs::Event::ImportFont => {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Fonts", &["ttf", "otf", "ttc"])
                                .pick_file()
                            {
                                let mut faces = dialogs::imported_faces(&path)?;
                                faces.reverse();
                                for face in faces {
                                    self.dialog.as_mut().unwrap().add_font(face);
                                }
                            }
                        }
                        dialogs::Event::CreateComposition(settings) => {
                            let settings =
                                serde_json::from_value::<motion_core::CompositionSettings>(
                                    settings,
                                )
                                .map_err(|e| e.to_string())?;
                            self.state = self.engine.edit(
                                serde_json::to_string(&Command::Composition {
                                    action: motion_core::CompositionAction::Create { settings },
                                })
                                .map_err(|e| e.to_string())?,
                            )?;
                            self.dialog = None;
                            self.home = None;
                            self.remember_project()?;
                        }
                        dialogs::Event::Settings(settings) => {
                            let settings =
                                serde_json::from_value::<motion_core::CompositionSettings>(
                                    settings,
                                )
                                .map_err(|e| e.to_string())?;
                            self.state = self.engine.edit(
                                serde_json::to_string(&Command::Composition {
                                    action: motion_core::CompositionAction::Settings { settings },
                                })
                                .map_err(|e| e.to_string())?,
                            )?;
                            self.dialog = None;
                        }
                        dialogs::Event::ProjectPrecision(precision) => {
                            let command = serde_json::to_string(&Command::SetWorkingPrecision { precision })
                                .map_err(|e| e.to_string())?;
                            self.state = self.engine.edit(command)?;
                            self.dialog = None;
                        }
                        event @ dialogs::Event::KeyframeTiming { .. } => self.timing_dialog_event(event)?,
                        dialogs::Event::Text {
                            text,
                            size,
                            width,
                            font,
                        } => {
                            self.state = self.engine.create_text(
                                text,
                                size,
                                width,
                                font.map(|f| (f.path, f.index)),
                            )?;
                            let selected = self.state["project"]["layers"]
                                .as_array()
                                .and_then(|ls| ls.last())
                                .and_then(|l| l["id"].as_u64());
                            self.panels.select_only(selected);
                            self.dialog = None;
                        }
                    };
                    self.dirty = true;
                    Ok(())
                })();
                if let Err(error) = result {
                    if let Some(dialog) = &mut self.dialog {
                        dialog.error = Some(self.workspace.locale.error(&error));
                    }
                }
            }
        }
        if let Some(browser) = &mut self.node_examples {
            if let Some(event) = browser.draw(
                &mut paint,
                &mut self.atlas,
                size,
                &self.input,
                self.workspace.locale,
            ) {
                let browser = self.node_examples.take().unwrap();
                let result = match event {
                    node_examples::Event::Cancel => Ok(()),
                    node_examples::Event::Open { id, save_first } => {
                        if !self.node_close_current(&browser.snapshot) {
                            Err(self.workspace.locale.t(T::NodeStaleOpen).into())
                        } else if save_first {
                            self.node_example_after_save = Some((browser.snapshot.clone(), id));
                            self.actions.push_back(panels::Action::Node(
                                node_editor::Action::Save(browser.snapshot),
                            ));
                            Ok(())
                        } else {
                            self.open_node_example(&browser.snapshot, id)
                        }
                    }
                };
                if let Err(error) = result {
                    self.panels.node_editor.report_error(error);
                }
                self.dirty = true;
            }
        }
        if self.node_close_prompt.is_some() {
            if let Some(choice) = node_tasks::close_prompt(
                &mut paint,
                &mut self.atlas,
                size,
                &self.input,
                self.workspace.locale,
            ) {
                let snapshot = self.node_close_prompt.take().unwrap();
                match choice {
                    node_tasks::CloseChoice::Cancel => {
                        if let Err(error) = self.schedule_node_compile() { log::error!("node close recovery compile failed operation=schedule_node_compile reason={error}"); }
                    }
                    _ if !self.node_close_current(&snapshot) => {
                        self.panels
                            .node_editor
                            .report_error(self.workspace.locale.t(T::NodeStaleClose));
                        if let Err(error) = self.schedule_node_compile() { log::error!("node close recovery compile failed operation=schedule_node_compile reason={error}"); }
                    }
                    node_tasks::CloseChoice::Discard => self.begin_close(),
                    node_tasks::CloseChoice::Save => {
                        self.node_close_save = Some(snapshot.clone());
                        self.actions
                            .push_back(panels::Action::Node(node_editor::Action::Save(snapshot)));
                    }
                }
                self.dirty = true;
            }
        }
        if let Some(manager) = &mut self.workspace_manager {
            if let Some(event) = manager.draw(
                &mut paint,
                &mut self.atlas,
                size,
                &self.input,
                self.workspace.locale,
            ) {
                match event {
                    workspace::ManagerEvent::Cancel => self.workspace_manager = None,
                    workspace::ManagerEvent::Preset(preset) => {
                        self.workspace_manager = None;
                        actions.push(panels::Action::Workspace(preset));
                    }
                    workspace::ManagerEvent::Apply { visible, name } => {
                        self.workspace_manager = None;
                        actions.push(panels::Action::ApplyWorkspace { visible, name });
                    }
                }
            }
        }
        project_lifecycle::render(self, &mut paint, size);
        self.input.begin_frame(); // Clear transient input only after it has been consumed.
        self.paint_input = self.input.clone();
        if let Some(window) = &self.window {
            window.set_cursor(
                self.cursors.get(
                    self.workspace
                        .drag_cursor()
                        .or(self.panels.cursor)
                        .unwrap_or_default(),
                    self.scale,
                ),
            );
        }
        (paint, actions)
    }
    pub(crate) fn viewer_image(&self, view: motion_ui::Rect, scale: f32) -> Option<[f32; 4]> {
        let canvas = [self.state["project"]["width"].as_f64()?, self.state["project"]["height"].as_f64()?];
        let mapping = self.panels.composition.navigation.mapping(view, canvas)?;
        let r = mapping.image;
        Some([r.min[0]*scale, r.min[1]*scale, r.width()*scale, r.height()*scale])
    }
    pub(crate) fn ui_frame(
        &mut self,
        paint: PaintList,
        size: [f32; 2],
        force_atlas: bool,
    ) -> UiFrame {
        let config = self.atlas.config();
        let atlas = if self.atlas.take_dirty() || force_atlas {
            Some(AtlasUpload {
                pixels: self.atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            })
        } else {
            None
        };
        let view = panels::composition_view_styled(self.workspace.display_root(), size, &paint.styles());
        UiFrame {
            paint,
            atlas,
            logical_size: size,
            window: 0,
            node_preview: if self.home.is_none() {
                self.node_viewport(panels::node_view_bound(self.workspace.display_root(),size,self.panels.node_editor.layer_target.is_some()), self.scale)
            } else { None },
            composition_image: self.viewer_image(view, self.scale),
            composition: if self.home.is_none() && view.width() > 0.0 && view.height() > 0.0 {
                Some([
                    view.min[0] * self.scale,
                    view.min[1] * self.scale,
                    view.width() * self.scale,
                    view.height() * self.scale,
                ])
            } else {
                None
            },
        }
    }
    pub(crate) fn present(&mut self, _event_loop: &ActiveEventLoop) -> Result<(), String> {
        if (!self.actions.is_empty() && !self.present_due)
            || self.editing.is_some()
            || !self.attached
            || self.rendering.contains_key(&0)
            || self.resizing.is_some()
            || !self.resize_queue.is_empty()
        {
            if !self.actions.is_empty() && !self.present_due {
                self.observe_action_deferral(0, "before_draw");
            }
            return Ok(());
        }
        let Some(size) = self.size() else {
            return Ok(());
        };
        if size[0] <= 0.0 || size[1] <= 0.0 {
            self.present_due = false;
            return Ok(());
        }
        // A completed edit gets a visible frame before more input can enqueue
        // another edit. Retain real input for the following interaction pass.
        if self.present_due {
            let (paint, actions) = self.draw_acknowledged(size);
            self.actions.extend(actions);
            let ui = self.ui_frame(paint, size, false);
            let composition = ui.composition.is_some();
            self.rendering.insert(0, self.present_ui_async(ui)?);
            self.track_exposure_present(0,composition);
            self.present_deferrals.submitted(0);
            self.present_due = false;
            self.dirty = true;
            return Ok(());
        }
        if let Some(press) = self.input.pressed {
            if std::env::var_os("MOTION_TRACE_UI").is_some() {
                eprintln!(
                    "[motion-ui] input press {:?}, focused {:?}, expanded {:?}",
                    press, self.panels.focused, self.panels.timeline.expanded
                );
            }
        }
        let header_hit = if self.panels.header_size == Some(size) {
            self.panels
                .header_hits
                .iter()
                .rev()
                .find(|hit| hit.rect.contains(self.input.mouse))
                .map(|hit| (hit.panel, hit.action))
        } else {
            None
        };
        let header_action = self
            .input
            .pressed
            .is_some_and(|(button, _)| button == MouseButton::Left)
            && header_hit.is_some_and(|(_, action)| action);
        let modal = self.panels.timeline_columns.editing()
            || self.panels.layer_actions.modal_in(None)
            || self.panels.layer_actions.modal_in(Some(Panel::Timeline))
            || self.panels.layer_actions.text_in(None)
            || self.panels.layer_actions.text_in(Some(Panel::Timeline))
            || self.panels.panel_menu.is_some()
            || self.panels.menu.is_some()
            || self.panels.effect_search_in(None)
            || panels::expression_overlay_open(&self.panels)
            || self.panels.node_editor.popup_open()
            || self.dialog.is_some()
            || self.project_modal()
            || self.node_close_prompt.is_some()
            || self.node_examples.is_some();
        if header_action || modal {
            self.workspace.cancel();
        }
        if let Some(change) = if header_action || modal {
            None
        } else {
            self.workspace.interact(&self.input, size)
        } {
            match change {
                workspace::Change::Float(panel) => {
                    self.actions.push_back(panels::Action::Float(panel))
                }
                workspace::Change::Changed => {
                    if self.smoke.is_none() && self.input.released.is_some() {
                        self.workspace.save(&self.layout_path)?;
                    }
                    self.dirty = true;
                }
            }
        }
        if !modal
            && self
                .input
                .pressed
                .is_some_and(|(button, _)| button == MouseButton::Left)
        {
            if let Some((panel, false)) = header_hit {
                self.workspace.choose_dragged_tab(panel);
            }
        }
        let (paint, actions) = self.draw_commands(size);
        self.actions.extend(actions);
        if !self.actions.is_empty() {
            self.observe_action_deferral(0, "after_draw");
            self.engine.recycle_paint(paint);
            return Ok(());
        }
        let ui = self.ui_frame(paint, size, false);
        let composition = ui.composition.is_some();
        self.rendering.insert(0, self.present_ui_async(ui)?);
        self.track_exposure_present(0,composition);
        self.present_deferrals.submitted(0);
        self.dirty = false;
        Ok(())
    }
    fn observe_action_deferral(&mut self, window: u64, phase: &str) {
        self.present_deferrals.deferred(
            window,
            phase,
            self.actions.len(),
            self.editing.is_some(),
            self.engine.queue_depth(),
        );
    }
    fn draw_acknowledged(&mut self, size: [f32; 2]) -> (PaintList, Vec<panels::Action>) {
        // Preserve the last consumed pointer/key state. New movement, release
        // and IME events remain queued for the next real interaction pass.
        let input = std::mem::replace(&mut self.input, self.paint_input.clone());
        let navigation = self.panels.timeline.navigation_paint_only;
        self.panels.timeline.navigation_paint_only = true;
        let frame = self.draw_commands(size);
        self.panels.timeline.navigation_paint_only = navigation;
        self.input = input;
        frame
    }
    pub(crate) fn float_panel(
        &mut self,
        id: dock::DockId,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        if id == dock::Panel::Masks.dock() || id == dock::Panel::Expression.dock() {
            return self.float_panel(dock::Panel::Properties.dock(), event_loop);
        }
        if self.floating.values().any(|f| f.panel.dock() == id) {
            return Ok(());
        }
        let panel = panels::panel(id).ok_or("invalid panel")?;
        self.workspace.restore_panel();
        if panel == dock::Panel::Nodes {
            if self.panels.node_editor.cancel_interaction() {
                self.schedule_node_compile()?;
            }
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(format!(
                            "{} — Motion Studio",
                            self.workspace.locale.panel(panel.dock())
                        ))
                        .with_inner_size(if panel == dock::Panel::Nodes {
                            winit::dpi::LogicalSize::new(1280.0, 800.0)
                        } else {
                            winit::dpi::LogicalSize::new(720.0, 520.0)
                        })
                        .with_min_inner_size(winit::dpi::LogicalSize::new(360.0, 320.0))
                        .with_visible(self.smoke.is_none()),
                )
                .map_err(|e| e.to_string())?,
        );
        if let Some(geometry) = self.workspace.window_geometry.get(&id.0) {
            if geometry
                .size
                .iter()
                .all(|v| v.is_finite() && (320.0..=8192.0).contains(v))
            {
                let _ = window.request_inner_size(winit::dpi::LogicalSize::new(
                    geometry.size[0],
                    geometry.size[1],
                ));
            }
            if let Some([x, y]) = geometry.position {
                if event_loop.available_monitors().any(|m| {
                    let p = m.position();
                    let s = m.size();
                    x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
                }) {
                    window.set_outer_position(winit::dpi::PhysicalPosition::new(x, y));
                }
            }
        }
        let size = window.inner_size();
        window.set_ime_allowed(true);
        let key = self.next_window;
        self.next_window += 1;
        self.engine
            .float_window(key, window.clone(), size.width, size.height)?;
        let scale = window.scale_factor() as f32;
        self.cursors.prepare(event_loop, scale);
        self.workspace.float(id);
        self.floating.insert(
            window.id(),
            FloatWindow {
                window,
                key,
                panel,
                input: Default::default(),
                paint_input: Default::default(),
                pointer_position: Default::default(),
                scale,
            },
        );
        if self.smoke.is_none() {
            self.workspace.save(&self.layout_path)?;
        }
        self.dirty = true;
        self.redraw();
        Ok(())
    }
    pub(crate) fn redock_panel(&mut self, panel: dock::DockId) -> Result<(), String> {
        if panel==dock::Panel::Project.dock() {
            self.panels.project.browser.cancel();
            self.panels.project.preview.close();
        }
        if self
            .panels
            .effect_search
            .as_ref()
            .is_some_and(|c| c.owner.is_some_and(|owner| owner.dock() == panel))
        {
            self.panels.effect_search = None;
        }
        if panel == dock::Panel::Nodes.dock() {
            if self.panels.node_editor.cancel_interaction() {
                self.schedule_node_compile()?;
            }
        }
        if let Some(id) = self
            .floating
            .iter()
            .find(|(_, f)| f.panel.dock() == panel)
            .map(|(id, _)| *id)
        {
            if let Some(f) = self.floating.remove(&id) {
                if self.dialog.as_ref().is_some_and(|d|d.timing_owner()==Some(f.key)) {
                    self.dialog=None;
                }
                self.exposure_presentations.remove(&f.key);
                self.present_deferrals.closed(f.key);
                self.resize_queue.remove(&f.key);
                self.engine.close_window(f.key)?;
            }
        }
        self.workspace.redock(panel);
        if panel == dock::Panel::Nodes.dock()
            && self.panels.node_preview.active != Some(self.panels.node_editor.stamp())
            && !self.closing
        {
            self.schedule_node_compile()?;
        }
        if self.smoke.is_none() {
            self.workspace.save(&self.layout_path)?;
        }
        self.dirty = true;
        self.redraw();
        Ok(())
    }
    pub(crate) fn present_float(
        &mut self,
        id: WindowId,
        _event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        self.panels.clipboard_available = self.layer_clipboard.available();
        self.panels.keyframe_clipboard_available = self.layer_clipboard.keyframes_available();
        self.panels.path_clipboard_available = self.layer_clipboard.paths_available();
        self.panels.clipboard_busy = self.layer_clipboard.busy();
        if (!self.actions.is_empty() && !self.present_due)
            || self.editing.is_some()
            || !self.attached
            || self.resizing.is_some()
            || !self.resize_queue.is_empty()
        {
            if !self.actions.is_empty() && !self.present_due {
                if let Some(window) = self.floating.get(&id).map(|f| f.key) {
                    self.observe_action_deferral(window, "before_draw");
                }
            }
            return Ok(());
        }
        let project_modal = self.project_modal();
        let f = self.floating.get_mut(&id).ok_or("floating window closed")?;
        f.input.text_clipboard = Some(self.text_clipboard.clone());
        if self.rendering.contains_key(&f.key) {
            return Ok(());
        }
        let physical = f.window.inner_size();
        if physical.width == 0 || physical.height == 0 {
            return Ok(());
        }
        let size = [
            physical.width as f32 / f.scale,
            physical.height as f32 / f.scale,
        ];
        let panel = f.panel;
        let key = f.key;
        let scale = f.scale;
        self.atlas.set_pixel_ratio(scale);
        let mut paint = self.engine.take_paint();
        paint.stylesheet = self.panels.styles.clone();
        let acknowledged = self.present_due;
        let neutral = if acknowledged { f.paint_input.clone() } else { Input {
            mouse: f.input.mouse,
            modifiers: f.input.modifiers,
            ..Default::default()
        }};
        let blocked = acknowledged
            || project_modal
            || self.dialog.is_some()
            || self.node_examples.is_some()
            || self.node_close_prompt.is_some();
        let navigation = self.panels.timeline.navigation_paint_only;
        self.panels.timeline.navigation_paint_only = acknowledged;
        let actions = panels::floating_chrome(
            &mut paint,
            &mut self.atlas,
            panel,
            size,
            &mut self.panels,
            &self.state,
            if blocked { &neutral } else { &f.input },
        );
        self.panels.timeline.navigation_paint_only = navigation;
        let timing_event = self.dialog.as_mut().filter(|d|d.timing_owner()==Some(key))
            .and_then(|dialog|dialog.render(&mut paint,&mut self.atlas,size,if acknowledged {&neutral}else{&f.input},self.workspace.locale));
        f.window.set_cursor(
            self.cursors
                .get(self.panels.cursor.unwrap_or_default(), scale),
        );
        self.actions.extend(actions);
        if !acknowledged {
            f.input.begin_frame();
            f.paint_input = f.input.clone();
        }
        if let Some(event)=timing_event {self.timing_dialog_event(event)?;}
        if !self.actions.is_empty() && !acknowledged {
            self.observe_action_deferral(key, "after_draw");
            self.engine.recycle_paint(paint);
            return Ok(());
        }
        if !self.floating.contains_key(&id) {
            return Ok(());
        }
        let config = self.atlas.config();
        let atlas = if self.atlas.take_dirty() {
            Some(AtlasUpload {
                pixels: self.atlas.pixel_data().to_vec(),
                width: config.width,
                height: config.height,
            })
        } else {
            None
        };
        let viewer = panels::floating_view_styled(panel, size, &paint.styles());
        let composition = viewer.map(|v| {
            [
                v.min[0] * scale,
                v.min[1] * scale,
                v.width() * scale,
                v.height() * scale,
            ]
        });
        self.rendering.insert(
            key,
            self.present_ui_async(UiFrame {
                paint,
                atlas,
                logical_size: size,
                composition,
                composition_image: viewer.and_then(|v| self.viewer_image(v, scale)),
                window: key,
                node_preview: self.node_viewport(panels::floating_node_view_bound(panel,size,self.panels.node_editor.layer_target.is_some()), scale),
            })?,
        );
        self.track_exposure_present(key,composition.is_some());
        self.present_deferrals.submitted(key);
        if acknowledged {
            self.present_due = false;
            self.dirty = true;
        }
        Ok(())
    }
    pub(crate) fn floating_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
        let Some(f) = self.floating.get_mut(&id) else {
            return;
        };
        let input_event = match event {
            WindowEvent::CloseRequested => {
                if f.panel == dock::Panel::Nodes {
                    self.node_tasks.cancel(&mut self.panels.node_editor);
                    self.node_compile_due = None;
                }
                self.actions
                    .push_back(panels::Action::Redock(f.panel.dock()));
                return;
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.present_float(id, event_loop) {
                    self.panels.message = Some(self.workspace.locale.error(&e));
                }
                return;
            }
            WindowEvent::Resized(size) => {
                static POSITION_FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
                let position = match f.window.outer_position() {
                    Ok(p) => { POSITION_FAILURES.success(); Some([p.x, p.y]) }
                    Err(error) => { POSITION_FAILURES.failure("window.geometry.position", format_args!("window={} reason={error} fallback=no_position", f.key)); None }
                };
                self.workspace.window_geometry.insert(
                    f.panel.dock().0,
                    workspace::WindowGeometry {
                        size: [
                            size.width as f64 / f.scale as f64,
                            size.height as f64 / f.scale as f64,
                        ],
                        position,
                    },
                );
                if self.smoke.is_none() {
                    observe_geometry_save(self.workspace.save(&self.layout_path), &self.layout_path);
                }
                let key = f.key;
                if size.width > 0 && size.height > 0 {
                    self.resize_queue.insert(key, [size.width, size.height]);
                }
                self.dirty = true;
                self.redraw();
                return;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                f.scale = scale_factor as f32;
                self.cursors.prepare(event_loop, f.scale);
                self.dirty = true;
                self.redraw();
                return;
            }
            WindowEvent::Moved(position) => {
                let size = f.window.inner_size();
                self.workspace.window_geometry.insert(
                    f.panel.dock().0,
                    workspace::WindowGeometry {
                        size: [
                            size.width as f64 / f.scale as f64,
                            size.height as f64 / f.scale as f64,
                        ],
                        position: Some([position.x, position.y]),
                    },
                );
                if self.smoke.is_none() {
                    observe_geometry_save(self.workspace.save(&self.layout_path), &self.layout_path);
                }
                return;
            }
            WindowEvent::ModifiersChanged(m) => {
                let m = m.state();
                f.input.modifiers = Modifiers {
                    control: m.control_key() || m.super_key(),
                    shift: m.shift_key(),
                    alt: m.alt_key(),
                };
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                Some(f.pointer_position.moved(position, f.scale))
            }
            #[cfg(target_os = "windows")]
            WindowEvent::CursorEntered { .. } => f.pointer_position.entered(f.scale),
            WindowEvent::DroppedFile(path) => {
                if f.panel == dock::Panel::Project && super::project_import::media_path(&path) {
                    let position = f.pointer_position.dropped(&f.window, f.scale);
                    let size = f.window.inner_size();
                    let size = [size.width as f32 / f.scale, size.height as f32 / f.scale];
                    let styles = self.panels.styles.clone().unwrap_or_else(motion_ui::style::default_stylesheet_arc);
                    if let Some(bounds) = panels::floating_view_styled(dock::Panel::Project, size, &styles) {
                        self.drop_project_media(path, position, bounds);
                    }
                    self.redraw();
                    return;
                }
                if f.panel == dock::Panel::Nodes && super::node_image_drop::node_image_path(&path) {
                    let position = f.pointer_position.dropped(&f.window, f.scale);
                    let key = f.key;
                    self.drop_node_image(path.clone(), position, key);
                }
                if path.extension().is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("msfx")) {
                    self.actions.push_back(panels::Action::ImportEffectPath(path));
                }
                self.dirty = true;
                self.redraw();
                return;
            }
            WindowEvent::CursorLeft { .. } => {
                if f.input.held.is_some() {
                    self.workspace.cancel();
                    self.actions
                        .extend(panels::cancel_interaction(&mut self.panels));
                }
                Some(Event::PointerLeft)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    WinitMouseButton::Left => MouseButton::Left,
                    WinitMouseButton::Right => MouseButton::Right,
                    _ => MouseButton::Middle,
                };
                Some(if state == ElementState::Pressed {
                    Event::MousePressed {
                        position: f.input.mouse,
                        button,
                    }
                } else {
                    Event::MouseReleased {
                        position: f.input.mouse,
                        button,
                    }
                })
            }
            WindowEvent::MouseWheel { delta, .. } => Some(Event::MouseWheel {
                delta: mouse_wheel_delta(delta, f.scale),
            }),
            WindowEvent::KeyboardInput { event, .. } => {
                for event in keyboard_events(&event, &f.input) {
                    motion_ui::ui::accumulate(&mut f.input, event);
                }
                self.dirty = true;
                self.redraw();
                return;
            }
            WindowEvent::Ime(ime) => translate_ime(ime),
            WindowEvent::Focused(false) => {
                if self.panels.project.browser.preview_in(Some(f.panel)) {
                    self.panels.project.browser.cancel();
                    self.panels.project.preview.close();
                }
                self.actions
                    .extend(panels::cancel_for_focus_loss(&mut self.panels));
                Some(Event::FocusLost)
            }
            _ => None,
        };
        if let Some(event) = input_event {
            motion_ui::ui::accumulate(&mut f.input, event);
            self.dirty = true;
            self.redraw();
        }
    }
}

// Hover changes UI paint, not composition time. Preserve the ordering barrier
// for real interactions without starving playback on a stream of pointer moves.
fn playback_input_blocks(input: &Input) -> bool {
    input.pending
        && (input.pressed.is_some()
            || input.released.is_some()
            || input.held.is_some()
            || input.wheel != [0.; 2]
            || input.long_press.is_some()
            || input.focus_lost
            || !input.keys.is_empty()
            || !input.released_keys.is_empty()
            || !input.text.is_empty()
            || input.composing
            || input.ime_consumed)
}

#[cfg(test)]
mod playback_tests {
    use super::*;

    #[test]
    fn incomplete_exposure_waits_for_its_canvas_reply_without_blocking_manual_seek() {
        let folder=tempfile::tempdir().unwrap();
        let mut shell=Shell::new(folder.path().join("project"),None,Some(Locale::En)).unwrap();
        shell.attached=true;
        shell.pending=false;
        shell.playback=Some((Instant::now()-Duration::from_millis(200),0.));
        shell.next_composition=Instant::now();
        shell.state["preview"]["exposureRequired"]=serde_json::json!(true);
        assert!(shell.playback_ready(Instant::now()));
        for window in [0,9] {
            shell.track_exposure_present(window,true);
            shell.finish_exposure_present(window,&Ok(false));
            assert!(!shell.playback_ready(Instant::now()));
            assert!(shell.presentation_pending());
            shell.track_exposure_present(3,false);
            shell.finish_exposure_present(3,&Ok(true));
            assert!(!shell.playback_ready(Instant::now()),"another window only painted chrome");
            assert!(shell.presentation_pending(),"keep scheduling canvas redraws after chrome acknowledges");
            shell.finish_exposure_present(window,&Ok(true));
            assert!(shell.playback_ready(Instant::now()));
            assert!(!shell.presentation_pending());
        }
        shell.track_exposure_present(9,true);
        shell.finish_exposure_present(9,&Err("controlled failure".into()));
        assert!(shell.exposure_presentations.is_empty());
        shell.track_exposure_present(9,true);
        shell.track_exposure_present(9,false); // Composition was hidden or redocked.
        assert!(shell.playback_ready(Instant::now()));
        shell.track_exposure_present(0,true);
        shell.actions.push_back(panels::Action::Seek(4.));
        assert!(!shell.playback_ready(Instant::now()));
        // The barrier is consulted by automatic playback, not action dispatch.
        assert!(matches!(crate::shell::actions::pop_action(&mut shell.actions),Some(panels::Action::Seek(4.))));
        shell.state["preview"]["exposureRequired"]=serde_json::json!(false);
        shell.track_exposure_present(0,true);
        assert!(shell.playback_ready(Instant::now()));
    }

    #[test]
    fn repeated_pointer_events_cannot_starve_composition_ticks() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.attached = true;
        shell.playback = Some((Instant::now() - Duration::from_millis(150), 0.));
        for tick in 0..12 {
            for movement in 0..120 {
                shell.translate(Event::MouseMoved {
                    position: [movement as f32, tick as f32],
                });
            }
            assert!(shell.input.pending);
            shell.next_composition = Instant::now();
            assert!(shell.playback_ready(Instant::now()));
            shell.advance_playback().unwrap();
            assert!(!shell.playback_ready(Instant::now())); // Never overlap seeks.
            let (_, reply) = shell.editing.take().unwrap();
            shell.state = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
            assert!(shell.state["frame"].as_f64().unwrap() >= 4.);
            assert!(shell.playback.is_some());
        }
    }

    #[test]
    fn active_interaction_and_acknowledged_frames_still_block_playback() {
        let mut input = Input {
            pending: true,
            ..Default::default()
        };
        assert!(!playback_input_blocks(&input));
        input.pressed = Some((MouseButton::Left, [10., 10.]));
        assert!(playback_input_blocks(&input));
        input.pressed = None;
        input.held = Some(MouseButton::Left);
        assert!(playback_input_blocks(&input));
        input.held = None;
        input.text = "12".into();
        assert!(playback_input_blocks(&input));
        input.text.clear();
        input.keys.push((Key::Space, Modifiers::default()));
        assert!(playback_input_blocks(&input));
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.attached = true;
        shell.playback = Some((Instant::now(), 0.));
        assert!(shell.playback_ready(Instant::now()));
        shell.present_due = true;
        assert!(!shell.playback_ready(Instant::now()));
        shell.present_due = false;
        shell.actions.push_back(panels::Action::Save);
        assert!(!shell.playback_ready(Instant::now()));
    }
}

#[cfg(test)]
mod interaction_tests {
    use super::*;
    use motion_ui::Rect;

    fn node_shell(root: PathBuf) -> (Shell, Rect) {
        let mut shell = Shell::new(root, None, Some(Locale::En)).unwrap();
        shell.home = None;
        let id = dock::Panel::Nodes.dock();
        shell.workspace.root = dock::Node::Dock { id, tabs: vec![id], active: 0 };
        shell.panels.focused = Some(dock::Panel::Nodes);
        let mut graph = shell.panels.node_editor.snapshot().document;
        let mut math = motion_graph::Node::new("math", motion_graph::NodeOp::Math {
            operation: motion_graph::MathOp::Add, value_type: motion_graph::ValueType::Float });
        math.position = [380., 0.];
        graph.nodes = vec![motion_graph::Node::new("number", motion_graph::NodeOp::Float { value: 1. }), math];
        graph.connections.clear();
        graph.public_parameters.clear();
        shell.panels.node_editor = node_editor::NodeEditor::new(graph);
        shell.draw_commands([1200., 900.]);
        let area = panels::frames(&shell.workspace.root, [1200., 900.])[0].content;
        let body = Rect::new(area.min[0], area.min[1] + 28., area.width(), area.height() - 28.);
        let editor = node_preview::regions(body).0;
        let canvas = shell.panels.node_editor.layout(editor).canvas;
        (shell, canvas)
    }


    #[test]
    fn home_hides_active_node_preview_and_restores_it_without_project_edits() {
        for locale in [Locale::Zh, Locale::En] {
            let folder = tempfile::tempdir().unwrap();
            let (mut shell, _) = node_shell(folder.path().join("project"));
            shell.workspace.locale = locale;
            shell.panels.node_preview.active = Some(shell.panels.node_editor.stamp());
            let active = shell.panels.node_preview.active;
            let before = shell.state.clone();
            let graph = shell.panels.node_editor.snapshot().document;
            for scale in [1., 1.5, 2.] {
                shell.scale = scale;
                for size in [[1200., 900.], [1598., 998.]] {
                    shell.home = None;
                    let (paint, actions) = shell.draw_commands(size);
                    assert!(actions.is_empty());
                    assert!(shell.ui_frame(paint, size, false).node_preview.is_some());
                    shell.home = Some(home::Home::default());
                    let (paint, actions) = shell.draw_commands(size);
                    assert!(actions.is_empty());
                    let ui = shell.ui_frame(paint, size, false);
                    assert!(ui.node_preview.is_none(), "home retained a node preview overlay");
                    assert!(ui.composition.is_none());
                    assert_eq!(shell.panels.node_preview.active, active);
                    assert_eq!(shell.state, before);
                    assert_eq!(shell.panels.node_editor.snapshot().document, graph);
                }
            }
        }
    }

    #[test]
    fn acknowledged_frame_preserves_node_drag_and_pending_release() {
        let folder = tempfile::tempdir().unwrap();
        let (mut shell, canvas) = node_shell(folder.path().join("project"));
        let point = shell.panels.node_editor.view.world_to_screen([60., 14.], canvas);
        shell.translate(Event::MousePressed { position: point, button: MouseButton::Left });
        assert!(shell.draw_commands([1200., 900.]).1.is_empty());
        let before = shell.panels.node_editor.snapshot();
        let moved = [point[0] + 48., point[1] + 24.];
        shell.translate(Event::MouseMoved { position: moved });
        shell.draw_commands([1200., 900.]);
        let dragging = shell.panels.node_editor.snapshot();
        assert_ne!(dragging.document, before.document);
        shell.translate(Event::MouseReleased { position: moved, button: MouseButton::Left });
        assert!(shell.draw_acknowledged([1200., 900.]).1.is_empty());
        assert_eq!(shell.panels.node_editor.snapshot().document, dragging.document);
        assert!(shell.panels.node_editor.editing());
        assert!(shell.input.released.is_some() && shell.input.pending);
        shell.draw_commands([1200., 900.]);
        assert!(!shell.panels.node_editor.editing());
        assert_eq!(shell.panels.node_editor.snapshot().document, dragging.document);
        assert!(shell.panels.node_editor.undo());
        assert_eq!(shell.panels.node_editor.snapshot().document, before.document);
    }

    #[test]
    fn node_wiring_pan_marquee_and_zoom_survive_acknowledged_frames() {
        let folder = tempfile::tempdir().unwrap();
        let (mut shell, canvas) = node_shell(folder.path().join("project"));
        let view = shell.panels.node_editor.view;
        let source = view.world_to_screen([204., 40.5], canvas);
        let target = view.world_to_screen([380., 40.5], canvas);
        shell.translate(Event::MousePressed { position: source, button: MouseButton::Left });
        shell.draw_commands([1200., 900.]);
        assert!(shell.draw_acknowledged([1200., 900.]).1.is_empty());
        assert!(shell.panels.node_editor.editing());
        shell.translate(Event::MouseReleased { position: target, button: MouseButton::Left });
        shell.draw_acknowledged([1200., 900.]);
        shell.draw_commands([1200., 900.]);
        assert_eq!(shell.panels.node_editor.document().connections,
            vec![motion_graph::Connection::new("number", "value", "math", "a")]);
        assert!(shell.panels.node_editor.undo());
        assert!(shell.panels.node_editor.document().connections.is_empty());

        let point = [canvas.min[0] + 20., canvas.min[1] + 20.];
        shell.translate(Event::MousePressed { position: point, button: MouseButton::Middle });
        shell.draw_commands([1200., 900.]);
        let moved = [point[0] + 30., point[1] + 40.];
        shell.translate(Event::MouseMoved { position: moved });
        shell.draw_commands([1200., 900.]);
        let pan = shell.panels.node_editor.view.pan;
        shell.draw_acknowledged([1200., 900.]);
        assert!(shell.panels.node_editor.editing());
        assert_eq!(shell.panels.node_editor.view.pan, pan);
        shell.translate(Event::MouseReleased { position: moved, button: MouseButton::Middle });
        shell.draw_commands([1200., 900.]);
        assert!(!shell.panels.node_editor.editing());

        let view = shell.panels.node_editor.view;
        let start = view.world_to_screen([-10., -10.], canvas);
        let end = view.world_to_screen([220., 160.], canvas);
        shell.translate(Event::MousePressed { position: start, button: MouseButton::Left });
        shell.draw_commands([1200., 900.]);
        shell.translate(Event::MouseMoved { position: end });
        shell.draw_commands([1200., 900.]);
        assert_eq!(shell.panels.node_editor.selected(), Some("number"));
        shell.draw_acknowledged([1200., 900.]);
        assert_eq!(shell.panels.node_editor.selected(), Some("number"));
        assert!(shell.panels.node_editor.editing());
        shell.translate(Event::MouseReleased { position: end, button: MouseButton::Left });
        shell.draw_commands([1200., 900.]);
        shell.input.modifiers.control = true;
        shell.translate(Event::MouseWheel { delta: [0., -20.] });
        shell.draw_commands([1200., 900.]);
        let zoom = shell.panels.node_editor.view.zoom;
        shell.draw_acknowledged([1200., 900.]);
        assert_eq!(shell.panels.node_editor.view.zoom, zoom);
        assert_ne!(zoom, view.zoom);
    }

    fn finish_actions(shell: &mut Shell) -> usize {
        let mut frames = 0;
        while let Some(action) = shell::actions::pop_action(&mut shell.actions) {
            shell.action(action, None).unwrap();
            if let Some((_, receive)) = shell.editing.take() {
                shell.accept_edit_state(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap());
                frames += usize::from(shell.present_due);
            }
        }
        frames
    }

    #[test]
    fn composition_menu_creates_in_the_existing_project_and_cancel_is_read_only() {
        for locale in [Locale::Zh, Locale::En] {
            let folder = tempfile::tempdir().unwrap();
            let mut shell = Shell::new(folder.path().join("project"), None, Some(locale)).unwrap();
            shell.home = None;
            shell.state = shell.engine.edit(serde_json::to_string(&Command::Add {
                layer: Layer::solid(7, "Keep this layer", [100.; 2], [100., 200., 0.], [1.; 4]) }).unwrap()).unwrap();
            let before = shell.state["project"].clone();
            let root = shell.state["root"].clone();
            for cancel in [true, false] {
                shell.panels.menu = Some(2);
                let x = 20. + shell.atlas.measure(locale.t(crate::i18n::Text::File)) + 22.
                    + shell.atlas.measure(locale.t(crate::i18n::Text::Edit)) + 22.;
                let point = [x, motion_ui::theme::metrics::TITLE_BAR + motion_ui::theme::metrics::MENU_BAR + 18.];
                shell.input = Input { mouse: point, pressed: Some((MouseButton::Left, point)), ..Default::default() };
                let (_, actions) = shell.draw_commands([1200., 900.]);
                assert!(matches!(actions.as_slice(), [panels::Action::NewComposition]));
                shell.actions.extend(actions);
                finish_actions(&mut shell);
                assert!(shell.dialog.is_some());
                shell.input = Input { keys: vec![(if cancel { Key::Escape } else { Key::Enter }, Modifiers::default())], ..Default::default() };
                assert!(shell.draw_commands([1200., 900.]).1.is_empty());
                assert!(shell.dialog.is_none());
                assert_eq!(shell.state["root"], root);
                if cancel {
                    assert_eq!(shell.state["project"], before);
                } else {
                    assert_eq!(shell.state["project"]["layers"], before["layers"]);
                    assert_eq!(shell.state["project"]["compositions"].as_array().unwrap().len(), 1);
                    shell.state = shell.engine.save().unwrap();
                    let reopened = motion_core::storage::load(&PathBuf::from(root.as_str().unwrap())).unwrap();
                    assert_eq!(serde_json::to_value(reopened).unwrap(), shell.state["project"]);
                    assert_eq!(shell.engine.history(0).unwrap()["project"], before);
                }
            }
        }
    }

    #[test]
    fn mask_timeline_drag_survives_acknowledgement_and_commits_the_latest_release() {
        use motion_core::masks::{LayerMask, MaskAction, MaskProperty};
        use motion_core::vector::{PathNode, VectorPath};
        for path in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
            shell.home = None;
            let mut layer = Layer::solid(7, "Masked", [100.; 2], [100., 200., 0.], [1.; 4]);
            layer.timeline = Some(motion_core::LayerTimeline {
                in_frame: 12,
                out_frame: 300,
                offset_frame: 12,
            });
            layer.masks.push(LayerMask::new(
                1,
                VectorPath {
                    id: 1,
                    closed: true,
                    nodes: [[10., 10.], [90., 10.], [50., 90.]]
                        .into_iter()
                        .enumerate()
                        .map(|(i, p)| PathNode {
                            id: i as u64 + 1,
                            geometry: motion_core::Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                        })
                        .collect(),
                },
            ));
            if path {
                for node in &mut layer.masks[0].path.nodes {
                    node.geometry.set_animated(9, true).unwrap();
                    node.geometry
                        .set_at(78, [40., 30., 0., 0., 0., 0.])
                        .unwrap();
                }
            }
            shell.state = shell
                .engine
                .edit(
                    serde_json::to_string(&vec![
                        Command::Add { layer },
                        Command::Mask {
                            object: 7,
                            action: MaskAction::Set {
                                mask: 1,
                                property: MaskProperty::Opacity,
                                frame: 21,
                                value: vec![50.],
                                animated: Some(true),
                            },
                        },
                        Command::Mask {
                            object: 7,
                            action: MaskAction::Set {
                                mask: 1,
                                property: MaskProperty::Opacity,
                                frame: 90,
                                value: vec![80.],
                                animated: None,
                            },
                        },
                    ])
                    .unwrap(),
                )
                .unwrap();
            let before = shell.state["project"].clone();
            let id = dock::Panel::Timeline.dock();
            shell.workspace.root = dock::Node::Dock {
                id,
                tabs: vec![id],
                active: 0,
            };
            shell.panels.selected = Some(7);
            shell.panels.focused = Some(dock::Panel::Timeline);
            shell.panels.timeline.expanded.insert(7);
            shell.panels.timeline.collapsed_transforms.insert(7);
            assert!(shell.draw_commands([1280., 700.]).1.is_empty());
            let body_top = shell.panels.timeline.work_area_band.unwrap().max[1];
            let point = |frame: f32| {
                [
                    640. + frame * 632. / 300.,
                    body_top + (if path { 3. } else { 4. }) * 22. + 11.,
                ]
            };
            shell.translate(Event::MousePressed {
                position: point(21.),
                button: MouseButton::Left,
            });
            let (_, actions) = shell.draw_commands([1280., 700.]);
            shell.actions.extend(actions);
            finish_actions(&mut shell);
            shell.translate(Event::MouseMoved {
                position: point(51.),
            });
            let (_, actions) = shell.draw_commands([1280., 700.]);
            assert!(matches!(
                &actions[..],
                [panels::Action::History(2), panels::Action::Edit(_)]
            ));
            shell.actions.extend(actions);
            shell.translate(Event::MouseReleased {
                position: point(69.),
                button: MouseButton::Left,
            });
            assert_eq!(finish_actions(&mut shell), 1);
            assert!(shell.draw_acknowledged([1280., 700.]).1.is_empty());
            assert!(shell.input.pending && shell.input.released.is_some());
            let (_, actions) = shell.draw_commands([1280., 700.]);
            assert!(matches!(
                &actions[..],
                [panels::Action::Edit(_), panels::Action::History(3)]
            ));
            shell.actions.extend(actions);
            finish_actions(&mut shell);
            let mask = &shell.state["project"]["layers"][0]["masks"][0];
            if path {
                for (index, node) in mask["path"]["nodes"].as_array().unwrap().iter().enumerate() {
                    assert_eq!(node["geometry"]["keys"][0]["frame"], 57);
                    assert_eq!(
                        node["geometry"]["keys"][1],
                        before["layers"][0]["masks"][0]["path"]["nodes"][index]["geometry"]["keys"][1]
                    );
                }
            } else {
                let keys = &mask["opacity"]["keys"];
                assert_eq!(keys[0]["frame"], 57);
                assert_eq!(
                    keys[1],
                    before["layers"][0]["masks"][0]["opacity"]["keys"][1]
                );
            }
            assert_eq!(shell.engine.history(0).unwrap()["project"], before);
        }
    }

    #[test]
    fn shape_content_timeline_drag_survives_acknowledgement_and_commits_the_latest_release() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.home = None;
        let mut layer = Layer::solid(7, "Masked", [100.; 2], [100., 200., 0.], [1.; 4]);
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 12,
            out_frame: 300,
            offset_frame: 12,
        });
        let grouped = motion_core::vector::groups::VectorGroup::wrap(
            motion_core::vector::VectorContent::shape(motion_core::vector::ShapeKind::Rectangle),
            layer.size,
        );
        let motion_core::vector::VectorSource::Group { mut group } = grouped.source else {
            unreachable!()
        };
        let mut r = motion_core::vector::groups::Repeater::default();
        r.copies.set_animated(9, true).unwrap();
        r.copies.set_at(78, 8.).unwrap();
        group
            .items
            .push(motion_core::vector::groups::GroupItem::Repeater {
                id: 3,
                name: "Repeater".into(),
                repeater: r,
            });
        layer.content = motion_core::Content::Vector {
            vector: motion_core::vector::VectorContent {
                source: motion_core::vector::VectorSource::Group { group },
                fill: None,
                stroke: None,
                trim: None,
                fill_rule: motion_core::vector::FillRule::NonZero,
            },
        };
        shell.state = shell
            .engine
            .edit(serde_json::to_string(&vec![Command::Add { layer }]).unwrap())
            .unwrap();
        let before = shell.state["project"].clone();
        let id = dock::Panel::Timeline.dock();
        shell.workspace.root = dock::Node::Dock {
            id,
            tabs: vec![id],
            active: 0,
        };
        shell.panels.selected = Some(7);
        shell.panels.focused = Some(dock::Panel::Timeline);
        shell.panels.timeline.expanded.insert(7);
        shell.panels.timeline.collapsed_transforms.insert(7);
        shell
            .panels
            .timeline
            .vector_open
            .extend([(7, 0, 0), (7, 1, 0), (7, 3, 2)]);
        assert!(panels::reveal_shape_keys(&mut shell.panels, &shell.state, 7).is_empty());
        assert!(shell.draw_commands([1280., 700.]).1.is_empty());
        let body_top = shell.panels.timeline.work_area_band.unwrap().max[1];
        let point = |frame: f32| {
            [
                640. + frame * 632. / 300.,
                body_top + 5. * 22. + 11.,
            ]
        };
        shell.translate(Event::MousePressed {
            position: point(21.),
            button: MouseButton::Left,
        });
        let (_, actions) = shell.draw_commands([1280., 700.]);
        shell.actions.extend(actions);
        finish_actions(&mut shell);
        shell.translate(Event::MouseMoved {
            position: point(51.),
        });
        let (_, actions) = shell.draw_commands([1280., 700.]);
        assert!(matches!(
            &actions[..],
            [panels::Action::History(2), panels::Action::Edit(_)]
        ));
        shell.actions.extend(actions);
        shell.translate(Event::MouseReleased {
            position: point(69.),
            button: MouseButton::Left,
        });
        assert_eq!(finish_actions(&mut shell), 1);
        assert!(shell.draw_acknowledged([1280., 700.]).1.is_empty());
        assert!(shell.input.pending && shell.input.released.is_some());
        let (_, actions) = shell.draw_commands([1280., 700.]);
        assert!(matches!(
            &actions[..],
            [panels::Action::Edit(_), panels::Action::History(3)]
        ));
        shell.actions.extend(actions);
        finish_actions(&mut shell);
        let keys = &shell.state["project"]["layers"][0]["content"]["vector"]["source"]["group"]
            ["items"][1]["repeater"]["copies"]["keys"];
        assert_eq!(keys[0]["frame"], 57);
        assert_eq!(
            keys[1],
            before["layers"][0]["content"]["vector"]["source"]["group"]["items"][1]["repeater"]
                ["copies"]["keys"][1]
        );
        assert_eq!(shell.engine.history(0).unwrap()["project"], before);
    }

    #[test]
    fn property_scrub_processes_a_gesture_batch_before_paint_and_commits_the_latest_release() {
        let folder = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(folder.path().join("project"), None, Some(Locale::En)).unwrap();
        shell.home = None;
        shell.state = shell.engine.edit(serde_json::to_string(&Command::Add {
            layer: Layer::solid(7, "Scrub", [100.; 2], [100., 200., 0.], [1.; 4]) }).unwrap()).unwrap();
        let before = shell.state["project"].clone();
        shell.panels.selected = Some(7);
        let id = dock::Panel::Properties.dock();
        shell.workspace.root = dock::Node::Dock { id, tabs: vec![id], active: 0 };
        let area = panels::frames(&shell.workspace.root, [400., 1000.])[0].content;
        let styles = motion_ui::style::default_stylesheet_arc();
        // Layer title, Masks, Transform and Anchor precede the Position row.
        let y = area.min[1] + 8. + 4. * styles.metric("row.height", 22.);
        // The key-navigation rail now precedes the property fields. Exercise
        // the rendered Position X field while retaining the gesture assertions.
        let value_x = area.min[0] + styles.metric("padding.panel", 8.) * 2.
            + styles.metric("tree.indent", 14.) + styles.metric("icon.slot", 20.) * 4.
            + styles.metric("label.column", 72.);
        let point = [value_x + 10., y + 10.];
        shell.translate(Event::MousePressed { position: point, button: MouseButton::Left });
        assert!(shell.draw_commands([400., 1000.]).1.is_empty());
        shell.translate(Event::MouseMoved { position: [point[0] + 40., point[1]] });
        let (_, actions) = shell.draw_commands([400., 1000.]);
        assert!(matches!(actions.as_slice(), [panels::Action::History(2), panels::Action::Edit(_)]));
        shell.actions.extend(actions);
        shell.translate(Event::MouseReleased { position: [point[0] + 60., point[1]], button: MouseButton::Left });
        assert_eq!(finish_actions(&mut shell), 1);
        assert_eq!(shell.state["sampledLayers"][0]["position"][0], 120.);
        assert!(shell.draw_acknowledged([400., 1000.]).1.is_empty());
        assert!(shell.input.pending && shell.input.released.is_some());
        let (_, actions) = shell.draw_commands([400., 1000.]);
        assert!(matches!(actions.as_slice(), [panels::Action::Edit(_), panels::Action::History(3)]));
        shell.actions.extend(actions);
        assert_eq!(finish_actions(&mut shell), 1);
        assert_eq!(shell.state["sampledLayers"][0]["position"][0], 130.);
        assert_eq!(shell.state["sampledLayers"][0]["position"][1], 200.);
        assert_eq!(shell.engine.history(0).unwrap()["project"], before);
    }
}

impl Shell {
    pub(crate) fn advance_playback(&mut self) -> Result<(), String> {
        if std::env::var_os("MOTION_TRACE_UI").is_some() {
            eprintln!("[motion-ui] content tick");
        }
        let Some((start, origin)) = self.playback else {
            return Ok(());
        };
        if self.audio_only_preview && self.audio.is_none() {
            let error = "audio-only preview lost its output";
            log::error!("audio preview failed phase=tick frame={} cause={error}", self.frame);
            return Err(error.into());
        }
        // The plan is frozen for this playback episode. Direct clock consumers
        // may initialize the episode lazily; regular TogglePlay sets it before
        // starting audio. Project edits and option changes stop the episode.
        let plan = match self.playback_plan {
            Some(plan) => plan,
            None => {
                let project = serde_json::from_value::<Project>(self.state["project"].clone())
                    .map_err(|error| error.to_string())?;
                let plan = crate::preview_playback::Plan::new(&project, self.workspace.preview_playback, origin)?;
                self.playback_plan = Some(plan);
                plan
            }
        };
        let fps = plan.fps;
        if let Some(audio) = &self.audio {
            let position = audio.frame(fps);
            let error = audio.error();
            self.frame = plan.display_frame(position.clamp(f64::from(plan.range.in_frame), f64::from(plan.range.out_frame - 1)));
            let finished = audio.finished(position, fps);
            if finished || error.is_some() {
                if finished { self.frame = f64::from(plan.range.out_frame - 1); }
                self.stop();
                if let Some(error) = error {
                    self.panels.message = Some(self.workspace.locale.error(&error));
                }
            }
        } else {
            let (frame, finished) = plan.position(start.elapsed());
            self.frame = frame;
            if finished { self.stop(); }
        }
        self.editing = Some((Instant::now(), if self.audio_only_preview {
            self.engine.seek_audio_preview_async(self.frame)?
        } else { self.engine.seek_async(self.frame)? }));
        let rate = (f64::from(fps) / f64::from(plan.step))
            .min(self.state["preview"]["fps"].as_f64().unwrap_or(60.))
            .clamp(1., 120.);
        self.next_composition = Instant::now() + Duration::from_secs_f64(1. / rate);
        Ok(())
    }
}

// One process-wide consecutive episode shared by move/resize persistence.
fn observe_geometry_save(result: Result<(), String>, path: &std::path::Path) {
    static FAILURES: crate::fallback_observation::Counter = crate::fallback_observation::Counter::new();
    match result {
        Ok(()) => FAILURES.success(),
        Err(error) => FAILURES.failure("window.geometry.save", format_args!("path={} reason={error} fallback=memory_only", crate::fallback_observation::short(path.display()))),
    }
}

#[cfg(test)]
mod geometry_observation_tests {
    use super::*;
    #[test]
    fn failed_geometry_persistence_is_bounded_and_success_resets_episode() {
        const CHILD: &str = "MOTION_GEOMETRY_SAVE_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let workspace = workspace::Workspace::default();
            let bad = root.join("directory");
            std::fs::create_dir(&bad).unwrap();
            for _ in 0..120 { observe_geometry_save(workspace.save(&bad), &bad); }
            let good = root.join("layout.json");
            observe_geometry_save(workspace.save(&good), &good);
            for _ in 0..120 { observe_geometry_save(workspace.save(&bad), &bad); }
            assert!(workspace::Workspace::load(&good).valid());
            assert!(bad.is_dir());
            let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
            let records = text.lines().filter(|l| l.contains("site=window.geometry.save ")).collect::<Vec<_>>();
            assert_eq!(records.len(),4);
            assert_eq!(records.iter().filter(|l| l.contains("ERROR") && l.contains("count=60 ")).count(),2);
            return;
        }
        let temp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","shell::presentation::geometry_observation_tests::failed_geometry_persistence_is_bounded_and_success_resets_episode"]).env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
        assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stderr).contains("site=window.geometry.save"));
    }
}
