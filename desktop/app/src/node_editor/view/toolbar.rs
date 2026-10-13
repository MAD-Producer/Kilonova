use super::*;

impl NodeEditor {
    pub(super) fn toolbar(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
        actions: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let colors = styles.colors();

        let mut paint = list.child();
        paint::rect(&mut paint, area, colors.CHROME);
        let mut x = area.min[0] + 6.;
        let mut y = area.min[1] + 5.;
        for (index, width, zh, en) in TOOLBAR {
            if x + width > area.max[0] - 6. {
                x = area.min[0] + 6.;
                y += 34.;
            }
            let rect = Rect::new(x, y, width, 27.);
            let enabled = match index {
                9 => !self.undo.is_empty(),
                10 => !self.redo.is_empty(),
                6 => self.pending_compile.is_none(),
                _ => true,
            };
            if ui::button(
                &mut paint,
                atlas,
                rect,
                language.text(zh, en),
                input,
                enabled,
            )
            .clicked
            {
                match index {
                    0 => {
                        self.library_open = !self.library_open;
                        self.search_focus = false;
                    }
                    1 => {
                        if area.width() < 1024. && self.library_open {
                            self.library_open = false;
                            self.inspector_open = true;
                        } else {
                            self.inspector_open = !self.inspector_open;
                        }
                    }
                    8 => self.fit_all(self.last_canvas),
                    9 => {
                        self.undo();
                    }
                    10 => {
                        self.redo();
                    }
                    _ => {
                        let request = match index {
                            2 => Request::Open,
                            3 => Request::Save,
                            4 => Request::SaveAs,
                            5 => Request::Validate,
                            6 => Request::Compile,
                            11 => Request::Examples,
                            _ => Request::ExportMsfx,
                        };
                        if let Ok(action) = self.request(request) {
                            actions.push(action);
                        }
                    }
                }
            }
            x += width + 4.;
        }
        if x + 160. < area.max[0] {
            let title = format!(
                "{}{}",
                self.document.english_name,
                if self.dirty() { "  •" } else { "" }
            );
            label(
                &mut paint,
                atlas,
                Rect::new(x + 10., y, area.max[0] - x - 16., 27.),
                &title,
                colors.TEXT_MUTED,
                Align::Right,
            );
        }
        paint.clip_to(area);
        list.append(paint);
    }
}
