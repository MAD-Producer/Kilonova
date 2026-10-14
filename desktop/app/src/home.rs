//! Project landing page. Rows represent real local projects, never demo data.
use crate::{i18n::Locale, panels::Action, workspace::RecentProject};
use motion_ui::{
    paint,
    style::{box_style, text_style, States},
    ui::{self, Align},
    widgets::{ScrollArea, SearchField},
    Input, PaintList, Rect, TextAtlas,
};
#[derive(Default)]
pub struct Home {
    tab: usize,
    query: String,
    search: SearchField,
    scroll: ScrollArea,
}
impl Home {
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        recent: &[RecentProject],
        input: &Input,
        locale: Locale,
    ) -> Vec<Action> {
        let sheet = list.styles();
        let zh = locale == Locale::Zh;
        let mut actions = vec![];
        if !input.composing && !input.ime_consumed {
            if motion_ui::input::KeyChord::new(motion_ui::Key::N, true, false, false).pressed(input)
            {
                actions.push(Action::NewComposition);
            }
            if motion_ui::input::KeyChord::new(motion_ui::Key::O, true, false, false).pressed(input)
            {
                actions.push(Action::Open);
            }
        }

        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            sheet.color("well"),
        );
        let sidebar = Rect::new(0., 24., 224., (size[1] - 24.).max(0.));
        paint::rect(list, sidebar, sheet.color("panel"));
        ui::text_sized(
            list,
            atlas,
            Rect::new(24., 36., 24., 32.),
            "M",
            sheet.color("focus"),
            Align::Left,
            20.,
        );
        ui::text_sized(
            list,
            atlas,
            Rect::new(52., 36., 160., 32.),
            "Motion Studio",
            sheet.color("text"),
            Align::Left,
            16.,
        );
        for (i, (text, action)) in [
            (if zh { "新建工程" } else { "New project" }, Action::New),
            (
                if zh {
                    "打开工程…"
                } else {
                    "Open project…"
                },
                Action::Open,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if ui::button_styled(
                list,
                atlas,
                Rect::new(24., 104. + i as f32 * 48., 176., 32.),
                text,
                input,
                true,
                &sheet,
                if i == 0 { "button.primary" } else { "button" },
            )
            .clicked
            {
                actions.push(action);
            }
        }
        for (i, text) in [
            if zh { "主页" } else { "Home" },
            if zh { "最近" } else { "Recent" },
            if zh { "模板" } else { "Templates" },
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::new(24., 200. + i as f32 * 44., 176., 28.);
            let mut style = sheet.resolve("row", States::default()).unwrap();
            if self.tab == i {
                style.background = sheet.color("hover");
            }
            box_style(list, rect, style);
            text_style(list, atlas, rect.shrink(6.), text, style, Align::Left);
            if input.pressed.is_some_and(|(_, p)| rect.contains(p)) {
                self.tab = i;
                self.scroll.offset = 0.;
            }
        }
        let x = 272.;
        let width = (size[0] - x - 48.).max(0.);
        ui::text_sized(
            list,
            atlas,
            Rect::new(x, 48., width, 42.),
            if zh {
                "欢迎使用 Motion Studio"
            } else {
                "Welcome to Motion Studio"
            },
            sheet.color("text"),
            Align::Left,
            24.,
        );
        for (i, (text, action)) in [
            (
                if zh {
                    "新建合成"
                } else {
                    "New composition"
                },
                Action::NewComposition,
            ),
            (if zh { "导入工程" } else { "Import project" }, Action::Open),
        ]
        .into_iter()
        .enumerate()
        {
            if ui::button(
                list,
                atlas,
                Rect::new(x + i as f32 * 156., 112., 144., 36.),
                text,
                input,
                true,
            )
            .clicked
            {
                actions.push(action);
            }
        }
        if self.tab == 2 {
            ui::text_sized(
                list,
                atlas,
                Rect::new(x, 180., width, 32.),
                if zh {
                    "新建画布预设"
                } else {
                    "Composition presets"
                },
                sheet.color("text"),
                Align::Left,
                16.,
            );
            for (i, (label, w, h)) in [
                ("HD · 1920 × 1080 · 30 fps", 1920, 1080),
                ("Portrait · 1080 × 1920 · 30 fps", 1080, 1920),
                ("Square · 1080 × 1080 · 30 fps", 1080, 1080),
            ]
            .into_iter()
            .enumerate()
            {
                if ui::button(
                    list,
                    atlas,
                    Rect::new(x, 232. + i as f32 * 48., width.min(480.), 36.),
                    label,
                    input,
                    true,
                )
                .clicked
                {
                    actions.push(Action::CompositionPreset(w, h));
                }
            }
        } else {
            ui::text_sized(
                list,
                atlas,
                Rect::new(x, 180., width, 32.),
                if zh {
                    "最近项目"
                } else {
                    "Recent projects"
                },
                sheet.color("text"),
                Align::Left,
                16.,
            );
            self.search.draw(
                list,
                atlas,
                Rect::new(size[0] - 328., 180., 280., 28.),
                &mut self.query,
                if zh {
                    "搜索最近项目"
                } else {
                    "Search recent projects"
                },
                input,
                true,
            );
            let filtered: Vec<_> = recent
                .iter()
                .filter(|p| p.name.to_lowercase().contains(&self.query.to_lowercase()))
                .collect();
            let area = Rect::new(x, 264., width, (size[1] - 352.).max(0.));
            let frame = self
                .scroll
                .begin(list, area, filtered.len() as f32 * 68., input, false);
            let mut rows = list.child();
            for (i, p) in filtered.iter().enumerate() {
                let rect = Rect::new(x, frame.content.min[1] + i as f32 * 68., width, 68.);
                if rect.max[1] < area.min[1] || rect.min[1] > area.max[1] {
                    continue;
                }
                paint::rect(&mut rows, rect, sheet.color("panel"));
                paint::rect(
                    &mut rows,
                    Rect::new(x + 12., rect.min[1] + 12., 76., 44.),
                    sheet.color("well"),
                );
                motion_ui::icons::draw(
                    &mut rows,
                    Rect::new(x + 34., rect.min[1] + 20., 28., 28.),
                    motion_ui::icons::Icon::Folder,
                    18.,
                    sheet.color("icon"),
                );
                text_style(
                    &mut rows,
                    atlas,
                    Rect::new(x + 104., rect.min[1] + 8., width * 0.5, 26.),
                    &p.name,
                    sheet.resolve("label", States::default()).unwrap(),
                    Align::Left,
                );
                text_style(
                    &mut rows,
                    atlas,
                    Rect::new(x + 104., rect.min[1] + 32., width * 0.5, 20.),
                    if zh {
                        "本地 / Motion Studio"
                    } else {
                        "Local / Motion Studio"
                    },
                    sheet.resolve("label.caption", States::default()).unwrap(),
                    Align::Left,
                );
                text_style(
                    &mut rows,
                    atlas,
                    Rect::new(rect.max[0] - 250., rect.min[1], 240., 68.),
                    &format!("{} × {} · {} fps", p.width, p.height, p.fps),
                    sheet.resolve("label.muted", States::default()).unwrap(),
                    Align::Left,
                );
                if frame
                    .input
                    .pressed
                    .is_some_and(|(_, point)| rect.contains(point))
                {
                    actions.push(Action::OpenPath(p.root.clone()));
                }
            }
            if filtered.is_empty() {
                text_style(
                    &mut rows,
                    atlas,
                    area,
                    if self.query.is_empty() {
                        if zh {
                            "还没有项目 · 点击「新建合成」开始"
                        } else {
                            "No recent projects · Create a composition to begin"
                        }
                    } else {
                        if zh {
                            "没有匹配的项目"
                        } else {
                            "No matching projects"
                        }
                    },
                    sheet.resolve("label.muted", States::default()).unwrap(),
                    Align::Center,
                );
            }
            rows.clip_to(frame.viewport);
            list.append(rows);
        }
        text_style(
            list,
            atlas,
            Rect::new(x, size[1] - 70., width, 24.),
            if zh {
                "Motion Studio · 本地创作空间"
            } else {
                "Motion Studio · Local creative workspace"
            },
            sheet.resolve("label.caption", States::default()).unwrap(),
            Align::Left,
        );
        actions
    }
}
