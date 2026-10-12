use super::*;

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

        let mut paint = list.child();
        paint::rect(&mut paint, area, colors.PANEL);
        paint::rect(
            &mut paint,
            Rect::new(area.max[0] - 1., area.min[1], 1., area.height()),
            colors.BORDER,
        );
        label(
            &mut paint,
            atlas,
            Rect::new(area.min[0] + 12., area.min[1] + 6., area.width() - 24., 26.),
            language.text("节点库", "NODE LIBRARY"),
            colors.TEXT,
            Align::Left,
        );
        let search = Rect::new(
            area.min[0] + 10.,
            area.min[1] + 36.,
            area.width() - 20.,
            27.,
        );
        paint::rect(&mut paint, search, colors.SUNKEN);
        if self.search_focus {
            paint::stroke(&mut paint, search, colors.ACCENT, 1.);
        }
        label(
            &mut paint,
            atlas,
            search.shrink(6.),
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
        if hit(input, search) && self.commit_field().is_ok() {
            self.search_focus = true;
            self.field = None;
        }
        let content = Rect::new(
            area.min[0] + 7.,
            search.max[1] + 10.,
            area.width() - 14.,
            (area.max[1] - search.max[1] - 14.).max(0.),
        );
        let items: Vec<_> = catalog()
            .into_iter()
            .filter(|item| {
                let query = self.search.to_lowercase();
                item.title(Language::Chinese)
                    .to_lowercase()
                    .contains(&query)
                    || item
                        .title(Language::English)
                        .to_lowercase()
                        .contains(&query)
                    || item.operation.kind().contains(&query)
            })
            .collect();
        let extent = items.len() as f32 * 29. + 6. * 30.;
        if area.contains(input.mouse) && !input.modifiers.alt {
            self.library_scroll = (self.library_scroll + input.wheel[1])
                .clamp(0., (extent - content.height()).max(0.));
        }
        let mut rows = list.child();
        let mut y = content.min[1] - self.library_scroll;
        let mut category = None;
        for item in items {
            if category != Some(item.category) {
                label(
                    &mut rows,
                    atlas,
                    Rect::new(content.min[0] + 5., y, content.width() - 10., 27.),
                    item.category.title(language),
                    colors.TEXT_MUTED,
                    Align::Left,
                );
                y += 29.;
                category = Some(item.category);
            }
            let rect = Rect::new(content.min[0], y, content.width(), 27.);
            if intersects(rect, content) {
                let hovered = rect.contains(input.mouse) && content.contains(input.mouse);
                if hovered {
                    paint::rect(&mut rows, rect, colors.SUNKEN);
                }
                paint::rect(
                    &mut rows,
                    Rect::new(rect.min[0] + 5., rect.min[1] + 9., 7., 7.),
                    category_color(&item.operation),
                );
                label(
                    &mut rows,
                    atlas,
                    Rect::new(rect.min[0] + 20., rect.min[1], rect.width() - 24., 27.),
                    &item.title(language),
                    colors.TEXT,
                    Align::Left,
                );
                if content.contains(input.mouse) && hit(input, rect) && self.commit_field().is_ok()
                {
                    self.search_focus = false;
                    self.field = None;
                    let position = self.view.screen_to_world(
                        [
                            self.last_canvas.min[0] + self.last_canvas.width() * 0.45,
                            self.last_canvas.min[1] + self.last_canvas.height() * 0.45,
                        ],
                        self.last_canvas,
                    );
                    let result = if let Some(ty) = item.parameter_type {
                        self.add_public_parameter(ty, position)
                    } else {
                        self.add_node(item.operation, position)
                    };
                    if let Err(error) = result {
                        self.notice = Some(error.to_string());
                    }
                }
            }
            y += 29.;
        }
        rows.clip_to(content);
        paint.append(rows);
        paint.clip_to(area);
        list.append(paint);
    }
}
