//! Categorized popup with placement latched in world space when Shift+A opens it.
use super::super::catalog::Category;
use super::*;

impl NodeEditor {
    pub(super) fn paint_add_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        input: &Input,
        language: Language,
    ) {
        let Some(mut menu) = self.add_menu.take() else {
            return;
        };
        if input
            .key_presses()
            .any(|(key, _, repeat)| key == Key::Escape && !repeat)
            || input.focus_lost
        {
            return;
        }
        if !input.text.is_empty() {
            menu.query.push_str(&input.text);
            menu.selected = 0;
            menu.scroll = 0.;
        }
        for (key, modifiers, _) in input.key_presses() {
            if modifiers.control || modifiers.alt {
                continue;
            }
            match key {
                Key::Backspace => {
                    menu.query.pop();
                    menu.selected = 0;
                    menu.scroll = 0.;
                }
                Key::Down => menu.selected = menu.selected.saturating_add(1),
                Key::Up => menu.selected = menu.selected.saturating_sub(1),
                _ => {}
            }
        }
        let width = 410f32.min(bounds.width() - 16.);
        let height = 400f32.min(bounds.height() - 16.);
        let x = menu.position[0].clamp(bounds.min[0] + 8., bounds.max[0] - width - 8.);
        let y = menu.position[1].clamp(bounds.min[1] + 8., bounds.max[1] - height - 8.);
        let area = Rect::new(x, y, width, height);
        if input
            .pressed
            .is_some_and(|(_, point)| !area.contains(point))
        {
            return;
        }
        let mut popup = PaintList::default();
        paint::rect(&mut popup, area, palette::CHROME);
        paint::stroke(&mut popup, area, palette::BORDER, 1.);
        let search = Rect::new(x + 8., y + 8., width - 16., 30.);
        paint::rect(&mut popup, search, palette::SUNKEN);
        label(
            &mut popup,
            atlas,
            search.shrink(6.),
            if menu.query.is_empty() {
                language.text("搜索节点…", "Search nodes…")
            } else {
                &menu.query
            },
            palette::TEXT,
            Align::Left,
        );
        for (i, category) in [
            None,
            Some(Category::Input),
            Some(Category::Value),
            Some(Category::Math),
            Some(Category::Color),
            Some(Category::Sdf),
            Some(Category::Output),
        ]
        .into_iter()
        .enumerate()
        {
            let row = Rect::new(x + 8., y + 46. + i as f32 * 30., 126., 28.);
            if menu.category == category {
                paint::rect(&mut popup, row, palette::ACCENT.with_alpha(0.2));
            }
            label(
                &mut popup,
                atlas,
                row.shrink(4.),
                category
                    .map(|c| c.title(language))
                    .unwrap_or(language.text("全部", "ALL")),
                palette::TEXT_MUTED,
                Align::Left,
            );
            if hit(input, row) {
                menu.category = category;
                menu.selected = 0;
                menu.scroll = 0.;
            }
        }
        let query = menu.query.to_lowercase();
        let entries: Vec<_> = catalog()
            .into_iter()
            .filter(|item| {
                menu.category.is_none_or(|c| item.category == c)
                    && (query.is_empty()
                        || [
                            item.title(Language::Chinese),
                            item.title(Language::English),
                            item.operation.kind().into(),
                        ]
                        .iter()
                        .any(|name| name.to_lowercase().contains(&query)))
            })
            .collect();
        menu.selected = menu.selected.min(entries.len().saturating_sub(1));
        let content = Rect::new(x + 142., y + 46., width - 150., height - 54.);
        let max_scroll = (entries.len() as f32 * 28. - content.height()).max(0.);
        menu.scroll = menu.scroll.clamp(0., max_scroll);
        if content.contains(input.mouse) {
            menu.scroll = (menu.scroll + input.wheel[1]).clamp(0., max_scroll);
        }
        if input
            .key_presses()
            .any(|(k, _, _)| matches!(k, Key::Up | Key::Down))
        {
            let top = menu.selected as f32 * 28.;
            menu.scroll = menu
                .scroll
                .min(top)
                .max(top + 28. - content.height())
                .clamp(0., max_scroll);
        }
        let mut rows = PaintList::default();
        let mut chosen = None;
        let enter = motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
        for (i, item) in entries.iter().enumerate() {
            let row = Rect::new(
                content.min[0],
                content.min[1] + i as f32 * 28. - menu.scroll,
                content.width(),
                27.,
            );
            if i == menu.selected {
                paint::rect(&mut rows, row, palette::ACCENT.with_alpha(0.2));
            }
            label(
                &mut rows,
                atlas,
                row.shrink(4.),
                &item.title(language),
                palette::TEXT,
                Align::Left,
            );
            if (enter && i == menu.selected) || (content.contains(input.mouse) && hit(input, row)) {
                chosen = Some(item.clone());
            }
        }
        if entries.is_empty() {
            label(
                &mut rows,
                atlas,
                content.shrink(4.),
                language.text("没有匹配的节点", "No nodes found"),
                palette::TEXT_MUTED,
                Align::Left,
            );
        }
        rows.clip_to(content);
        popup.append(rows);
        list.retain_glyphs(|g| {
            !intersects(
                area,
                Rect::new(g.position[0], g.position[1], g.size[0], g.size[1]),
            )
        });
        popup.clip_to(area);
        list.append(popup);
        if let Some(item) = chosen {
            let result = if let Some(ty) = item.parameter_type {
                self.add_public_parameter(ty, menu.world)
            } else {
                self.add_node(item.operation, menu.world)
            };
            if let Err(error) = result {
                self.notice = Some(error.to_string());
                self.add_menu = Some(menu);
            }
        } else {
            self.add_menu = Some(menu);
        }
    }
}
