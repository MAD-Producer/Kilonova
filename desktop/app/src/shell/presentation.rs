use crate::*;

impl Shell {
    pub(crate) fn playback_ready(&self, now: Instant) -> bool {
        self.playback.is_some()
            && !playback_input_blocks(&self.input)
            && self
                .floating
                .values()
                .all(|window| !playback_input_blocks(&window.input))
            && now >= self.next_composition
            && self.attached
            && self.rendering.is_empty()
            && self.editing.is_none()
            && !self.present_due
            && self.actions.is_empty()
            && self.resizing.is_none()
            && self.resize_queue.is_empty()
    }
    pub(crate) fn lose_focus(&mut self) {
        self.workspace.cancel();
        self.actions
            .extend(panels::cancel_interaction(&mut self.panels));
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
                &self.workspace.root,
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
        if let Some(dialog) = &mut self.dialog {
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
                        let _ = self.schedule_node_compile();
                    }
                    _ if !self.node_close_current(&snapshot) => {
                        self.panels
                            .node_editor
                            .report_error(self.workspace.locale.t(T::NodeStaleClose));
                        let _ = self.schedule_node_compile();
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
        let view = panels::composition_view(&self.workspace.root, size);
        UiFrame {
            paint,
            atlas,
            logical_size: size,
            window: 0,
            node_preview: self
                .node_viewport(panels::node_view(&self.workspace.root, size), self.scale),
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
            let neutral = Input {
                mouse: self.input.mouse,
                modifiers: self.input.modifiers,
                ..Default::default()
            };
            let input = std::mem::replace(&mut self.input, neutral);
            let (paint, actions) = self.draw_commands(size);
            self.actions.extend(actions);
            self.input = input;
            let ui = self.ui_frame(paint, size, false);
            self.rendering.insert(0, self.engine.present_async(ui)?);
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
            || self.panels.panel_menu.is_some()
            || self.panels.menu.is_some()
            || self.panels.fx_console_in(None)
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
        let (mut paint, actions) = self.draw_commands(size);
        self.actions.extend(actions);
        if !self.actions.is_empty() {
            self.engine.recycle_paint(paint);
            return Ok(());
        }
        // Actions may change project, layer selection or playhead during this draw.
        if self.dirty
            && self.home.is_none()
            && self.workspace_manager.is_none()
            && self.dialog.is_none()
            && !self.project_modal()
            && self.node_close_prompt.is_none()
            && self.node_examples.is_none()
        {
            let neutral = Input {
                mouse: self.input.mouse,
                modifiers: self.input.modifiers,
                ..Default::default()
            };
            paint.clear();
            let actions = panels::chrome(
                &mut paint,
                &mut self.atlas,
                size,
                &self.workspace.root,
                &mut self.panels,
                &self.state,
                &neutral,
            );
            self.actions.extend(actions);
        }
        let ui = self.ui_frame(paint, size, false);
        self.rendering.insert(0, self.engine.present_async(ui)?);
        self.dirty = false;
        Ok(())
    }
    pub(crate) fn float_panel(
        &mut self,
        id: dock::DockId,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        if self.floating.values().any(|f| f.panel.dock() == id) {
            return Ok(());
        }
        let panel = panels::panel(id).ok_or("invalid panel")?;
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
        if self
            .panels
            .fx_console
            .as_ref()
            .is_some_and(|c| c.owner.is_some_and(|owner| owner.dock() == panel))
        {
            self.panels.fx_console = None;
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
        if (!self.actions.is_empty() && !self.present_due)
            || self.editing.is_some()
            || !self.attached
            || self.resizing.is_some()
            || !self.resize_queue.is_empty()
        {
            return Ok(());
        }
        let project_modal = self.project_modal();
        let f = self.floating.get_mut(&id).ok_or("floating window closed")?;
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
        let neutral = Input {
            mouse: f.input.mouse,
            modifiers: f.input.modifiers,
            ..Default::default()
        };
        let acknowledged = self.present_due;
        let blocked = acknowledged
            || project_modal
            || self.dialog.is_some()
            || self.node_examples.is_some()
            || self.node_close_prompt.is_some();
        let actions = panels::floating_chrome(
            &mut paint,
            &mut self.atlas,
            panel,
            size,
            &mut self.panels,
            &self.state,
            if blocked { &neutral } else { &f.input },
        );
        f.window.set_cursor(
            self.cursors
                .get(self.panels.cursor.unwrap_or_default(), scale),
        );
        self.actions.extend(actions);
        if !acknowledged {
            f.input.begin_frame();
        }
        if !self.actions.is_empty() && !acknowledged {
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
        let composition = panels::floating_view(panel, size).map(|v| {
            [
                v.min[0] * scale,
                v.min[1] * scale,
                v.width() * scale,
                v.height() * scale,
            ]
        });
        self.rendering.insert(
            key,
            self.engine.present_async(UiFrame {
                paint,
                atlas,
                logical_size: size,
                composition,
                window: key,
                node_preview: self.node_viewport(panels::floating_node_view(panel, size), scale),
            })?,
        );
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
                let position = f.window.outer_position().ok().map(|p| [p.x, p.y]);
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
                    let _ = self.workspace.save(&self.layout_path);
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
                    let _ = self.workspace.save(&self.layout_path);
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
            WindowEvent::CursorMoved { position, .. } => Some(Event::MouseMoved {
                position: [position.x as f32 / f.scale, position.y as f32 / f.scale],
            }),
            WindowEvent::DroppedFile(path)
                if path
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("msfx")) =>
            {
                self.actions
                    .push_back(panels::Action::ImportEffectPath(path));
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
                self.actions
                    .extend(panels::cancel_interaction(&mut self.panels));
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

impl Shell {
    pub(crate) fn advance_playback(&mut self) -> Result<(), String> {
        if std::env::var_os("MOTION_TRACE_UI").is_some() {
            eprintln!("[motion-ui] content tick");
        }
        let Some((start, origin)) = self.playback else {
            return Ok(());
        };
        let fps = self.state["project"]["fps"].as_u64().unwrap_or(30) as u32;
        let frames = self.state["project"]["frames"].as_u64().unwrap_or(1) as u32;
        if let Some(audio) = &self.audio {
            let position = audio.frame(fps);
            let error = audio.error();
            self.frame = position.min(f64::from(frames - 1));
            if position >= f64::from(frames) - f64::from(fps) / 48000. || error.is_some() {
                self.stop();
                if let Some(error) = error {
                    self.panels.message = Some(self.workspace.locale.error(&error));
                }
            }
        } else {
            self.frame = playback_frame(origin, start.elapsed(), fps, frames);
        }
        self.editing = Some((Instant::now(), self.engine.seek_async(self.frame)?));
        let rate = (fps as f64)
            .min(self.state["preview"]["fps"].as_f64().unwrap_or(60.))
            .clamp(1., 120.);
        self.next_composition = Instant::now() + Duration::from_secs_f64(1. / rate);
        Ok(())
    }
}
