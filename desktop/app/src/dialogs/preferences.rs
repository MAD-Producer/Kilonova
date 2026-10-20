//! Draft-only preferences. Applying belongs to the shell; cancel has no side effects.
use super::*;
use crate::{
    i18n::Text as T,
    workspace::{
        preferences::{Preferences, PreviewQuality},
        Workspace,
    },
};
use motion_ui::widgets::{Checkbox, Dropdown, DropdownItem, ScrollArea};

mod shortcuts;
#[cfg(test)]
mod tests;

pub(crate) struct Editor {
    pub draft: Preferences,
    page: Page,
    language: Dropdown,
    preview: Dropdown,
    scroll: ScrollArea,
    shortcuts: shortcuts::Shortcuts,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Page {
    #[default]
    General,
    Interface,
    Shortcuts,
}
impl Editor {
    fn new(workspace: &Workspace) -> Self {
        Self {
            draft: workspace.preferences(),
            page: Page::General,
            language: Default::default(),
            preview: Default::default(),
            scroll: Default::default(),
            shortcuts: Default::default(),
        }
    }
    pub(crate) fn import_shortcuts(&mut self, next: crate::shortcuts::bindings::Bindings) {
        self.shortcuts.replace(&mut self.draft.shortcuts, next);
    }
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
        error: Option<&str>,
    ) -> Option<Event> {
        let mut consumed = Input::default();
        let closing = !input.composing
            && !input.ime_consumed
            && input.key_presses().any(|(key, _, _)| key == Key::Escape);
        let input = if input.focus_lost
            || closing && (self.language.open || self.preview.open || self.shortcuts.owns_input())
        {
            consumed.focus_lost = input.focus_lost;
            self.language.open = false;
            self.preview.open = false;
            self.shortcuts.close();
            &consumed
        } else {
            input
        };
        if closing && !std::ptr::eq(input, &consumed) {
            return Some(Event::Cancelled);
        }
        let sheet = list.styles();
        let pad = sheet.metric("preferences.margin", 16.);
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let section = sheet.metric("preferences.section_gap", 12.);
        let width = sheet
            .metric("preferences.width", 512.)
            .min((size[0] - pad * 2.).max(0.));
        let height = sheet
            .metric("preferences.max_height", 492.)
            .min((size[1] - pad * 2.).max(0.));
        let card = Rect::new(
            (size[0] - width) * 0.5,
            (size[1] - height) * 0.5,
            width,
            height,
        );
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("shadow"),
        );
        static MISSING: crate::fallback_observation::Counter =
            crate::fallback_observation::Counter::new();
        if let Some(style) = sheet.resolve("popup", Default::default()) {
            MISSING.success();
            motion_ui::style::box_style(list, card, style);
        } else {
            MISSING.failure(
                "preferences.popup",
                format_args!("style=popup fallback=panel"),
            );
            paint::rect(list, card, sheet.color("panel"));
        }
        let x = card.min[0] + pad;
        let right = card.max[0] - pad;
        ui::text(
            list,
            atlas,
            Rect::new(x, card.min[1] + pad, right - x, row),
            &word(locale, "偏好设置", "Preferences"),
            sheet.color("text"),
            Align::Left,
        );
        let hint = Rect::new(x, card.min[1] + pad + row, right - x, row);
        ui::text(
            list,
            atlas,
            hint,
            &word(
                locale,
                "应用到所有窗口，独立于工程保存。",
                "Applies to all windows; saved separately from projects.",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        let footer = card.max[1] - pad - row;
        let mut area = Rect::new(
            x,
            hint.max[1] + section,
            right - x,
            (footer - hint.max[1] - section * 2. - row * 2.).max(0.),
        );
        let dropdown_owned = self.language.open || self.preview.open || self.shortcuts.owns_input();
        let neutral = Input::default();
        let nav_input = if dropdown_owned { &neutral } else { input };
        let compact = width < sheet.metric("preferences.sidebar_breakpoint", 440.);
        let nav_width = if compact {
            (area.width() - gap * 2.) / 3.
        } else {
            sheet.metric("preferences.sidebar_width", 100.)
        };
        let navigation = [
            (0, Page::General, word(locale, "常规", "General")),
            (1, Page::Interface, word(locale, "界面与编辑", "Interface")),
            (2, Page::Shortcuts, word(locale, "快捷键", "Shortcuts")),
        ]
        .map(|(index, page, title)| {
            let rect = if compact {
                Rect::new(
                    area.min[0] + index as f32 * (nav_width + gap),
                    area.min[1],
                    nav_width,
                    row,
                )
            } else {
                Rect::new(
                    area.min[0],
                    area.min[1] + index as f32 * (row + gap),
                    nav_width,
                    row,
                )
            };
            (page, title, rect)
        });
        let draw_navigation = |paint: &mut PaintList, atlas: &mut TextAtlas, current: Page| {
            let mut clicked = None;
            for (page, title, rect) in &navigation {
                let state = if current == *page {
                    motion_ui::style::States::default().with(motion_ui::style::State::Selected)
                } else {
                    Default::default()
                };
                if ui::button_with_states(
                    paint, atlas, *rect, title, nav_input, true, &sheet, "row", state,
                )
                .clicked
                {
                    clicked = Some(*page);
                }
            }
            clicked
        };
        let mut nav_paint = list.child();
        if let Some(page) =
            draw_navigation(&mut nav_paint, atlas, self.page).filter(|page| *page != self.page)
        {
            self.page = page;
            self.scroll = Default::default();
            // Category content and its selected row must agree in the click frame.
            nav_paint = list.child();
            draw_navigation(&mut nav_paint, atlas, self.page);
        }
        list.append(nav_paint);
        if compact {
            area.min[1] += row + gap;
        } else {
            paint::rect(
                list,
                Rect::new(
                    area.min[0] + nav_width + section * 0.5,
                    area.min[1],
                    sheet.metric("border", 1.),
                    area.height(),
                ),
                sheet.color("border"),
            );
            area.min[0] += nav_width + section;
        }
        let x = area.min[0];
        let content_height = if self.page == Page::Shortcuts {
            row * 11. + gap * 6.
        } else if self.page == Page::General {
            row * 2. + gap
        } else {
            row * 9. + gap * 8. + section
        };
        let frame = self.scroll.begin(
            list,
            area,
            content_height,
            if dropdown_owned { &neutral } else { input },
            true,
        );
        let viewport = frame.viewport;
        let mut content = list.child();
        let label_width = sheet
            .metric("preferences.label_width", 180.)
            .min(viewport.width() * 0.5);
        let value_width = (viewport.width() - label_width - gap).max(0.);
        let mut y = frame.content.min[1];
        let label = |y| Rect::new(x, y, label_width, row);
        let value = |y| Rect::new(x + label_width + gap, y, value_width, row);
        // A dropdown owns pointer/keyboard input until it closes.
        let body_input = if dropdown_owned {
            &neutral
        } else {
            &frame.input
        };
        let mut language_row = None;
        let mut preview_row = None;
        let action_width = sheet
            .metric("preferences.button_width", 96.)
            .min((viewport.width() - gap) * 0.5);
        let mut choose_style = false;
        let mut shortcut_event = None;
        if self.page == Page::Shortcuts {
            shortcut_event = self.shortcuts.draw(
                &mut content,
                atlas,
                frame.content,
                viewport,
                if self.shortcuts.owns_input() {
                    input
                } else {
                    &frame.input
                },
                locale,
                &mut self.draft.shortcuts,
            );
        } else if self.page == Page::General {
            ui::text(
                &mut content,
                atlas,
                label(y),
                &word(locale, "界面语言", "Interface language"),
                sheet.color("muted"),
                Align::Left,
            );
            language_row = Some(value(y));
            y += row + gap;
            ui::text(
                &mut content,
                atlas,
                label(y),
                &word(locale, "预览质量", "Preview quality"),
                sheet.color("muted"),
                Align::Left,
            );
            preview_row = Some(value(y));
        } else {
            ui::text(
                &mut content,
                atlas,
                Rect::new(x, y, viewport.width(), row),
                &word(locale, "编辑与显示", "Editing and display"),
                sheet.color("text"),
                Align::Left,
            );
            y += row + gap;
            for (label, selected) in [
                (
                    word(locale, "节点网格", "Node grid"),
                    &mut self.draft.node_view.grid,
                ),
                (
                    word(locale, "节点小地图", "Node minimap"),
                    &mut self.draft.node_view.minimap,
                ),
                (
                    word(locale, "时间轴混合模式列", "Timeline blend mode column"),
                    &mut self.draft.columns.blend,
                ),
                (
                    word(
                        locale,
                        "时间轴父级与链接列",
                        "Timeline parent and link column",
                    ),
                    &mut self.draft.columns.parent,
                ),
                (
                    word(locale, "时间轴遮罩列", "Timeline track matte column"),
                    &mut self.draft.columns.matte,
                ),
            ] {
                if Checkbox::draw(
                    &mut content,
                    atlas,
                    Rect::new(x, y, viewport.width(), row),
                    &label,
                    *selected,
                    body_input,
                    true,
                    false,
                    None,
                )
                .clicked
                {
                    *selected = !*selected;
                }
                y += row + gap;
            }
            y += section;
            ui::text(
                &mut content,
                atlas,
                Rect::new(x, y, viewport.width(), row),
                &word(locale, "样式", "Style"),
                sheet.color("text"),
                Align::Left,
            );
            y += row + gap;
            let path = self
                .draft
                .style_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| word(locale, "内置样式", "Bundled style"));
            ui::text(
                &mut content,
                atlas,
                Rect::new(x, y, viewport.width(), row),
                &path,
                sheet.color("muted"),
                Align::Left,
            );
            y += row + gap;
            choose_style = ui::button(
                &mut content,
                atlas,
                Rect::new(x, y, action_width, row),
                &word(locale, "选择样式…", "Choose style…"),
                body_input,
                true,
            )
            .clicked;
            if ui::button(
                &mut content,
                atlas,
                Rect::new(x + action_width + gap, y, action_width, row),
                &word(locale, "内置样式", "Bundled style"),
                body_input,
                self.draft.style_path.is_some(),
            )
            .clicked
            {
                self.draft.style_path = None;
            }
        }
        // Draw overlays after scrolling content so their actual hit boxes match paint.
        let languages = [
            DropdownItem {
                label: "English",
                enabled: true,
            },
            DropdownItem {
                label: "简体中文",
                enabled: true,
            },
        ];
        let labels = [
            locale.t(T::Auto),
            locale.t(T::Full),
            locale.t(T::Smooth),
            locale.t(T::Economy),
        ];
        let quality = labels.map(|label| DropdownItem {
            label,
            enabled: true,
        });
        let mut overlay = list.child();
        if let Some(language_row) = language_row {
            if let Some(index) = self.language.draw(
                &mut overlay,
                atlas,
                language_row,
                viewport,
                &languages,
                usize::from(self.draft.locale == Locale::Zh),
                if self.language.open {
                    input
                } else {
                    &frame.input
                },
                !self.preview.open,
                false,
            ) {
                self.draft.locale = if index == 0 { Locale::En } else { Locale::Zh };
            }
        }
        if let Some(preview_row) = preview_row {
            if let Some(index) = self.preview.draw(
                &mut overlay,
                atlas,
                preview_row,
                viewport,
                &quality,
                self.draft.preview.id() as usize,
                if self.preview.open {
                    input
                } else {
                    &frame.input
                },
                !self.language.open,
                false,
            ) {
                self.draft.preview = [
                    PreviewQuality::Auto,
                    PreviewQuality::High,
                    PreviewQuality::Balanced,
                    PreviewQuality::Economy,
                ][index];
            }
        }
        content.clip_to(viewport);
        overlay.clip_to(viewport);
        list.append(content);
        list.append(overlay);
        if let Some(error) = error {
            ui::text(
                list,
                atlas,
                Rect::new(
                    card.min[0] + pad,
                    footer - row * 2.,
                    width - pad * 2.,
                    row * 2.,
                ),
                error,
                sheet.color("error"),
                Align::Left,
            );
        }
        let footer_input = if dropdown_owned
            || self.language.open
            || self.preview.open
            || self.shortcuts.owns_input()
        {
            &neutral
        } else {
            input
        };
        let left = card.min[0] + pad;
        let footer_width = sheet
            .metric("preferences.button_width", 96.)
            .min(((right - left - gap * 2.) / 3.).max(0.));
        if ui::button(
            list,
            atlas,
            Rect::new(left, footer, footer_width, row),
            &word(locale, "恢复默认", "Reset defaults"),
            footer_input,
            true,
        )
        .clicked
        {
            let next = Preferences::default();
            self.shortcuts
                .replace(&mut self.draft.shortcuts, next.shortcuts.clone());
            let shortcuts = self.draft.shortcuts.clone();
            self.draft = next;
            self.draft.shortcuts = shortcuts;
        }
        if ui::button(
            list,
            atlas,
            Rect::new(right - footer_width * 2. - gap, footer, footer_width, row),
            &word(locale, "取消", "Cancel"),
            footer_input,
            true,
        )
        .clicked
        {
            return Some(Event::Cancelled);
        }
        if ui::button(
            list,
            atlas,
            Rect::new(right - footer_width, footer, footer_width, row),
            &word(locale, "应用", "Apply"),
            footer_input,
            true,
        )
        .clicked
        {
            return Some(Event::Preferences(self.draft.clone()));
        }
        shortcut_event.or_else(|| choose_style.then_some(Event::PreferencesStyle))
    }
}

impl Dialog {
    pub(crate) fn preferences(workspace: &Workspace) -> Self {
        let mut dialog = Self::settings(&Value::Null);
        dialog.preferences = Some(Editor::new(workspace));
        dialog
    }
}
