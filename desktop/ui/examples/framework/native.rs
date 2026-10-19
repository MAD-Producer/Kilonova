use super::*;

impl App {
    pub(super) fn redraw(
        &mut self,
        id: WindowId,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        let mut pending_float = None;
        if self.windows[&id].panel.is_none() {
            let window = &self.windows[&id];
            let size = window.window.inner_size();
            let scale = window.window.scale_factor() as f32;
            let input = window.input.clone();
            if let Some(change) = self.workspace.interact(
                &input,
                Rect::new(
                    0.,
                    0.,
                    size.width as f32 / scale,
                    size.height as f32 / scale,
                ),
            ) {
                match change {
                    motion_ui::workspace::Change::Float(panel) => pending_float = Some(panel),
                    motion_ui::workspace::Change::Changed => {
                        self.windows.get_mut(&id).unwrap().dirty = true;
                    }
                }
            }
        }
        self.windows.get_mut(&id).unwrap().dirty = false;
        let (paint, size, float) = self.paint(id);
        let float = float.or(pending_float);
        let gpu = self.gpu.as_mut().unwrap();
        let window = self.windows.get_mut(&id).unwrap();
        let atlas = self.atlas.config();
        if self.atlas.take_dirty() {
            gpu.painter.upload_atlas(
                &gpu.device,
                &gpu.queue,
                self.atlas.pixel_data(),
                atlas.width,
                atlas.height,
            );
        }
        let frame = match window.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                window.surface.configure(&gpu.device, &window.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return Ok(()),
            wgpu::CurrentSurfaceTexture::Validation => return Err("surface acquisition validation failed".into()),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        clear(&mut encoder, &view);
        gpu.painter
            .render(&gpu.device, &gpu.queue, &mut encoder, &view, &paint, size).map_err(|error|error.to_string())?;
        gpu.queue.submit(Some(encoder.finish()));
        gpu.queue.present(frame);
        window.frames += 1;
        if let Some(panel) = float {
            if window.panel.is_some() {
                self.redock(panel)?;
            } else {
                self.detach(panel, event_loop)?;
            }
        }
        Ok(())
    }
    pub(super) fn detach(
        &mut self,
        panel: DockId,
        event_loop: &ActiveEventLoop,
    ) -> Result<(), String> {
        let descriptor = self.host.registry().get(panel).ok_or("unknown panel")?;
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(descriptor.title.get(self.language))
                        .with_inner_size(winit::dpi::LogicalSize::new(
                            descriptor.floating_size[0],
                            descriptor.floating_size[1],
                        ))
                        .with_visible(self.smoke.is_none()),
                )
                .map_err(|e| e.to_string())?,
        );
        let gpu = self.gpu.as_ref().unwrap();
        window.set_ime_allowed(true);
        let surface = gpu
            .instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let config = config(&surface, &gpu.adapter, gpu.format, size.width, size.height);
        surface.configure(&gpu.device, &config);
        self.router.forget_panel(0, panel);
        let actions = self
            .host
            .lifecycle(panel, Lifecycle::Detached { window: 0 });
        self.apply(actions);
        if !self.workspace.float(panel) {
            return Err("panel does not have a dock owner".into());
        }
        self.windows.insert(
            window.id(),
            NativeWindow {
                window,
                surface,
                config,
                key: panel.0 as u64,
                panel: Some(panel),
                input: Default::default(),
                frames: 0,
                dirty: true,
            },
        );
        Ok(())
    }
    pub(super) fn redock(&mut self, panel: DockId) -> Result<(), String> {
        if let Some(id) = self
            .windows
            .iter()
            .find(|(_, w)| w.panel == Some(panel))
            .map(|(id, _)| *id)
        {
            let key = self.windows[&id].key;
            self.router.forget_panel(key, panel);
            let actions = self
                .host
                .lifecycle(panel, Lifecycle::Detached { window: key });
            self.apply(actions);
            self.windows.remove(&id);
        }
        let target = if panel != DockId(3) && self.workspace.root.find(DockId(3)).is_some() {
            DockId(3)
        } else {
            dock::solve(&self.workspace.root, Rect::new(0., 0., 1280., 800.))
                .first()
                .map_or(panel, |p| p.id)
        };
        if !self.workspace.redock_to(panel, target, Some(Edge::Right)) {
            return Err("could not redock panel".into());
        }
        Ok(())
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        let result = (|| -> Result<(), String> {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title("Motion Studio · UI Framework")
                            .with_inner_size(winit::dpi::LogicalSize::new(self.initial_size[0], self.initial_size[1]))
                            .with_visible(self.smoke.is_none()),
                    )
                    .map_err(|e| e.to_string())?,
            );
            window.set_ime_allowed(true);
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::VULKAN | wgpu::Backends::DX12 | wgpu::Backends::METAL,
                ..wgpu::InstanceDescriptor::new_without_display_handle()
            });
            let surface = instance
                .create_surface(window.clone())
                .map_err(|e| e.to_string())?;
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    compatible_surface: Some(&surface),
                    ..Default::default()
                }))
                .map_err(|e| e.to_string())?;
            let (device, queue) = pollster::block_on(adapter.request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("UI framework"),
                    required_features: wgpu::Features::empty(),
                    required_limits:
                        wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    ..Default::default()
                },
            ))
            .map_err(|e| e.to_string())?;
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            let size = window.inner_size();
            let config = config(&surface, &adapter, format, size.width, size.height);
            surface.configure(&device, &config);
            let painter = Painter::new(&device, format);
            self.gpu = Some(Gpu {
                instance,
                adapter,
                device,
                queue,
                painter,
                format,
            });
            self.windows.insert(
                window.id(),
                NativeWindow {
                    window,
                    surface,
                    config,
                    key: 0,
                    panel: None,
                    input: Default::default(),
                    frames: 0,
                    dirty: true,
                },
            );
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if !self.windows.contains_key(&id) {
            return;
        }
        if matches!(&event,WindowEvent::KeyboardInput{event,..} if event.state==ElementState::Pressed && self.windows[&id].input.modifiers.control && matches!(&event.logical_key,WKey::Character(key) if key.eq_ignore_ascii_case("r")))
        {
            if let Err(error) = self.reload_style() {
                self.style_status = Some(error);
            }
            for window in self.windows.values_mut() {
                window.dirty = true;
            }
            return;
        }
        match event {
            WindowEvent::CloseRequested => {
                if let Some(panel) = self.windows[&id].panel {
                    if let Err(e) = self.redock(panel) {
                        self.error = Some(e);
                        event_loop.exit();
                    }
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    let gpu = self.gpu.as_ref().unwrap();
                    let w = self.windows.get_mut(&id).unwrap();
                    w.config.width = size.width;
                    w.config.height = size.height;
                    w.surface.configure(&gpu.device, &w.config);
                    w.dirty = true;
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw(id, event_loop) {
                    self.error = Some(e);
                    event_loop.exit();
                }
            }
            event => {
                let w = self.windows.get_mut(&id).unwrap();
                let scale = w.window.scale_factor() as f32;
                w.dirty = true;
                let mapped = match event {
                    WindowEvent::CursorMoved { position, .. } => {
                        Some(motion_ui::input::Event::MouseMoved {
                            position: [position.x as f32 / scale, position.y as f32 / scale],
                        })
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        let button = match button {
                            winit::event::MouseButton::Left => motion_ui::MouseButton::Left,
                            winit::event::MouseButton::Right => motion_ui::MouseButton::Right,
                            _ => motion_ui::MouseButton::Middle,
                        };
                        Some(if state == ElementState::Pressed {
                            motion_ui::input::Event::MousePressed {
                                position: w.input.mouse,
                                button,
                            }
                        } else {
                            motion_ui::input::Event::MouseReleased {
                                position: w.input.mouse,
                                button,
                            }
                        })
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        Some(motion_ui::input::Event::MouseWheel {
                            delta: match delta {
                                MouseScrollDelta::LineDelta(x, y) => [-x * 32., -y * 32.],
                                MouseScrollDelta::PixelDelta(p) => {
                                    [-p.x as f32 / scale, -p.y as f32 / scale]
                                }
                            },
                        })
                    }
                    WindowEvent::ModifiersChanged(m) => {
                        let m = m.state();
                        w.input.modifiers = motion_ui::input::Modifiers {
                            control: m.control_key() || m.super_key(),
                            shift: m.shift_key(),
                            alt: m.alt_key(),
                        };
                        None
                    }
                    WindowEvent::KeyboardInput { event, .. }
                        if event.state == ElementState::Pressed =>
                    {
                        let key = match &event.logical_key {
                            WKey::Named(NamedKey::Enter) => Some(motion_ui::Key::Enter),
                            WKey::Named(NamedKey::Escape) => Some(motion_ui::Key::Escape),
                            WKey::Named(NamedKey::Tab) => Some(motion_ui::Key::Tab),
                            WKey::Named(NamedKey::Backspace) => Some(motion_ui::Key::Backspace),
                            WKey::Named(NamedKey::Delete) => Some(motion_ui::Key::Delete),
                            WKey::Named(NamedKey::ArrowLeft) => Some(motion_ui::Key::Left),
                            WKey::Named(NamedKey::ArrowRight) => Some(motion_ui::Key::Right),
                            WKey::Named(NamedKey::Home) => Some(motion_ui::Key::Home),
                            WKey::Named(NamedKey::End) => Some(motion_ui::Key::End),
                            WKey::Character(c) if c.eq_ignore_ascii_case("a") => {
                                Some(motion_ui::Key::A)
                            }
                            _ => None,
                        };
                        if let Some(key) = key {
                            let modifiers = w.input.modifiers;
                            ui::accumulate(
                                &mut w.input,
                                motion_ui::input::Event::KeyPressed { key, modifiers },
                            );
                        }
                        event
                            .text
                            .filter(|_| !w.input.modifiers.control)
                            .map(|t| motion_ui::input::Event::TextInput(t.to_string()))
                    }
                    WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                        Some(motion_ui::input::Event::TextInput(text))
                    }
                    WindowEvent::Focused(false) => Some(motion_ui::input::Event::FocusLost),
                    _ => None,
                };
                if let Some(event) = mapped {
                    ui::accumulate(&mut w.input, event);
                }
            }
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.smoke.is_some() {
            let ids: Vec<_> = self.windows.keys().copied().collect();
            for id in ids {
                if let Err(e) = self.redraw(id, event_loop) {
                    self.error = Some(e);
                    event_loop.exit();
                    return;
                }
            }
            if let Err(e) = self.smoke_step(event_loop) {
                self.error = Some(e);
                event_loop.exit();
            }
        }
        for window in self.windows.values() {
            if window.dirty {
                window.window.request_redraw();
            }
        }
        event_loop.set_control_flow(if self.smoke.is_some() {
            ControlFlow::WaitUntil(Instant::now() + std::time::Duration::from_millis(16))
        } else {
            ControlFlow::Wait
        });
    }
}
