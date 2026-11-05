use super::*;
use crate::style::{box_style, ResolvedStyle, States, StyleSheet};

impl TextEditor {
    pub(super) fn draw_active_text(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        style: ResolvedStyle,
        sheet: &StyleSheet,
    ) {
        let pad = style
            .padding
            .min(area.width().min(area.height()).max(0.) * 0.5);
        let body = area.shrink(pad);
        if self.select_all {
            box_style(
                list,
                body,
                sheet
                    .resolve("selection", States::default())
                    .expect("selection style"),
            );
        }
        let Some(layout) = atlas.editor_layout(&self.text) else {
            return;
        };
        let caret = atlas.editor_caret(&layout, self.byte_index()) * style.font_size;
        let scroll = if self.select_all {
            0.
        } else {
            (caret - body.width()).max(0.)
        };
        let origin = body.min[0] - scroll;
        if !self.select_all {
            if let Some(range) = self.selection_bytes() {
                layout.selection_spans(range, |span| {
                    let selection = Rect::new(
                        origin + span[0] * style.font_size,
                        body.min[1],
                        (span[1] - span[0]) * style.font_size,
                        body.height(),
                    );
                    let selection = selection.intersection(body);
                    if selection.width() > 0. && selection.height() > 0. {
                        box_style(
                            list,
                            selection,
                            sheet
                                .resolve("selection", States::default())
                                .expect("selection style"),
                        );
                    }
                });
            }
        }
        ui::positioned_text(
            list,
            atlas,
            body,
            &layout,
            origin,
            style.font_size,
            true,
            |_| style.foreground,
        );
        if !self.select_all {
            let x = origin + caret;
            paint::line(
                list,
                vec![[x, body.min[1]], [x, body.max[1]]],
                style.foreground,
            );
        }
    }
}
