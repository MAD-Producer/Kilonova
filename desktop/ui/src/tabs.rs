//! Content-width panel tabs with inline actions and optional selection context.
use crate::{
    icons::{self, Icon},
    input::MouseButton,
    paint,
    style::States,
    ui::{self, Align},
    Color, PaintList, Rect, TextAtlas,
};

pub struct Tab<'a> {
    pub title: &'a str,
    pub context: Option<&'a str>,
    pub badge: Option<Color>,
    pub locked: bool,
    pub closable: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Activate(usize),
    Close(usize),
    Menu(usize),
}
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: Rect,
    pub close: Option<Rect>,
    pub menu: Option<Rect>,
}

/// Prefer actual content widths; shorten labels only when the group overflows.
pub fn layouts(atlas: &mut TextAtlas, area: Rect, tabs: &[Tab<'_>], active: usize) -> Vec<Layout> {
    let widths: Vec<_> = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            12. + atlas.measure(t.title)
                + t.context.map_or(0., |s| atlas.measure(s) + 4.)
                + if t.badge.is_some() { 14. } else { 0. }
                + if t.locked { 16. } else { 0. }
                + if i == active {
                    18. + if t.closable { 14. } else { 0. }
                } else {
                    0.
                }
        })
        .collect();
    let total: f32 = widths.iter().sum();
    let floor = 42_f32.min(area.width() / tabs.len().max(1) as f32);
    let spare = (area.width() - floor * tabs.len() as f32).max(0.);
    let weighted: f32 = widths.iter().map(|w| (w - floor).max(0.)).sum();
    let mut x = area.min[0];
    widths
        .into_iter()
        .enumerate()
        .map(|(i, w)| {
            let width = if total <= area.width() {
                w
            } else {
                floor + spare * (w - floor).max(0.) / weighted.max(1.)
            };
            let r = Rect::new(
                x,
                area.min[1],
                width.min((area.max[0] - x).max(0.)),
                area.height(),
            );
            x = r.max[0];
            Layout {
                area: r,
                close: (i == active && tabs[i].closable)
                    .then(|| Rect::new(r.min[0] + 3., r.min[1] + 5., 12., 16.)),
                menu: (i == active).then(|| Rect::new(r.max[0] - 19., r.min[1] + 4., 16., 18.)),
            }
        })
        .collect()
}
pub fn draw(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    tabs: &[Tab<'_>],
    active: usize,
    input: &impl crate::InputSource,
    underline: bool,
) -> Option<Event> {
    let view = input.input_view();
    let input = &view;
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.HEADER);
    paint::rect(
        list,
        Rect::new(area.min[0], area.max[1] - 1., area.width(), 1.),
        colors.BORDER,
    );
    let rects = layouts(atlas, area, tabs, active);
    let mut event = None;
    for (i, (tab, layout)) in tabs.iter().zip(rects).enumerate() {
        let mut x = layout.area.min[0] + 6.;
        if let Some(close) = layout.close {
            icons::draw(list, close, Icon::Close, 10., colors.TEXT_MUTED);
            if pressed(input, close) {
                event = Some(Event::Close(i));
            }
            x = close.max[0] + 3.;
        }
        if let Some(color) = tab.badge {
            paint::rect(list, Rect::new(x, area.min[1] + 9., 8., 8.), color);
            x += 14.;
        }
        if tab.locked {
            icons::draw(
                list,
                Rect::new(x, area.min[1] + 5., 12., 16.),
                Icon::Lock,
                12.,
                colors.TEXT_MUTED,
            );
            x += 16.;
        }
        let end = layout
            .menu
            .map_or(layout.area.max[0] - 6., |m| m.min[0] - 2.);
        let header = styles.resolve("panel.header", States::default()).unwrap();
        let title_width = (atlas.measure(tab.title) * header.font_size / atlas.font_size())
            .min((end - x).max(0.));
        ui::text_sized(
            list,
            atlas,
            Rect::new(x, area.min[1], title_width, area.height()),
            tab.title,
            if i == active {
                colors.TEXT
            } else {
                colors.TEXT_MUTED
            },
            Align::Left,
            header.font_size,
        );
        x += title_width + 4.;
        if let Some(context) = tab.context {
            ui::text(
                list,
                atlas,
                Rect::new(x, area.min[1], (end - x).max(0.), area.height()),
                context,
                if i == active {
                    colors.ACCENT
                } else {
                    colors.TEXT_MUTED
                },
                Align::Left,
            );
        }
        if let Some(menu) = layout.menu {
            let color = styles
                .as_ref()
                .resolve("icon", States::default())
                .map_or(colors.TEXT_MUTED, |s| s.foreground);
            icons::draw(list, menu, Icon::Menu, 12., color);
            if pressed(input, menu) {
                event = Some(Event::Menu(i));
            }
        }
        if i == active && underline {
            paint::rect(
                list,
                Rect::new(
                    layout.area.min[0] + 6.,
                    area.max[1] - 2.,
                    (layout.area.width() - 12.).max(0.),
                    1.,
                ),
                colors.TEXT,
            );
        }
        let action_hit = layout.close.is_some_and(|r| r.contains(input.mouse))
            || layout.menu.is_some_and(|r| r.contains(input.mouse));
        if !action_hit && pressed(input, layout.area) {
            event = Some(Event::Activate(i));
        }
    }
    event
}
fn pressed(input: &impl crate::InputSource, area: Rect) -> bool {
    let view = input.input_view();
    let input = &view;
    matches!(input.pressed,Some((MouseButton::Left,p)) if area.contains(p))
}
