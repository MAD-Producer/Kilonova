use super::*;

impl App {
    pub(super) fn capture(&mut self, id: WindowId, path: &std::path::Path) -> Result<(), String> {
        let (paint, logical, _) = self.paint(id);
        let window = &self.windows[&id];
        let gpu = self.gpu.as_mut().unwrap();
        let [width, height] = [window.config.width, window.config.height];
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("UI framework capture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let atlas = self.atlas.config();
        gpu.painter.upload_atlas(
            &gpu.device,
            &gpu.queue,
            self.atlas.pixel_data(),
            atlas.width,
            atlas.height,
        );
        let view = texture.create_view(&Default::default());
        let stride = (width * 4).div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: stride as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        clear(&mut encoder, &view);
        gpu.painter.render(
            &gpu.device,
            &gpu.queue,
            &mut encoder,
            &view,
            &paint,
            logical,
        ).map_err(|error|error.to_string())?;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        gpu.queue.submit(Some(encoder.finish()));
        let (send, receive) = std::sync::mpsc::channel();
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = send.send(result);
            });
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
        receive
            .recv()
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        let data = buffer.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
        let mut pixels = Vec::new();
        for row in data.chunks(stride as usize) {
            pixels.extend_from_slice(&row[..width as usize * 4]);
        }
        drop(data);
        buffer.unmap();
        if matches!(
            gpu.format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        ) {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        image::save_buffer(path, &pixels, width, height, image::ColorType::Rgba8)
            .map_err(|e| e.to_string())
    }
    pub(super) fn smoke_step(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let Some(path) = self.smoke.clone() else {
            return Ok(());
        };
        if self.start.elapsed().as_secs() > 30 {
            return Err("framework acceptance timed out".into());
        }
        let Some(main) = self
            .windows
            .iter()
            .find(|(_, w)| w.panel.is_none())
            .map(|(id, _)| *id)
        else {
            return Ok(());
        };
        if self.windows[&main].frames < 2 {
            return Ok(());
        };
        let layout = self
            .layouts(&self.windows[&main])
            .into_iter()
            .find(|p| p.panel == DockId(2));
        match self.stage {
            0 => {
                let r = field_rect(layout.unwrap().bounds, 0);
                let p = [r.min[0] + 10., r.min[1] + 10.];
                ui::accumulate(
                    &mut self.windows.get_mut(&main).unwrap().input,
                    motion_ui::input::Event::MousePressed {
                        position: p,
                        button: motion_ui::MouseButton::Left,
                    },
                );
            }
            1 => {
                let r = field_rect(layout.unwrap().bounds, 0);
                ui::accumulate(
                    &mut self.windows.get_mut(&main).unwrap().input,
                    motion_ui::input::Event::MouseMoved {
                        position: [r.min[0] + 30., r.min[1] + 10.],
                    },
                );
            }
            2 => {
                self.secondary_buttons_probe(main, event_loop, &path.with_extension("input-main.png"))?;
                let input = &mut self.windows.get_mut(&main).unwrap().input;
                let position = input.mouse;
                ui::accumulate(
                    input,
                    motion_ui::input::Event::MouseReleased {
                        position,
                        button: motion_ui::MouseButton::Left,
                    },
                );
            }
            3 => {
                if self.model.borrow().undo.len() != 1 || self.model.borrow().value != 35. {
                    return Err("numeric drag did not produce one undo step".into());
                }
                self.apply(vec![Action::Undo]);
                self.verified_undo = self.model.borrow().value == 25.;
                self.tab_probe(main, event_loop)?;
                self.detach(DockId(2), event_loop)?;
            }
            4 => {
                let Some(id) = self
                    .windows
                    .iter()
                    .find(|(_, w)| w.panel == Some(DockId(2)) && w.frames >= 2)
                    .map(|(id, _)| *id)
                else {
                    return Ok(());
                };
                self.floating_input_probe(id, event_loop, &path)?;
                self.capture(id, &path.with_extension("floating.png"))?;
                self.redock(DockId(2))?;
            }
            5 => {
                let mut style_checks = serde_json::Value::Null;
                if self.style_path.is_some() {
                    let before = self.model.borrow().clone();
                    self.reload_style()?;
                    if *self.model.borrow() != before {
                        return Err("stylesheet reload edited UI model or undo history".into());
                    }
                    let old_style = self
                        .host
                        .styles()
                        .resolve("panel", States::default())
                        .unwrap();
                    let old_font = self.host.styles().font_family().to_owned();
                    let original = self.style_path.take();
                    let invalid = path.with_extension("invalid-style.json");
                    std::fs::write(&invalid, b"invalid json").map_err(|e| e.to_string())?;
                    self.style_path = Some(invalid.clone());
                    let rejected = self.reload_style().is_err();
                    self.style_path = original;
                    let _ = std::fs::remove_file(invalid);
                    if !rejected
                        || self
                            .host
                            .styles()
                            .resolve("panel", States::default())
                            .unwrap()
                            != old_style
                        || self.host.styles().font_family() != old_font
                        || *self.model.borrow() != before
                    {
                        return Err("invalid stylesheet replaced live state".into());
                    }
                    style_checks = serde_json::json!({"reloadPreservedModel":true,"invalidReloadKeptStyle":true});
                }
                if !self.workspace.valid_for(self.host.registry()) || !self.verified_undo {
                    return Err("workspace or undo validation failed".into());
                }
                self.workspace.save(&path.with_extension("layout.json"))?;
                let reopened = Workspace::load(&path.with_extension("layout.json"));
                if !reopened.valid_for(self.host.registry()) {
                    return Err("workspace failed to reopen".into());
                }
                self.capture(main, &path.with_extension("png"))?;
                let info = self.gpu.as_ref().unwrap().adapter.get_info();
                std::fs::write(&path,serde_json::to_vec_pretty(&serde_json::json!({"styles":style_checks,"ok":true,"adapter":info.name,"backend":format!("{:?}",info.backend),"independentUiOnly":true,"numericDragOneUndo":true,"undoRestored":true,"floatingPresented":true,"layoutRoundtrip":true,"secondaryButtonsPreservedGesture":true,"floatingCancellationRestored":true,"tabNavigationGuarded":true,"language":if self.language==Language::Chinese{"zh"}else{"en"},"screenshot":path.with_extension("png")})).unwrap()).map_err(|e|e.to_string())?;
                event_loop.exit();
            }
            _ => {}
        }
        self.stage += 1;
        Ok(())
    }
}
