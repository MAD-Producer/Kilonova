//! Toolbar rendering and icon buttons
use motion_ui::{
    icons::{self, Icon},
    style::{State, States},
};
use motion_ui::{
    input::{Input, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::metrics,
};
use serde_json::Value;

use super::types::{Action, Panels, Tool};

/// Render the main toolbar
pub fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();
    let paint_input = input;
    let neutral = Input::default();
    let input = if panels.toolbar_style.editing() { &neutral } else { input };

    use crate::panels::path_controls::Tool as PathTool;
    let area = Rect::new(
        0.,
        metrics::TITLE_BAR + metrics::MENU_BAR,
        size[0],
        metrics::TOOL_BAR,
    );
    paint::rect(list, area, colors.CHROME);
    paint::rect(
        list,
        Rect::new(0., area.max[1] - 1., size[0], 1.),
        colors.BORDER,
    );
    let locale = panels.locale;
    use super::tools::{self, Active};
    let step = styles.metric("toolbar.tools.step", 28.);
    let side = styles.metric("toolbar.tools.size", 24.);
    let top = styles.metric("toolbar.tools.top", 4.);
    let start = styles.metric("toolbar.tools.start", 8.);
    let transforms = styles.metric("toolbar.transform.start", 96.);
    let actions = styles.metric("toolbar.actions.start", 180.);
    for (i, tool, icon) in [
        (0, Active::Select, Icon::Select),
        (1, Active::Pen, Icon::Pen),
        (2, Active::Mask, Icon::Shape),
        (3, Active::Rotate, Icon::Rotate),
        (4, Active::Scale, Icon::Scale),
        (5, Active::Anchor, Icon::Anchor),
        (
            8,
            if tools::active(panels) == Active::Ellipse {
                Active::Ellipse
            } else {
                Active::Rectangle
            },
            Icon::Shape,
        ),
        (9, Active::Text, Icon::Text),
    ] {
        let x = if i < 3 {
            start + i as f32 * step
        } else if i < 6 {
            transforms + (i - 3) as f32 * step
        } else {
            actions + (i - 6) as f32 * step
        };
        let side = if i >= 6 {
            styles.metric("toolbar.actions.size", 25.)
        } else {
            side
        };
        let top = if i >= 6 {
            styles.metric("toolbar.actions.top", 3.)
        } else {
            top
        };
        let rect = Rect::new(x, area.min[1] + top, side, side);
        let states = if tools::active(panels) == tool {
            States::default().with(State::Selected)
        } else {
            States::default()
        };
        if icons::button(list, rect, icon, input, &styles, states).clicked {
            let tool = if matches!(tool, Active::Rectangle | Active::Ellipse)
                && tools::active(panels) == tool
            {
                if tool == Active::Rectangle {
                    Active::Ellipse
                } else {
                    Active::Rectangle
                }
            } else {
                tool
            };
            tools::choose(panels, tool, state, out);
        }
        if rect.contains(paint_input.mouse) && !panels.toolbar_style.editing() {
            panels.drag_readout = Some((paint_input.mouse, tools::hint(tool, panels)));
        }
    }
    for (i, (tool, action, enabled)) in [
        (
            Tool::Undo,
            Action::History(0),
            tools::camera::history_enabled(panels,state,false),
        ),
        (
            Tool::Redo,
            Action::History(1),
            tools::camera::history_enabled(panels,state,true),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        icon_button(
            list,
            Rect::new(
                actions + i as f32 * step,
                area.min[1] + styles.metric("toolbar.actions.top", 3.),
                styles.metric("toolbar.actions.size", 25.),
                styles.metric("toolbar.actions.size", 25.),
            ),
            tool,
            input,
            enabled,
            action,
            out,
        );
    }
    paint::rect(
        list,
        Rect::new(styles.metric("toolbar.separator.x", 299.), area.min[1] + styles.metric("toolbar.separator.top",6.),
            styles.metric("toolbar.separator.width", 1.), styles.metric("toolbar.separator.height", 20.)),
        colors.BORDER,
    );
    let ids = panels.selection_ids();
    panels.toolbar_style.render(list, atlas, area, state, &ids,
        panels.path_controls.tool == PathTool::Mask, paint_input, locale, out);
    if let Some(notice) = &panels.toolbar_style.notice { panels.message = Some(notice.clone()); }
    if let Some(cursor) = panels.toolbar_style.cursor { panels.cursor = Some(cursor); }
    let right = (size[0] - styles.metric("toolbar.workspace.reserve", 340.))
        .max(super::toolbar_style::end(&styles, atlas, locale) + styles.metric("padding.panel", 8.));
    let mut x = right;
    for preset in crate::workspace::Preset::ALL {
        let text = preset.title(locale);
        let width = (atlas.measure(text) + 16.).max(38.);
        let rect = Rect::new(x, area.min[1] + 4., width, 24.);
        if rect.max[0] > size[0] - styles.metric("toolbar.workspace.overflow_width", 34.) {
            break;
        }
        let states = if preset == panels.workspace_preset {
            States::default().with(State::Selected)
        } else {
            States::default()
        };
        if motion_ui::ui::button_with_states(
            list,
            atlas,
            rect,
            text,
            input,
            true,
            &styles,
            "button.icon",
            states,
        )
        .clicked
        {
            out.push(Action::Workspace(preset));
        }
        x += width + 2.;
    }
    if motion_ui::ui::button_styled(
        list,
        atlas,
        Rect::new((size[0] - 34.).max(0.), area.min[1] + 4., 28., 24.),
        "»",
        input,
        true,
        &styles,
        "button.icon",
    )
    .clicked
    {
        out.push(Action::ManageWorkspace);
    }
}

/// Render an icon button
pub(super) fn icon_button(
    list: &mut PaintList,
    r: Rect,
    tool: Tool,
    input: &Input,
    enabled: bool,
    action: Action,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    // Use the current Ardot SVGs wherever there is a matching canonical glyph.
    let svg = match tool {
        Tool::Open => Some(Icon::Folder),
        Tool::Undo => Some(Icon::Undo),
        Tool::Redo => Some(Icon::Redo),
        Tool::Text => Some(Icon::Text),
        Tool::Rectangle => Some(Icon::Shape),
        Tool::Play => Some(Icon::Play),
        Tool::Pause => Some(Icon::Pause),
        Tool::First => Some(Icon::Previous),
        Tool::Last => Some(Icon::Next),
        _ => None,
    };
    if let Some(icon) = svg {
        let states = if enabled {
            States::default()
        } else {
            States::default().with(State::Disabled)
        };
        if icons::button(list, r, icon, input, styles.as_ref(), states).clicked {
            out.push(action);
        }
        return;
    }
    let hovered = enabled && r.contains(input.mouse);
    if hovered {
        paint::rect(list, r, colors.CHROME);
        paint::stroke(list, r, colors.BORDER, 1.0);
    }
    let c = if enabled {
        colors.TEXT
    } else {
        colors.TEXT_MUTED.with_alpha(0.45)
    };
    let x = r.min[0] + 8.0;
    let y = r.min[1] + 6.0;
    let line = |list: &mut PaintList, points: &[[f32; 2]]| {
        paint::line(
            list,
            points.iter().map(|p| [x + p[0], y + p[1]]).collect(),
            c,
        )
    };
    match tool {
        Tool::Open => {
            line(
                list,
                &[
                    [0., 16.],
                    [0., 3.],
                    [6., 3.],
                    [8., 6.],
                    [18., 6.],
                    [18., 16.],
                    [0., 16.],
                ],
            );
        }
        Tool::Save => {
            paint::stroke(list, Rect::new(x, y, 17., 17.), c, 1.);
            paint::stroke(list, Rect::new(x + 4., y + 1., 9., 5.), c, 1.);
            paint::stroke(list, Rect::new(x + 4., y + 10., 9., 6.), c, 1.);
        }
        Tool::Undo => {
            line(list, &[[4., 4.], [0., 8.], [4., 12.]]);
            line(list, &[[0., 8.], [12., 8.], [16., 12.], [16., 16.]]);
        }
        Tool::Redo => {
            line(list, &[[12., 4.], [16., 8.], [12., 12.]]);
            line(list, &[[16., 8.], [4., 8.], [0., 12.], [0., 16.]]);
        }
        Tool::Solid => paint::rect(list, Rect::new(x + 1., y + 1., 16., 16.), c),
        Tool::Text => {
            // "T" icon for text layer
            line(list, &[[2., 2.], [14., 2.]]);
            line(list, &[[8., 2.], [8., 16.]]);
            line(list, &[[5., 16.], [11., 16.]]);
        }
        Tool::Null => {
            // Circle with crosshair for null object
            paint::stroke(list, Rect::new(x + 2., y + 2., 14., 14.), c, 1.);
            line(list, &[[9., 4.], [9., 14.]]);
            line(list, &[[4., 9.], [14., 9.]]);
        }
        Tool::Rectangle => {
            // Rectangle shape icon
            paint::stroke(list, Rect::new(x + 2., y + 2., 14., 14.), c, 1.);
        }
        Tool::Ellipse => {
            // Circle shape icon
            let center_x = x + 9.;
            let center_y = y + 9.;
            let radius = 7.;
            for i in 0..16 {
                let angle1 = (i as f32 / 16.0) * std::f32::consts::PI * 2.0;
                let angle2 = ((i + 1) as f32 / 16.0) * std::f32::consts::PI * 2.0;
                line(
                    list,
                    &[
                        [
                            center_x - x + radius * angle1.cos(),
                            center_y - y + radius * angle1.sin(),
                        ],
                        [
                            center_x - x + radius * angle2.cos(),
                            center_y - y + radius * angle2.sin(),
                        ],
                    ],
                );
            }
        }
        Tool::Play => {
            list.push_shape(
                paint::Shape::Quad {
                    points: [
                        [x + 3., y],
                        [x + 16., y + 8.],
                        [x + 3., y + 16.],
                        [x + 3., y + 16.],
                    ],
                },
                c,
            );
        }
        Tool::Pause => {
            paint::rect(list, Rect::new(x + 2., y, 5., 16.), c);
            paint::rect(list, Rect::new(x + 11., y, 5., 16.), c);
        }
        Tool::First => {
            line(list, &[[5., 0.], [5., 16.]]);
            line(list, &[[16., 0.], [7., 8.], [16., 16.]]);
        }
        Tool::Last => {
            line(list, &[[13., 0.], [13., 16.]]);
            line(list, &[[2., 0.], [11., 8.], [2., 16.]]);
        }
    }
    if super::helpers::clicked(input, r) && enabled {
        out.push(action);
    }
}
