use super::*;
use crate::shortcuts::{self, Command, Scope};

impl NodeEditor {
    pub(super) fn keyboard(&mut self, input: &Input, actions: &mut Vec<Action>) {
        if !self.focus {
            return;
        }
        // File shortcuts commit active text instead of silently swallowing the request.
        for (key, modifiers, repeat) in input.key_presses() {
            if let Some(command @ (Command::Open | Command::Save | Command::SaveAs)) =
                shortcuts::resolve(Scope::Nodes, key, modifiers, repeat)
            {
                let request = match command {
                    Command::Open => Request::Open,
                    Command::SaveAs => Request::SaveAs,
                    _ => Request::Save,
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
        for (key, modifiers, repeat) in input.key_presses() {
            if key == Key::Escape && !repeat {
                self.cancel_edit_only();
                self.add_menu = None;
                continue;
            }
            if self.drag.is_some() {
                continue;
            }
            let Some(command) = shortcuts::resolve(Scope::Nodes, key, modifiers, repeat) else {
                continue;
            };
            match command {
                Command::Delete => {
                    self.delete_selected();
                }
                Command::FitAll => {
                    self.fit_all(self.last_canvas);
                }
                Command::FitSelection => self.fit_selected(self.last_canvas),
                Command::SelectAll => {
                    self.selected = self.document.nodes.iter().map(|n| n.id.clone()).collect();
                    self.primary = self.document.nodes.first().map(|n| n.id.clone());
                }
                Command::DeselectAll => {
                    self.selected.clear();
                    self.primary = None;
                }
                Command::Undo => {
                    self.undo();
                }
                Command::Redo => {
                    self.redo();
                }
                Command::Duplicate => {
                    if let Err(error) = self.duplicate_selected() {
                        self.notice = Some(error.to_string());
                    }
                }
                Command::AddNode => {
                    let position = if self.last_canvas.contains(input.mouse) {
                        input.mouse
                    } else {
                        [
                            (self.last_canvas.min[0] + self.last_canvas.max[0]) * 0.5,
                            (self.last_canvas.min[1] + self.last_canvas.max[1]) * 0.5,
                        ]
                    };
                    self.add_menu = Some(AddMenu {
                        position,
                        world: self.view.screen_to_world(position, self.last_canvas),
                        query: String::new(),
                        category: None,
                        selected: 0,
                        scroll: 0.,
                    });
                    break;
                }
                Command::ResetZoom => self.view.zoom_at(1., input.mouse, self.last_canvas),
                Command::Arrange => {
                    self.arrange_selected();
                }
                Command::MoveLeft | Command::MoveRight | Command::MoveUp | Command::MoveDown => {
                    let step = if modifiers.shift { 10. } else { 1. };
                    let delta = match command {
                        Command::MoveLeft => [-step, 0.],
                        Command::MoveRight => [step, 0.],
                        Command::MoveUp => [0., -step],
                        _ => [0., step],
                    };
                    self.move_selected(delta);
                }
                _ => {}
            }
        }
    }
    pub(super) fn canvas_input(&mut self, cards: &[Card], canvas: Rect, input: &Input) {
        if canvas.contains(input.mouse) && self.drag.is_none() && input.wheel != [0.; 2] {
            if input.modifiers.control && !input.modifiers.alt && !input.modifiers.shift {
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
                    if input.modifiers.control && input.modifiers.shift && !input.modifiers.alt {
                        if let Some(port) = socket(point).filter(|s| s.output) {
                            if let Err(error) = self.toggle_preview(port.port) {
                                self.notice = Some(error.to_string());
                            }
                        } else if let Some(card) =
                            cards.iter().rev().find(|c| c.rect.contains(point))
                        {
                            if let Some(node) = self.document.nodes.iter().find(|n| n.id == card.id)
                            {
                                match node_ports(node, &self.document.public_parameters) {
                                    Ok(ports) if ports.outputs.is_empty() => {
                                        self.restore_preview();
                                    }
                                    Ok(ports) if ports.outputs.len() == 1 => {
                                        let port = ports.outputs.keys().next().unwrap();
                                        if let Err(error) =
                                            self.toggle_preview(PortRef::new(&card.id, port))
                                        {
                                            self.notice = Some(error.to_string());
                                        }
                                    }
                                    Ok(_) => {
                                        self.context_menu = Some(ContextMenu {
                                            position: point,
                                            kind: MenuKind::Preview(card.id.clone()),
                                            selected: 0,
                                        })
                                    }
                                    Err(error) => self.notice = Some(error.to_string()),
                                }
                            }
                        }
                    } else if input.modifiers.alt {
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
                        let header = point[1] < card.rect.min[1] + HEADER * self.view.zoom;
                        if !header || !self.selected.contains(&card.id) || input.modifiers.shift {
                            self.select(Some(&card.id), input.modifiers.shift);
                        } else {
                            self.primary = Some(card.id.clone());
                        }
                        if self.selected.contains(&card.id) && header {
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
                    if input.modifiers.alt && !input.modifiers.control && !input.modifiers.shift {
                        self.drag = Some(Drag::Cut {
                            points: vec![point],
                            last: point,
                            inputs: BTreeSet::new(),
                        });
                    } else if input.modifiers == motion_ui::input::Modifiers::default() {
                        let edge = socket(point)
                            .filter(|s| !s.output)
                            .filter(|s| self.document.connections.iter().any(|e| e.to == s.port))
                            .map(|s| s.port)
                            .or_else(|| self.wire_at(cards, canvas, point));
                        let kind = if let Some(port) = edge {
                            MenuKind::Wire(port)
                        } else if let Some(card) =
                            cards.iter().rev().find(|c| c.rect.contains(point))
                        {
                            if !self.selected.contains(&card.id) {
                                self.select(Some(&card.id), false);
                            }
                            MenuKind::Node(card.id.clone())
                        } else {
                            MenuKind::Canvas
                        };
                        self.context_menu = Some(ContextMenu {
                            position: point,
                            kind,
                            selected: 0,
                        });
                    }
                }
            }
        }
        if let Some(drag) = self.drag.take() {
            match drag {
                Drag::Cut {
                    mut points,
                    last,
                    mut inputs,
                } => {
                    if input.mouse.iter().all(|v| v.is_finite()) && input.mouse != last {
                        self.collect_cut_inputs(cards, canvas, last, input.mouse, &mut inputs);
                        points.push(input.mouse);
                        if points.len() > 1024 {
                            points = points.into_iter().step_by(2).collect();
                        }
                    }
                    if input
                        .released
                        .is_some_and(|(button, _)| button == MouseButton::Right)
                    {
                        self.disconnect_inputs(&inputs);
                    } else if input.held == Some(MouseButton::Right) {
                        self.drag = Some(Drag::Cut {
                            points,
                            last: input.mouse,
                            inputs,
                        });
                    }
                }
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
                        if canvas.contains(point) {
                            if let Some(target) = socket(point) {
                                if target.output {
                                    self.notice = Some("Connect to an input port".into());
                                } else if let Err(error) =
                                    self.connect_replacing(source, target.port, old_input.as_ref())
                                {
                                    self.notice = Some(error.to_string());
                                }
                            } else if !cards.iter().any(|c| c.rect.contains(point)) {
                                if let Some(input) = old_input {
                                    self.disconnect_input(&input);
                                }
                            }
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
