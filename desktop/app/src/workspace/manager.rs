use super::*;
use motion_ui::{
    paint,
    ui::{self, Align},
    widgets::{Checkbox, TextEditor},
    Key, PaintList, TextAtlas,
};
pub struct Manager {
    visible: std::collections::BTreeSet<DockId>,
    pub name: TextEditor,
    pub error: Option<String>,
    scroll: motion_ui::widgets::ScrollArea,
}
pub enum ManagerEvent {
    Cancel,
    Apply { visible: Vec<DockId>, name: String },
    Preset(Preset),
}
impl Manager {
    pub fn new(workspace: &Workspace) -> Self {
        Self {
            visible: workspace
                .owned_panels()
                .into_iter()
                .filter(|id| !workspace.hidden.contains(id))
                .collect(),
            name: Default::default(),
            error: None,
            scroll: Default::default(),
        }
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: crate::i18n::Locale,
    ) -> Option<ManagerEvent> {
        let sheet = list.styles();
        let zh = locale == crate::i18n::Locale::Zh;
        let height = 748_f32.min(size[1] - 64.);
        let card = Rect::new(
            (size[0] - 700.) * 0.5,
            (size[1] - height) * 0.5,
            700.,
            height,
        );
        if input.key_presses().any(|(k, _, _)| k == Key::Escape) {
            return Some(ManagerEvent::Cancel);
        }
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("well").with_alpha(0.9),
        );
        motion_ui::style::box_style(
            list,
            card,
            sheet.resolve("popup", Default::default()).unwrap(),
        );
        ui::text_sized(
            list,
            atlas,
            Rect::new(card.min[0] + 20., card.min[1] + 20., 600., 32.),
            if zh {
                "面板与工作区"
            } else {
                "Panels and workspaces"
            },
            sheet.color("text"),
            Align::Left,
            18.,
        );
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., card.min[1] + 58., 660., 28.),
            if zh {
                "图层、时间轴与曲线作为一个联动编辑区；其余面板按职责独立。"
            } else {
                "Layers, timeline and graph stay linked; other panels dock independently."
            },
            sheet.color("muted"),
            Align::Left,
        );
        let mut y = card.min[1] + 112.;
        let list_height = (height - 304.).clamp(144., 264.);
        let viewport = Rect::new(card.min[0] + 20., y, 660., list_height);
        let frame = self.scroll.begin(list, viewport, 264., input, false);
        let mut content = list.child();
        for (i, panel) in dock::Panel::available().enumerate() {
            let x = card.min[0] + 20. + (i % 2) as f32 * 330.;
            let row = Rect::new(x, frame.content.min[1] + (i / 2) as f32 * 24., 310., 22.);
            if Checkbox::draw(
                &mut content,
                atlas,
                row,
                if zh { panel.title_zh() } else { panel.title() },
                self.visible.contains(&panel.dock()),
                &frame.input,
                true,
                false,
                None,
            )
            .clicked
            {
                if !self.visible.remove(&panel.dock()) {
                    self.visible.insert(panel.dock());
                }
            }
        }
        content.clip_to(frame.viewport);
        list.append(content);
        y += list_height + 20.;
        paint::rect(
            list,
            Rect::new(card.min[0] + 20., y, 660., 0.5),
            sheet.color("border"),
        );
        y += 12.;
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., y, 660., 24.),
            if zh { "工作区" } else { "Workspace" },
            sheet.color("text"),
            Align::Left,
        );
        y += 28.;
        let mut x = card.min[0] + 20.;
        for preset in Preset::ALL {
            let text = preset.title(locale);
            let width = atlas.measure(text) + 24.;
            if ui::button(list, atlas, Rect::new(x, y, width, 28.), text, input, true).clicked {
                return Some(ManagerEvent::Preset(preset));
            }
            x += width + 8.;
        }
        y += 42.;
        let field = Rect::new(card.min[0] + 150., y, 530., 28.);
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 20., y, 122., 28.),
            if zh {
                "保存工作区名称"
            } else {
                "Save layout as"
            },
            sheet.color("muted"),
            Align::Left,
        );
        if input.pressed.is_some_and(|(_, p)| field.contains(p)) && !self.name.active() {
            let value = self.name.text.clone();
            self.name.begin(&value);
        }
        let _ = self.name.input(input);
        self.name.draw(list, atlas, field);
        if let Some(error) = &self.error {
            ui::text(
                list,
                atlas,
                Rect::new(card.min[0] + 20., card.max[1] - 84., 660., 24.),
                error,
                sheet.color("error"),
                Align::Left,
            );
        }
        let y = card.max[1] - 50.;
        if ui::button(
            list,
            atlas,
            Rect::new(card.max[0] - 202., y, 80., 30.),
            if zh { "取消" } else { "Cancel" },
            input,
            true,
        )
        .clicked
        {
            return Some(ManagerEvent::Cancel);
        }
        if ui::button_styled(
            list,
            atlas,
            Rect::new(card.max[0] - 112., y, 92., 30.),
            if zh { "保存" } else { "Save" },
            input,
            true,
            &sheet,
            "button.primary",
        )
        .clicked
        {
            return Some(ManagerEvent::Apply {
                visible: self.visible.iter().copied().collect(),
                name: self.name.text.trim().into(),
            });
        }
        None
    }
}
