use super::*;
use crate::node_editor::search as catalogue;

impl NodeEditor {
    pub(super) fn library(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        let styles = list.styles();
        let colors = styles.colors();
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let line = styles.metric("row.height", 22.);
        let row_height = styles.metric("node.search.result_height", 44.);
        let field_height = styles.metric("viewer.field.height", 26.);
        let normal_details = styles.metric("node.search.detail_height", 76.);
        let compact = area.height()
            < pad * 3. + gap * 2. + line * 2. + field_height + row_height + normal_details;
        let mut paint = list.child();
        paint::rect(&mut paint, area, colors.PANEL);
        let border = styles.metric("border", 1.);
        paint::rect(
            &mut paint,
            Rect::new(area.max[0] - border, area.min[1], border, area.height()),
            colors.BORDER,
        );
        if !compact {
            label(
                &mut paint,
                atlas,
                Rect::new(
                    area.min[0] + pad,
                    area.min[1] + gap,
                    area.width() - pad * 2.,
                    line,
                ),
                language.text("节点库", "NODE LIBRARY"),
                colors.TEXT,
                Align::Left,
            );
        }
        let field = Rect::new(
            area.min[0] + pad,
            area.min[1] + pad + if compact { 0. } else { line + gap },
            area.width() - pad * 2.,
            field_height,
        );
        if hit(input, field) && self.commit_field().is_ok() {
            self.search_focus = true;
            self.search.focus();
            self.field = None;
        }
        if self.search_focus {
            self.search.draw(&mut paint, atlas, field);
        } else {
            paint::rect(&mut paint, field, colors.SUNKEN);
        }
        if !self.search_focus || self.search.is_empty() {
            label(
                &mut paint,
                atlas,
                field.shrink(gap),
                if self.search.is_empty() {
                    language.text("搜索节点…", "Search nodes…")
                } else {
                    &self.search
                },
                if self.search.is_empty() {
                    colors.TEXT_MUTED
                } else {
                    colors.TEXT
                },
                Align::Left,
            );
        }
        let detail_height = if !compact {
            normal_details
        } else {
            let compact_details = styles.metric("node.search.compact_detail_height", 58.);
            if area.max[1] - pad - field.max[1] - pad - gap - row_height >= compact_details {
                compact_details
            } else {
                0.
            }
        };
        let details = Rect::new(
            area.min[0] + pad,
            area.max[1] - pad - detail_height,
            area.width() - pad * 2.,
            detail_height,
        );
        let content = Rect::new(
            area.min[0] + pad,
            field.max[1] + pad,
            area.width() - pad * 2.,
            (details.min[1] - if detail_height > 0. { gap } else { 0. } - field.max[1] - pad)
                .max(0.),
        );
        let query_active = !self.search.trim().is_empty();
        let mut sections = Vec::new();
        if query_active {
            sections.push((
                language.text("所有分类", "All categories"),
                catalogue::find(&self.search, None),
            ));
        } else {
            let recent = self.recent_nodes.items();
            if !recent.is_empty() {
                sections.push((language.text("最近使用", "Recent"), recent));
            }
            for category in [
                catalog::Category::Input,
                catalog::Category::Value,
                catalog::Category::Math,
                catalog::Category::Color,
                catalog::Category::Sdf,
                catalog::Category::Output,
            ] {
                let entries = catalogue::find("", Some(category));
                if !entries.is_empty() {
                    sections.push((category.title(language), entries));
                }
            }
        }
        let entries: Vec<_> = sections
            .iter()
            .flat_map(|(_, entries)| entries.iter().copied())
            .collect();
        self.library_selected = self.library_selected.min(entries.len().saturating_sub(1));
        let section_height = if compact && query_active { 0. } else { line };
        let extent = sections.len() as f32 * section_height + entries.len() as f32 * row_height;
        let max_scroll = (extent - content.height()).max(0.);
        self.library_scroll = self.library_scroll.clamp(0., max_scroll);
        if content.contains(input.mouse) && !input.modifiers.alt {
            self.library_scroll = (self.library_scroll + input.wheel[1]).clamp(0., max_scroll);
        }
        let moved = self.search_focus
            && input
                .key_presses()
                .any(|(k, m, _)| !m.control && !m.alt && matches!(k, Key::Up | Key::Down));
        let enter = self.search_focus
            && motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
        let mut chosen = if enter {
            entries.get(self.library_selected).copied()
        } else {
            None
        };
        let mut rows = list.child();
        let mut y = 0.;
        let mut index = 0;
        let mut hovered_entry = None;
        // Reveal the selected item before painting so scrolling and hit testing agree.
        if moved {
            let mut selected_y = 0.;
            let mut count = 0;
            for (_, items) in &sections {
                selected_y += section_height;
                if self.library_selected < count + items.len() {
                    selected_y += (self.library_selected - count) as f32 * row_height;
                    break;
                }
                selected_y += items.len() as f32 * row_height;
                count += items.len();
            }
            self.library_scroll = search::reveal_offset(
                self.library_scroll,
                selected_y,
                row_height,
                content.height(),
                max_scroll,
            );
        }
        for (title, items) in sections {
            if section_height > 0. {
                label(
                    &mut rows,
                    atlas,
                    Rect::new(
                        content.min[0] + gap,
                        content.min[1] + y - self.library_scroll,
                        content.width() - gap * 2.,
                        line,
                    ),
                    title,
                    colors.TEXT_MUTED,
                    Align::Left,
                );
            }
            y += section_height;
            for entry in items {
                let row = Rect::new(
                    content.min[0],
                    content.min[1] + y - self.library_scroll,
                    content.width(),
                    row_height,
                );
                if intersects(row, content) {
                    let hovered = content.contains(input.mouse) && row.contains(input.mouse);
                    search::result_row(
                        &mut rows,
                        atlas,
                        row,
                        entry,
                        language,
                        self.search_focus && index == self.library_selected,
                        hovered,
                    );
                    if hovered {
                        hovered_entry = Some(entry);
                    }
                    if content.contains(input.mouse) && hit(input, row) {
                        chosen = Some(entry);
                    }
                }
                y += row_height;
                index += 1;
            }
        }
        if entries.is_empty() {
            label(
                &mut rows,
                atlas,
                content.shrink(gap),
                language.text("没有匹配的节点", "No nodes found"),
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        rows.clip_to(content);
        paint.append(rows);
        if detail_height > 0. {
            search::details(
                &mut paint,
                atlas,
                details,
                hovered_entry.or_else(|| {
                    self.search_focus
                        .then(|| entries.get(self.library_selected).copied())
                        .flatten()
                }),
                language,
                self.search_focus,
            );
        }
        paint.clip_to(area);
        list.append(paint);
        if let Some(entry) = chosen.filter(|_| self.commit_field().is_ok()) {
            let position = self.view.screen_to_world(
                [
                    self.last_canvas.min[0] + self.last_canvas.width() * 0.45,
                    self.last_canvas.min[1] + self.last_canvas.height() * 0.45,
                ],
                self.last_canvas,
            );
            match self.add_catalogue_entry(entry, position) {
                Ok(_) => {
                    self.search_focus = false;
                    self.field = None;
                }
                Err(error) => self.notice = Some(error.to_string()),
            }
        }
    }
}
