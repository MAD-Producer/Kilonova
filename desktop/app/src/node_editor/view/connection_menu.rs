//! Ambiguous body drops ask for a port instead of silently choosing a branch.
use super::*;
use motion_ui::style::{box_style, text_style, State, States};

impl NodeEditor {
    pub(super) fn paint_connection_choices(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        input: &Input,
        language: Language,
        mut menu: ContextMenu,
    ) {
        let MenuKind::Connect {
            connections,
            old_input,
            scroll,
            pointer,
        } = &mut menu.kind
        else {
            return;
        };
        if connections.is_empty() {
            return;
        }
        let styles = list.styles();
        let padding = styles.metric("padding", 5.);
        let row_height = styles.metric("context.row.height", 20.).max(1.);
        let same_input = connections
            .iter()
            .all(|other| other.to == connections[0].to);
        let titles: Vec<_> = connections
            .iter()
            .map(|connection| {
                let port = if same_input {
                    &connection.from
                } else {
                    &connection.to
                };
                let node = self.document.nodes.iter().find(|node| node.id == port.node);
                let named = node
                    .and_then(|node| {
                        motion_graph::group_port(&node.operation, &port.port, same_input)
                    })
                    .map(|port| language.text(&port.name, &port.english_name).to_string());
                let name = named
                    .or_else(|| {
                        node.and_then(|node| {
                            motion_graph::node_port_specs(node, &self.document.public_parameters)
                                .ok()
                        })
                        .and_then(|specs| {
                            if same_input {
                                specs.outputs
                            } else {
                                specs.inputs
                            }
                            .remove(&port.port)
                        })
                        .map(|spec| language.text(spec.name, spec.english_name).to_string())
                    })
                    .unwrap_or_else(|| port.port.clone());
                let replaced = !same_input
                    && self
                        .document
                        .connections
                        .iter()
                        .any(|edge| edge.to == connection.to && edge.from != connection.from);
                format!(
                    "{name} · {}{}",
                    port.port,
                    if replaced {
                        language.text("（替换）", " (replace)")
                    } else {
                        ""
                    }
                )
            })
            .collect();
        let width = titles
            .iter()
            .map(|title| atlas.measure(title) + padding * 2.)
            .fold(styles.metric("context.menu.width", 220.), f32::max)
            .min(styles.metric("dropdown.max_width", 320.))
            .min((bounds.width() - padding * 2.).max(1.));
        let visible = (((bounds.height() - padding * 4. - row_height) / row_height)
            .floor()
            .max(1.) as usize)
            .min(connections.len());
        let height = (row_height * (visible + 1) as f32 + padding * 2.)
            .min((bounds.height() - padding * 2.).max(1.));
        let x = menu.position[0].clamp(
            bounds.min[0] + padding,
            (bounds.max[0] - width - padding).max(bounds.min[0] + padding),
        );
        let y = menu.position[1].clamp(
            bounds.min[1] + padding,
            (bounds.max[1] - height - padding).max(bounds.min[1] + padding),
        );
        let area = Rect::new(x, y, width, height);
        if input
            .pressed
            .is_some_and(|(_, point)| !area.contains(point))
        {
            return;
        }
        let mut keyboard = false;
        for (key, _, _) in input
            .key_presses()
            .filter(|(_, modifiers, _)| *modifiers == Default::default())
        {
            match key {
                Key::Up => {
                    menu.selected = menu.selected.saturating_sub(1);
                    keyboard = true;
                }
                Key::Down => {
                    menu.selected = (menu.selected + 1).min(connections.len() - 1);
                    keyboard = true;
                }
                Key::Home => {
                    menu.selected = 0;
                    keyboard = true;
                }
                Key::End => {
                    menu.selected = connections.len() - 1;
                    keyboard = true;
                }
                _ => {}
            }
        }
        let max_scroll = (connections.len() - visible) as f32 * row_height;
        *scroll = (*scroll).clamp(0., max_scroll);
        let wheeled = area.contains(input.mouse) && input.wheel[1] != 0.;
        if wheeled {
            *scroll = (*scroll + input.wheel[1]).clamp(0., max_scroll);
        }
        if keyboard {
            *scroll = (*scroll)
                .min(menu.selected as f32 * row_height)
                .max((menu.selected + 1).saturating_sub(visible) as f32 * row_height);
        }
        let start = (*scroll / row_height).floor() as usize;
        if wheeled {
            menu.selected = menu
                .selected
                .clamp(start, (start + visible - 1).min(connections.len() - 1));
        }
        let moved = *pointer != input.mouse;
        *pointer = input.mouse;
        if !keyboard && (moved || wheeled) {
            let offset = input.mouse[1] - (y + padding + row_height);
            if input.mouse[0] >= x + padding
                && input.mouse[0] <= x + width - padding
                && offset >= 0.
                && offset < visible as f32 * row_height
            {
                menu.selected = start + (offset / row_height).floor() as usize;
            }
        }
        let mut popup = list.child();
        let frame = styles
            .resolve("context_menu", States::default())
            .expect("context menu style");
        box_style(&mut popup, area, frame);
        let title = if connections
            .iter()
            .all(|connection| connection.from == connections[0].from)
        {
            language.text("选择输入端口", "Choose input port")
        } else {
            language.text("选择输出端口", "Choose output port")
        };
        let mut heading = styles
            .resolve("context_menu.row", States::default().with(State::Disabled))
            .expect("context menu row style");
        heading.padding = padding;
        text_style(
            &mut popup,
            atlas,
            Rect::new(x + padding, y + padding, width - padding * 2., row_height),
            title,
            heading,
            Align::Left,
        );
        let mut chosen = None;
        for (index, title) in titles.iter().enumerate().skip(start).take(visible) {
            let row = Rect::new(
                x + padding,
                y + padding + row_height * (index - start + 1) as f32,
                width - padding * 2.,
                row_height,
            );
            let mut states = States::default();
            if row.contains(input.mouse) && menu.selected == index {
                states = states.with(State::Hovered);
            }
            if menu.selected == index {
                states = states.with(State::Selected);
            }
            let style = styles
                .resolve("context_menu.row", states)
                .expect("context menu row style");
            box_style(&mut popup, row, style);
            text_style(
                &mut popup,
                atlas,
                Rect::new(
                    row.min[0] + padding,
                    row.min[1],
                    (row.width() - padding * 2.).max(1.),
                    row.height(),
                ),
                title,
                style,
                Align::Left,
            );
            if hit(input, row) {
                chosen = Some(index);
            }
        }
        if input.key_presses().any(|(key, modifiers, repeat)| {
            key == Key::Enter && modifiers == Default::default() && !repeat
        }) {
            chosen = Some(menu.selected.min(connections.len() - 1));
        }
        list.retain_glyphs(|glyph| {
            !intersects(
                area,
                Rect::new(
                    glyph.position[0],
                    glyph.position[1],
                    glyph.size[0],
                    glyph.size[1],
                ),
            )
        });
        popup.clip_to(area);
        list.append(popup);
        if let Some(index) = chosen {
            let connection = connections[index].clone();
            if let Err(error) =
                self.connect_replacing(connection.from, connection.to, old_input.as_ref())
            {
                self.notice = Some(error.to_string());
            }
        } else {
            self.context_menu = Some(menu);
        }
    }
}
