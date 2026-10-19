//! Builtin source graphs open as independent editable documents.
use crate::{i18n::Locale, node_editor::EditorSnapshot};
use motion_ui::{
    input::Key,
    paint,
    ui::{self, Align},
    Input, PaintList, Rect, TextAtlas,
};

pub struct Browser {
    pub snapshot: EditorSnapshot,
    pub needs_save: bool,
    selected: usize,
}
pub enum Event {
    Cancel,
    Open { id: &'static str, save_first: bool },
}
impl Browser {
    fn card(size: [f32; 2]) -> Rect {
        let width = (size[0] - 48.).min(840.);
        let height = (size[1] - 48.).min(690.);
        Rect::new(
            (size[0] - width) * 0.5,
            (size[1] - height) * 0.5,
            width,
            height,
        )
    }
    pub(crate) fn open_button(size: [f32; 2]) -> Rect {
        let card = Self::card(size);
        let width = (card.width() - 56.) / 3.;
        Rect::new(
            card.min[0] + 20. + width + 8.,
            card.max[1] - 48.,
            width,
            30.,
        )
    }
    pub fn new(snapshot: EditorSnapshot, needs_save: bool) -> Self {
        Self {
            snapshot,
            needs_save,
            selected: Self::recommended(),
        }
    }
    fn recommended() -> usize {
        motion_graph::builtin_examples()
            .iter()
            .position(|example| example.id == "solid_color")
            .unwrap_or(0)
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
    ) -> Option<Event> {
        let styles = list.styles();
        let colors = styles.colors();

        let zh = locale == Locale::Zh;
        let entries = motion_graph::builtin_examples();
        if input.keys.iter().any(|(key, _)| *key == Key::Escape) {
            return Some(Event::Cancel);
        }
        let card = Self::card(size);
        let width = card.width();
        let height = card.height();
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            colors.SUNKEN.with_alpha(0.85),
        );
        paint::rect(list, card, colors.PANEL);
        paint::stroke(list, card, colors.BORDER, 1.);
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., card.min[1] + 12., width - 40., 28.),
            if zh {
                "节点示例库"
            } else {
                "Node Example Library"
            },
            colors.TEXT,
            Align::Left,
        );
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., card.min[1] + 42., width - 40., 24.),
            if zh {
                "打开可编辑的 .msgraph，查看连线与公开参数"
            } else {
                "Open editable .msgraph sources and explore their connections and parameters"
            },
            colors.TEXT_MUTED,
            Align::Left,
        );
        let rows = entries.len().div_ceil(2).max(1);
        let row_height = (height - 282.) / rows as f32;
        let column_width = (width - 48.) * 0.5;
        let recommended = Self::recommended();
        let order = std::iter::once(recommended)
            .chain((0..entries.len()).filter(|index| *index != recommended));
        for (display_index, index) in order.enumerate() {
            let example = &entries[index];
            let rect = Rect::new(
                card.min[0] + 20. + (display_index % 2) as f32 * (column_width + 8.),
                card.min[1] + 76. + (display_index / 2) as f32 * row_height,
                column_width,
                row_height - 8.,
            );
            if ui::button(
                list,
                atlas,
                rect,
                if zh {
                    example.name
                } else {
                    example.english_name
                },
                input,
                true,
            )
            .clicked
            {
                self.selected = index;
            }
            if index == self.selected {
                paint::stroke(list, rect, colors.ACCENT, 2.);
            }
        }
        let example = &entries[self.selected.min(entries.len() - 1)];
        let details = Rect::new(card.min[0] + 20., card.max[1] - 195., width - 40., 106.);
        let description = if zh {
            example.description
        } else {
            example.english_description
        };
        let lesson = if zh {
            example.lesson
        } else {
            example.english_lesson
        };
        let used = paragraph(list, atlas, details, description, colors.TEXT, 2);
        paragraph(
            list,
            atlas,
            Rect::new(
                details.min[0],
                details.min[1] + used,
                details.width(),
                details.height() - used,
            ),
            lesson,
            colors.TEXT_MUTED,
            3,
        );
        if self.needs_save {
            ui::text(
                list,
                atlas,
                Rect::new(card.min[0] + 20., card.max[1] - 82., width - 40., 22.),
                if zh {
                    "当前节点图尚未保存，打开示例前可先保存"
                } else {
                    "The current graph has unsaved changes; save it before opening an example"
                },
                colors.ACCENT,
                Align::Left,
            );
        }
        let y = card.max[1] - 48.;
        let labels = [
            (
                if zh {
                    "保存当前图并打开"
                } else {
                    "Save current & open"
                },
                true,
                self.needs_save,
            ),
            (
                if zh {
                    if self.needs_save {
                        "打开（放弃修改）"
                    } else {
                        "打开示例"
                    }
                } else {
                    if self.needs_save {
                        "Open & discard edits"
                    } else {
                        "Open example"
                    }
                },
                false,
                true,
            ),
            (if zh { "取消" } else { "Cancel" }, false, true),
        ];
        let button_width = (width - 56.) / 3.;
        for (index, (label, save_first, enabled)) in labels.into_iter().enumerate() {
            if ui::button(
                list,
                atlas,
                if index == 1 {
                    Self::open_button(size)
                } else {
                    Rect::new(
                        card.min[0] + 20. + index as f32 * (button_width + 8.),
                        y,
                        button_width,
                        30.,
                    )
                },
                label,
                input,
                enabled,
            )
            .clicked
            {
                return Some(if index == 2 {
                    Event::Cancel
                } else {
                    Event::Open {
                        id: example.id,
                        save_first,
                    }
                });
            }
        }
        None
    }
}
pub fn paragraph(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    color: motion_ui::theme::Color,
    max_lines: usize,
) -> f32 {
    let mut lines = Vec::new();
    let mut line = String::new();
    for character in text.chars() {
        let candidate = format!("{line}{character}");
        if !line.is_empty() && atlas.measure(&candidate) > area.width() {
            lines.push(std::mem::take(&mut line));
        }
        line.push(character);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    let count = max_lines
        .min((area.height() / 20.).floor().max(0.) as usize)
        .min(lines.len());
    for (index, line) in lines.iter().take(count).enumerate() {
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0],
                area.min[1] + index as f32 * 20.,
                area.width(),
                20.,
            ),
            line,
            color,
            Align::Left,
        );
    }
    count as f32 * 20.
}
