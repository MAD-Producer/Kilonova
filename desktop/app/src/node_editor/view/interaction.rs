use super::*;

impl NodeEditor {
    pub(super) fn keyboard(&mut self, input: &Input, actions: &mut Vec<Action>) {
        if !self.focus {
            return;
        }
        // File shortcuts commit active text instead of silently swallowing the request.
        for (key, modifiers) in &input.keys {
            if modifiers.control && matches!(key, Key::O | Key::S) {
                let request = if *key == Key::O {
                    Request::Open
                } else if modifiers.shift {
                    Request::SaveAs
                } else {
                    Request::Save
                };
                if let Ok(action) = self.request(request) {
                    actions.push(action);
                }
                return;
            }
        }
        if self.search_focus {
            if !input.text.is_empty() {
                self.search.push_str(&input.text);
                self.library_scroll = 0.;
            }
            for (key, _) in &input.keys {
                match key {
                    Key::Backspace => {
                        self.search.pop();
                        self.library_scroll = 0.;
                    }
                    Key::Escape | Key::Enter => self.search_focus = false,
                    _ => {}
                }
            }
            return;
        }
        if self.field.is_some() {
            self.field_input(input);
            return;
        }
        for (key, modifiers) in &input.keys {
            match key {
                Key::Escape => self.cancel_edit_only(),
                Key::Delete | Key::Backspace if self.drag.is_none() => {
                    self.delete_selected();
                }
                // 框架所有节点（Home 或 F 键）
                Key::Home | Key::F if !modifiers.control && !modifiers.shift => {
                    self.fit_all(self.last_canvas);
                }
                // Ctrl+A 全选节点
                Key::A if modifiers.control => {
                    self.selected = self.document.nodes.iter().map(|n| n.id.clone()).collect();
                    self.primary = self.document.nodes.first().map(|n| n.id.clone());
                }
                // 撤销/重做
                Key::Z if modifiers.control => {
                    if modifiers.shift {
                        self.redo();
                    } else {
                        self.undo();
                    }
                }
                Key::Y if modifiers.control => {
                    self.redo();
                }
                // Ctrl+D 复制节点
                Key::D if modifiers.control && !self.selected.is_empty() => {
                    // TODO: 实现节点复制功能
                    // self.duplicate_selected();
                }
                // Keep Space available to the shared node preview transport.
                Key::A if modifiers.shift && !modifiers.control => {
                    self.library_open = true;
                }
                // G 切换网格显示
                Key::G if !modifiers.control => {
                    // TODO: 添加网格显示开关状态
                }
                // N 切换迷你地图
                Key::N if !modifiers.control => {
                    // TODO: 添加迷你地图开关状态
                }
                // Ctrl+0 重置缩放
                Key::Num0 if modifiers.control => {
                    self.view.zoom = 1.0;
                }
                _ => {}
            }
        }
    }
    pub(super) fn canvas_input(&mut self, cards: &[Card], canvas: Rect, input: &Input) {
        if canvas.contains(input.mouse) && self.drag.is_none() && input.wheel != [0.; 2] {
            if input.modifiers.alt {
                self.view.zoom_at(
                    self.view.zoom * (-input.wheel[1] * 0.004).exp(),
                    input.mouse,
                    canvas,
                );
            } else if input.modifiers.shift {
                self.view.pan[0] -= input.wheel[1] + input.wheel[0];
            } else {
                self.view.pan[0] -= input.wheel[0];
                self.view.pan[1] -= input.wheel[1];
            }
        }
        let socket = |point: [f32; 2]| {
            cards
                .iter()
                .rev()
                .flat_map(|c| c.sockets.iter())
                .find(|s| {
                    (s.point[0] - point[0]).abs() <= 8. && (s.point[1] - point[1]).abs() <= 8.
                })
                .cloned()
        };
        if let Some((button, point)) = input.pressed.filter(|(_, p)| canvas.contains(*p)) {
            if self.commit_field().is_err() {
                return;
            }
            match button {
                MouseButton::Middle => {
                    self.drag = Some(Drag::Pan {
                        origin: point,
                        pan: self.view.pan,
                        button,
                    })
                }
                MouseButton::Left => {
                    if input.modifiers.alt {
                        self.drag = Some(Drag::Pan {
                            origin: point,
                            pan: self.view.pan,
                            button,
                        });
                    } else if let Some(s) = socket(point) {
                        if s.output {
                            self.drag = Some(Drag::Wire {
                                source: s.port,
                                old_input: None,
                            });
                        } else if let Some(edge) =
                            self.document.connections.iter().find(|c| c.to == s.port)
                        {
                            self.drag = Some(Drag::Wire {
                                source: edge.from.clone(),
                                old_input: Some(edge.to.clone()),
                            });
                        } else {
                            self.drag = Some(Drag::WireInput { target: s.port });
                        }
                    } else if let Some(card) = cards.iter().rev().find(|c| c.rect.contains(point)) {
                        if !self.selected.contains(&card.id) || input.modifiers.shift {
                            self.select(Some(&card.id), input.modifiers.shift);
                        } else {
                            self.primary = Some(card.id.clone());
                        }
                        if self.selected.contains(&card.id) {
                            let positions = self
                                .document
                                .nodes
                                .iter()
                                .filter(|n| self.selected.contains(&n.id))
                                .map(|n| (n.id.clone(), n.position))
                                .collect();
                            self.drag = Some(Drag::Nodes {
                                origin: self.view.screen_to_world(point, canvas),
                                positions,
                                before: self.state(false),
                            });
                        }
                    } else {
                        self.drag = Some(Drag::Marquee {
                            origin: point,
                            current: point,
                            base: self.selected.clone(),
                            primary: self.primary.clone(),
                            extend: input.modifiers.shift,
                            toggle: input.modifiers.control,
                        });
                    }
                }
                MouseButton::Right => {
                    // 检查是否点击了输入端口（断开连接）
                    if let Some(s) = socket(point).filter(|s| !s.output) {
                        self.disconnect_input(&s.port);
                    }
                    // 检查是否点击了连线（断开连接）
                    else {
                        let edge = self
                            .document
                            .connections
                            .iter()
                            .find(|edge| {
                                let a = cards
                                    .iter()
                                    .flat_map(|c| c.sockets.iter())
                                    .find(|s| s.output && s.port == edge.from);
                                let b = cards
                                    .iter()
                                    .flat_map(|c| c.sockets.iter())
                                    .find(|s| !s.output && s.port == edge.to);
                                a.zip(b).is_some_and(|(a, b)| {
                                    curve(a.point, b.point)
                                        .windows(2)
                                        .any(|p| distance_segment(point, p[0], p[1]) < 6.)
                                })
                            })
                            .map(|e| e.to.clone());

                        if let Some(input) = edge {
                            self.disconnect_input(&input);
                        }
                        // 检查是否点击了节点（显示节点菜单）
                        else if let Some(card) = cards.iter().find(|c| c.rect.contains(point)) {
                            self.context_menu = Some(ContextMenu {
                                position: point,
                                target_node: Some(card.id.clone()),
                            });
                        }
                        // 点击空白处（显示画布菜单）
                        else {
                            self.context_menu = Some(ContextMenu {
                                position: point,
                                target_node: None,
                            });
                        }
                    }
                }
            }
        }
        if let Some(drag) = self.drag.take() {
            match drag {
                Drag::Pan {
                    origin,
                    pan,
                    button,
                } => {
                    self.view.pan = [
                        pan[0] + input.mouse[0] - origin[0],
                        pan[1] + input.mouse[1] - origin[1],
                    ];
                    if input.held == Some(button) {
                        self.drag = Some(Drag::Pan {
                            origin,
                            pan,
                            button,
                        });
                    }
                }
                Drag::Marquee {
                    origin,
                    base,
                    primary,
                    extend,
                    toggle,
                    ..
                } => {
                    let current = [
                        input.mouse[0].clamp(canvas.min[0], canvas.max[0]),
                        input.mouse[1].clamp(canvas.min[1], canvas.max[1]),
                    ];
                    let selection = selection_rect(origin, current);
                    let hits: BTreeSet<_> = cards
                        .iter()
                        .filter(|card| {
                            (selection.width() >= 3. || selection.height() >= 3.)
                                && intersects(card.rect, selection)
                                && intersects(card.rect, canvas)
                        })
                        .map(|card| card.id.clone())
                        .collect();
                    self.selected = if toggle {
                        base.symmetric_difference(&hits).cloned().collect()
                    } else if extend {
                        base.union(&hits).cloned().collect()
                    } else {
                        hits
                    };
                    self.primary = primary
                        .clone()
                        .filter(|id| self.selected.contains(id))
                        .or_else(|| self.selected.iter().next().cloned());
                    if input.held == Some(MouseButton::Left) {
                        self.drag = Some(Drag::Marquee {
                            origin,
                            current,
                            base,
                            primary,
                            extend,
                            toggle,
                        });
                    } else if !input
                        .released
                        .is_some_and(|(button, _)| button == MouseButton::Left)
                    {
                        self.selected = base;
                        self.primary = primary;
                    }
                }
                Drag::Nodes {
                    origin,
                    positions,
                    before,
                } => {
                    let point = self.view.screen_to_world(input.mouse, canvas);
                    for (id, start) in &positions {
                        if let Some(node) = self.document.nodes.iter_mut().find(|n| n.id == *id) {
                            node.position = [
                                (start[0] + point[0] - origin[0]).clamp(-1.0e6, 1.0e6),
                                (start[1] + point[1] - origin[1]).clamp(-1.0e6, 1.0e6),
                            ];
                        }
                    }
                    if input.held == Some(MouseButton::Left) {
                        self.drag = Some(Drag::Nodes {
                            origin,
                            positions,
                            before,
                        });
                    } else if input.released.is_some_and(|(b, _)| b == MouseButton::Left) {
                        if self.document != before.document {
                            self.finish_edit(before);
                        }
                    } else {
                        self.document = before.document;
                    }
                }
                Drag::Wire { source, old_input } => {
                    if let Some((MouseButton::Left, point)) = input.released {
                        if let Some(target) =
                            socket(point).filter(|s| !s.output && canvas.contains(point))
                        {
                            if let Err(error) =
                                self.connect_replacing(source, target.port, old_input.as_ref())
                            {
                                self.notice = Some(error.to_string());
                            }
                        } else if let Some(input) = old_input.filter(|_| canvas.contains(point)) {
                            self.disconnect_input(&input);
                        }
                    } else if input.held == Some(MouseButton::Left) {
                        self.drag = Some(Drag::Wire { source, old_input });
                    }
                }
                Drag::WireInput { target } => {
                    if let Some((MouseButton::Left, point)) = input.released {
                        if let Some(source) =
                            socket(point).filter(|s| s.output && canvas.contains(point))
                        {
                            if let Err(error) = self.connect_ports(source.port, target) {
                                self.notice = Some(error.to_string());
                            }
                        }
                    } else if input.held == Some(MouseButton::Left) {
                        self.drag = Some(Drag::WireInput { target });
                    }
                }
            }
        }
    }
}
