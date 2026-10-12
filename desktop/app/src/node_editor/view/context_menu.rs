//! Explicit commands replace destructive right-clicks; multiple preview outputs
//! are chosen by port rather than guessing a color conversion.
use super::*;

enum Choice {
    Add,
    Fit,
    Duplicate,
    Delete,
    Disconnect(PortRef),
    Preview(PortRef),
    Restore,
}

impl NodeEditor {
    pub(super) fn paint_context_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        input: &Input,
        language: Language,
    ) {
        let Some(mut menu) = self.context_menu.take() else {
            return;
        };
        if input.focus_lost
            || input
                .key_presses()
                .any(|(k, _, repeat)| k == Key::Escape && !repeat)
        {
            return;
        }
        let mut entries: Vec<(String, Choice)> = vec![];
        match &menu.kind {
            MenuKind::Wire(port) => entries.push((
                language.text("断开连线", "Disconnect wire").into(),
                Choice::Disconnect(port.clone()),
            )),
            MenuKind::Canvas => {
                entries.push((
                    language
                        .text("添加节点 · Shift+A", "Add node · Shift+A")
                        .into(),
                    Choice::Add,
                ));
                entries.push((
                    language.text("适配全部 · Home", "Frame all · Home").into(),
                    Choice::Fit,
                ));
            }
            MenuKind::Node(id) | MenuKind::Preview(id) => {
                if matches!(menu.kind, MenuKind::Node(_)) {
                    entries.push((
                        language.text("复制 · Ctrl+D", "Duplicate · Ctrl+D").into(),
                        Choice::Duplicate,
                    ));
                    entries.push((
                        language.text("删除 · X", "Delete · X").into(),
                        Choice::Delete,
                    ));
                }
                if let Some(node) = self.document.nodes.iter().find(|n| n.id == *id) {
                    if let Ok(ports) =
                        motion_graph::node_port_specs(node, &self.document.public_parameters)
                    {
                        for (port, spec) in ports.outputs {
                            entries.push((
                                format!(
                                    "{} · {}",
                                    language.text("预览", "Preview"),
                                    language.text(spec.name, spec.english_name)
                                ),
                                Choice::Preview(PortRef::new(id, port)),
                            ));
                        }
                    }
                }
            }
        }
        if self.preview_target.is_some() {
            entries.push((
                language.text("恢复永久输出", "Restore final output").into(),
                Choice::Restore,
            ));
        }
        if entries.is_empty() {
            return;
        }
        for (key, modifiers, _) in input.key_presses() {
            if modifiers != motion_ui::input::Modifiers::default() {
                continue;
            }
            match key {
                Key::Up => menu.selected = menu.selected.saturating_sub(1),
                Key::Down => menu.selected = menu.selected.saturating_add(1),
                _ => {}
            }
        }
        menu.selected = menu.selected.min(entries.len() - 1);
        let width = 280f32.min(bounds.width() - 16.);
        let rows = entries
            .len()
            .min(((bounds.height() - 32.) / 28.).max(1.) as usize);
        let height = rows as f32 * 28. + 16.;
        let x = menu.position[0].clamp(bounds.min[0] + 8., bounds.max[0] - width - 8.);
        let y = menu.position[1].clamp(bounds.min[1] + 8., bounds.max[1] - height - 8.);
        let area = Rect::new(x, y, width, height);
        if input.pressed.is_some_and(|(_, p)| !area.contains(p)) {
            return;
        }
        let start = menu.selected.saturating_sub(rows - 1);
        let enter = motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
        let mut chosen = None;
        let mut popup = PaintList::default();
        paint::rect(&mut popup, area, palette::CHROME);
        paint::stroke(&mut popup, area, palette::BORDER, 1.);
        for (i, (title, _)) in entries.iter().enumerate().skip(start).take(rows) {
            let row = Rect::new(x + 6., y + 8. + (i - start) as f32 * 28., width - 12., 26.);
            if i == menu.selected || row.contains(input.mouse) {
                paint::rect(&mut popup, row, palette::ACCENT.with_alpha(0.2));
            }
            label(
                &mut popup,
                atlas,
                row.shrink(4.),
                title,
                palette::TEXT,
                Align::Left,
            );
            if hit(input, row) || (enter && i == menu.selected) {
                chosen = Some(i);
            }
        }
        list.glyphs.retain(|g| {
            !intersects(
                area,
                Rect::new(g.position[0], g.position[1], g.size[0], g.size[1]),
            )
        });
        popup.clip_to(area);
        list.append(popup);
        if let Some(i) = chosen {
            match entries.into_iter().nth(i).unwrap().1 {
                Choice::Disconnect(port) => {
                    self.disconnect_input(&port);
                }
                Choice::Delete => {
                    self.delete_selected();
                }
                Choice::Duplicate => {
                    if let Err(error) = self.duplicate_selected() {
                        self.notice = Some(error.to_string());
                    }
                }
                Choice::Preview(port) => {
                    if let Err(error) = self.toggle_preview(port) {
                        self.notice = Some(error.to_string());
                    }
                }
                Choice::Restore => {
                    self.restore_preview();
                }
                Choice::Fit => self.fit_all(self.last_canvas),
                Choice::Add => {
                    self.add_menu = Some(AddMenu {
                        position: menu.position,
                        world: self.view.screen_to_world(menu.position, self.last_canvas),
                        query: String::new(),
                        category: None,
                        selected: 0,
                        scroll: 0.,
                    })
                }
            }
        } else {
            self.context_menu = Some(menu);
        }
    }
}
