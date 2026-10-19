//! Searchable catalogue with placement latched in world space on opening.
use super::super::catalog::Category;
use super::*;
use crate::node_editor::search as catalogue;

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
        let styles = list.styles();
        let colors = styles.colors();
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("padding", 5.);
        let line = styles.metric("row.height", 22.);
        let row_height = styles.metric("node.search.result_height", 44.);
        let Some(area) = search::menu_area(bounds, menu.position, &styles) else {
            return;
        };
        if input.focus_lost || input.pressed.is_some_and(|(_, p)| !area.contains(p)) {
            return;
        }
        let (changed, event) = menu.query.input(input);
        if matches!(event, Some(motion_ui::widgets::TextEvent::Cancel)) {
            return;
        }
        if changed {
            menu.selected = 0;
            menu.scroll = 0.;
        }
        let mut moved = false;
        for (key, modifiers, _) in input.key_presses() {
            if modifiers.control || modifiers.alt {
                continue;
            }
            match key {
                Key::Down => {
                    menu.selected = menu.selected.saturating_add(1);
                    moved = true;
                }
                Key::Up => {
                    menu.selected = menu.selected.saturating_sub(1);
                    moved = true;
                }
                _ => {}
            }
        }
        let mut popup = list.child();
        paint::rect(&mut popup, area, colors.CHROME);
        paint::stroke(&mut popup, area, colors.BORDER, styles.metric("border", 1.));
        let field = Rect::new(
            area.min[0] + pad,
            area.min[1] + pad,
            area.width() - pad * 2.,
            styles.metric("viewer.field.height", 26.),
        );
        menu.query.draw(&mut popup, atlas, field);
        if menu.query.is_empty() {
            label(
                &mut popup,
                atlas,
                field.shrink(gap),
                language.text("搜索节点…", "Search nodes…"),
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        let top = field.max[1] + pad;
        let normal_details = styles.metric("node.search.detail_height", 76.);
        let compact = area.max[1] - pad - normal_details - gap - top - line < row_height;
        let detail_height = if compact {
            styles.metric("node.search.compact_detail_height", 58.)
        } else {
            normal_details
        }
        .min((area.max[1] - pad - top - row_height - gap).max(0.));
        let details = Rect::new(
            area.min[0] + pad,
            area.max[1] - pad - detail_height,
            area.width() - pad * 2.,
            detail_height,
        );
        let category_width = styles
            .metric("node.add_menu.category_width", 128.)
            .min(area.width() * 0.35);
        let category_area = Rect::new(
            area.min[0] + pad,
            top,
            category_width,
            (details.min[1] - gap - top).max(0.),
        );
        let category_max_scroll = (line * 8. - category_area.height()).max(0.);
        menu.category_scroll = menu.category_scroll.clamp(0., category_max_scroll);
        if category_area.contains(input.mouse) {
            menu.category_scroll =
                (menu.category_scroll + input.wheel[1]).clamp(0., category_max_scroll);
        }
        let mut categories = popup.child();
        for (i, category) in [
            None,
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
            let recent = i == 1;
            let row = Rect::new(
                area.min[0] + pad,
                top + i as f32 * line - menu.category_scroll,
                category_width,
                line,
            );
            if !intersects(row, category_area) {
                continue;
            }
            let selected = if menu.query.trim().is_empty() {
                menu.category == category && menu.recent == recent
            } else {
                i == 0
            };
            if selected {
                paint::rect(&mut categories, row, styles.color("selected"));
            }
            label(
                &mut categories,
                atlas,
                row.shrink(gap),
                if recent {
                    language.text("最近使用", "Recent")
                } else {
                    category
                        .map(|c| c.title(language))
                        .unwrap_or(language.text("全部", "All"))
                },
                colors.TEXT_MUTED,
                Align::Left,
            );
            let hovered = menu.query.trim().is_empty()
                && input.keys.is_empty()
                && input.text.is_empty()
                && input.wheel[1] == 0.
                && category_area.contains(input.mouse)
                && row.contains(input.mouse);
            if category_area.contains(input.mouse)
                && hit(input, row)
                && (!selected || !menu.query.trim().is_empty())
                || !selected && hovered
            {
                menu.category = category;
                menu.recent = recent;
                menu.selected = 0;
                menu.scroll = 0.;
                menu.query.clear();
            }
        }
        if category_max_scroll > 0. {
            let thumb = category_area.height() * category_area.height() / (line * 8.);
            paint::rect(
                &mut categories,
                Rect::new(
                    category_area.max[0] - styles.metric("border", 1.),
                    category_area.min[1]
                        + menu.category_scroll / category_max_scroll
                            * (category_area.height() - thumb),
                    styles.metric("border", 1.),
                    thumb,
                ),
                colors.TEXT_MUTED,
            );
        }
        categories.clip_to(category_area);
        popup.append(categories);
        let entries = if menu.query.trim().is_empty() && menu.recent {
            self.recent_nodes.items()
        } else {
            catalogue::find(&menu.query, menu.category)
        };
        menu.selected = menu.selected.min(entries.len().saturating_sub(1));
        let results_x = area.min[0] + pad + category_width + gap;
        if !compact {
            label(
                &mut popup,
                atlas,
                Rect::new(results_x, top, area.max[0] - pad - results_x, line),
                &format!(
                    "{} · {}",
                    if !menu.query.trim().is_empty() {
                        language.text("所有分类", "All categories")
                    } else if menu.recent {
                        language.text("最近使用", "Recent")
                    } else {
                        menu.category
                            .map(|c| c.title(language))
                            .unwrap_or(language.text("全部", "All"))
                    },
                    entries.len()
                ),
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        let content = Rect::new(
            results_x,
            top + if compact { 0. } else { line },
            area.max[0] - pad - results_x,
            (details.min[1] - gap - top - if compact { 0. } else { line }).max(0.),
        );
        let max_scroll = (entries.len() as f32 * row_height - content.height()).max(0.);
        menu.scroll = menu.scroll.clamp(0., max_scroll);
        if content.contains(input.mouse) {
            menu.scroll = (menu.scroll + input.wheel[1]).clamp(0., max_scroll);
        }
        if moved {
            menu.scroll = search::reveal(
                menu.scroll,
                menu.selected,
                row_height,
                content.height(),
                max_scroll,
            );
        }
        let enter = motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
        let mut chosen = if enter {
            entries.get(menu.selected).copied()
        } else {
            None
        };
        let mut hovered_entry = None;
        let mut rows = list.child();
        for (i, entry) in entries.iter().enumerate() {
            let row = Rect::new(
                content.min[0],
                content.min[1] + i as f32 * row_height - menu.scroll,
                content.width(),
                row_height,
            );
            if !intersects(row, content) {
                continue;
            }
            let hovered = content.contains(input.mouse) && row.contains(input.mouse);
            search::result_row(
                &mut rows,
                atlas,
                row,
                entry,
                language,
                i == menu.selected,
                hovered,
            );
            if hovered {
                hovered_entry = Some(*entry);
            }
            if content.contains(input.mouse) && hit(input, row) {
                chosen = Some(*entry);
            }
        }
        if entries.is_empty() {
            label(
                &mut rows,
                atlas,
                content.shrink(gap),
                if menu.recent && menu.query.trim().is_empty() {
                    language.text("添加节点后会显示在这里", "Added nodes will appear here")
                } else {
                    language.text("没有匹配的节点", "No nodes found")
                },
                colors.TEXT_MUTED,
                Align::Left,
            );
        }
        rows.clip_to(content);
        popup.append(rows);
        search::details(
            &mut popup,
            atlas,
            details,
            hovered_entry.or_else(|| entries.get(menu.selected).copied()),
            language,
            true,
        );
        list.retain_glyphs(|g| {
            !intersects(
                area,
                Rect::new(g.position[0], g.position[1], g.size[0], g.size[1]),
            )
        });
        popup.clip_to(area);
        list.append(popup);
        if let Some(entry) = chosen {
            if let Err(error) = self.add_catalogue_entry(entry, menu.world) {
                self.notice = Some(error.to_string());
                self.add_menu = Some(menu);
            }
        } else {
            self.add_menu = Some(menu);
        }
    }
}
