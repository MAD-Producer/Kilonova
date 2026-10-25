//! One project-wide precision draft; only Apply emits the existing core edit.
use super::*;
use crate::i18n::Text as T;
use motion_effects::image_contract::ChannelPrecision;
use motion_ui::{
    InputSource,
    style::StyleSheet,
    widgets::{Dropdown, DropdownItem},
};

const VALUES: [ChannelPrecision; 4] = [
    ChannelPrecision::Unorm8,
    ChannelPrecision::Unorm16,
    ChannelPrecision::Float16,
    ChannelPrecision::Float32,
];

pub(crate) struct Editor {
    precision: ChannelPrecision,
    dropdown: Dropdown,
}

pub(crate) struct Layout {
    pub card: Rect,
    pub value: Rect,
    pub cancel: Rect,
    pub apply: Rect,
}
impl Layout {
    pub(crate) fn new(sheet: &StyleSheet, size: [f32; 2]) -> Self {
        let pad = sheet.metric("preferences.margin", 16.);
        let row = sheet.metric("row.height", 22.);
        let section = sheet.metric("preferences.section_gap", 12.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let width = sheet
            .metric("dialog.notice.width", 480.)
            .min((size[0] - pad * 2.).max(0.));
        let height = (row * 7. + section * 3. + pad * 2.)
            .min(sheet.metric("dialog.notice.max_height", 320.))
            .min((size[1] - pad * 2.).max(0.));
        let card = Rect::new(
            (size[0] - width) * 0.5,
            (size[1] - height) * 0.5,
            width,
            height,
        );
        let label = sheet
            .metric("preferences.label_width", 180.)
            .min((width - pad * 2. - gap).max(0.) * 0.5);
        let field_x = card.min[0] + pad + label + gap;
        let footer = card.max[1] - pad - row;
        let button = sheet
            .metric("dialog.notice.button_width", 96.)
            .min((width - pad * 2. - gap).max(0.) * 0.5);
        Self {
            card,
            value: Rect::new(
                field_x,
                card.min[1] + pad + row * 2. + section,
                (card.max[0] - pad - field_x).max(0.),
                row,
            ),
            cancel: Rect::new(card.max[0] - pad - button * 2. - gap, footer, button, row),
            apply: Rect::new(card.max[0] - pad - button, footer, button, row),
        }
    }
}

impl Editor {
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
        error: Option<&str>,
    ) -> Option<Event> {
        let sheet = list.styles();
        let layout = Layout::new(&sheet, size);
        let pad = sheet.metric("preferences.margin", 16.);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let section = sheet.metric("preferences.section_gap", 12.);
        let was_open = self.dropdown.open;
        let key = |wanted| {
            !input.composing
                && !input.ime_consumed
                && !input.focus_lost
                && input
                    .key_presses()
                    .any(|(key, _, repeated)| key == wanted && !repeated)
        };
        if !was_open && key(Key::Escape) {
            return Some(Event::Cancelled);
        }
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("shadow"),
        );
        static MISSING: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        if let Some(style) = sheet.resolve("popup", Default::default()) {
            MISSING.success();
            motion_ui::style::box_style(list, layout.card, style);
        } else {
            MISSING.failure(
                "project_precision.popup",
                format_args!("style=popup fallback=panel"),
            );
            paint::rect(list, layout.card, sheet.color("panel"));
        }
        let body_x = layout.card.min[0] + pad;
        let body_width = (layout.card.width() - pad * 2.).max(0.);
        let full_row = |y| Rect::new(body_x, y, body_width, row);
        ui::text(
            list,
            atlas,
            full_row(layout.card.min[1] + pad),
            locale.t(T::ProjectPrecision).trim_end_matches('…'),
            sheet.color("text"),
            Align::Left,
        );
        ui::text(
            list,
            atlas,
            full_row(layout.card.min[1] + pad + row),
            &word(
                locale,
                "应用于当前工程内的所有合成",
                "Applies to every composition in this project.",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        ui::text(
            list,
            atlas,
            Rect::new(
                body_x,
                layout.value.min[1],
                (layout.value.min[0] - body_x - gap).max(0.),
                row,
            ),
            &word(locale, "工作精度", "Working precision"),
            sheet.color("muted"),
            Align::Left,
        );
        ui::text(
            list,
            atlas,
            full_row(layout.value.max[1] + section),
            &word(
                locale,
                "预览与 PNG / 视频导出使用 SDR",
                "Preview and PNG/video output use SDR.",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        if let Some(error) = error {
            ui::text(
                list,
                atlas,
                Rect::new(
                    body_x,
                    layout.value.max[1] + section + row,
                    body_width,
                    row * 2.,
                ),
                error,
                sheet.color("error"),
                Align::Left,
            );
        }
        let labels = if locale == Locale::Zh {
            [
                "8 位整数（UNORM8）",
                "16 位整数（UNORM16）",
                "16 位浮点（Float16）",
                "32 位浮点（Float32）",
            ]
        } else {
            [
                "8-bit integer (UNORM8)",
                "16-bit integer (UNORM16)",
                "16-bit float (Float16)",
                "32-bit float (Float32)",
            ]
        };
        let items = labels.map(|label| DropdownItem {
            label,
            enabled: true,
        });
        let selected = match self.precision {
            ChannelPrecision::Unorm8 => 0,
            ChannelPrecision::Unorm16 => 1,
            ChannelPrecision::Float16 => 2,
            ChannelPrecision::Float32 => 3,
        };
        // Space opens the focused field. Enter applies the dialog when closed;
        // when open, Enter belongs solely to the dropdown's selection.
        let mut field_input = input.input_view();
        if !was_open && input.key_presses().any(|(key, _, _)| key == Key::Enter) {
            field_input.keys = &[];
            field_input.key_repeats = &[];
        }
        let mut overlay = list.child();
        if let Some(index) = self.dropdown.draw_grouped(
            &mut overlay,
            atlas,
            layout.value,
            Rect::new(0., 0., size[0], size[1]),
            &items,
            selected,
            &field_input,
            true,
            true,
            &[],
            true,
        ) {
            self.precision = VALUES[index];
        }
        let neutral = Input::default();
        let footer_input = if was_open || self.dropdown.open || input.focus_lost {
            &neutral
        } else {
            input
        };
        let cancel = ui::button(
            list,
            atlas,
            layout.cancel,
            &word(locale, "取消", "Cancel"),
            footer_input,
            true,
        )
        .clicked;
        let apply = ui::button(
            list,
            atlas,
            layout.apply,
            &word(locale, "应用", "Apply"),
            footer_input,
            true,
        )
        .clicked;
        list.append(overlay);
        if cancel {
            return Some(Event::Cancelled);
        }
        if apply || (!was_open && !self.dropdown.open && key(Key::Enter)) {
            return Some(Event::ProjectPrecision(self.precision));
        }
        None
    }
}

#[cfg(test)]
mod tests;

impl Dialog {
    pub(crate) fn project_precision(state: &Value) -> Result<Self, String> {
        let precision = serde_json::from_value(state["project"]["working_precision"].clone())
            .map_err(|error| format!("project precision field is invalid: {error}"))?;
        let mut dialog = Self::settings(state);
        dialog.project_precision = Some(Editor {
            precision,
            dropdown: Default::default(),
        });
        Ok(dialog)
    }
}
