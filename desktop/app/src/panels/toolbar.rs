//! Toolbar rendering and icon buttons
use motion_ui::{
    icons::{self, Icon},
    style::{default_stylesheet, State, States},
};
use motion_ui::{
    input::{Input, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::{metrics, palette},
};
use serde_json::Value;

use super::{
    helpers::{button, label},
    types::{Action, Panels, Tool},
};
use crate::i18n::{Locale, Text as T};

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
    use crate::panels::path_controls::Tool as PathTool;
    let area = Rect::new(0., metrics::MENU_BAR, size[0], metrics::TOOL_BAR);
    paint::rect(list, area, palette::CHROME);
    paint::rect(
        list,
        Rect::new(0., area.max[1] - 1., size[0], 1.),
        palette::BORDER,
    );
    let locale = panels.locale;
    for (i, tool) in [PathTool::Select, PathTool::Pen, PathTool::Mask]
        .into_iter()
        .enumerate()
    {
        let rect = Rect::new(8. + i as f32 * 28., area.min[1] + 4., 24., 24.);
        let states = if panels.path_controls.tool == tool && (tool!=PathTool::Select||panels.transform_controls.tool==super::transform_controls::Tool::Move) {
            States::default().with(State::Selected)
        } else {
            States::default()
        };
        let icon = match tool {
            PathTool::Select => Icon::Select,
            PathTool::Pen => Icon::Pen,
            PathTool::Mask => Icon::Shape,
        };
        if icons::button(list, rect, icon, input, default_stylesheet(), states).clicked {
            panels.path_controls.choose(tool, out);
            if tool==PathTool::Select {panels.transform_controls.choose(super::transform_controls::Tool::Move,out);}
        }
    }
    use super::transform_controls::Tool as TransformTool;
    for (i,(tool,icon)) in [(TransformTool::Rotate,Icon::Rotate),(TransformTool::Scale,Icon::Scale),(TransformTool::Anchor,Icon::Anchor)].into_iter().enumerate() {
        let states=if panels.path_controls.tool==PathTool::Select&&panels.transform_controls.tool==tool {
            States::default().with(State::Selected)
        }else{States::default()};
        if icons::button(list,Rect::new(96.+i as f32*28.,area.min[1]+4.,24.,24.),icon,input,default_stylesheet(),states).clicked {
            panels.path_controls.choose(PathTool::Select,out);
            panels.transform_controls.choose(tool,out);
        }
    }
    for (i, (tool, action, enabled)) in [
        (
            Tool::Undo,
            Action::History(0),
            state["canUndo"].as_bool().unwrap_or(false),
        ),
        (
            Tool::Redo,
            Action::History(1),
            state["canRedo"].as_bool().unwrap_or(false),
        ),
        (
            Tool::Rectangle,
            Action::AddShape(motion_core::vector::ShapeKind::Rectangle),
            true,
        ),
        (Tool::Text, Action::NewText, true),
    ]
    .into_iter()
    .enumerate()
    {
        icon_button(
            list,
            Rect::new(180. + i as f32 * 28., area.min[1] + 3., 25., 25.),
            tool,
            input,
            enabled,
            action,
            out,
        );
    }
    paint::rect(
        list,
        Rect::new(299., area.min[1] + 6., 1., 20.),
        palette::BORDER,
    );
    let x = 308.;
    label(
        list,
        atlas,
        Rect::new(x, area.min[1], 32., 32.),
        if locale == Locale::Zh {
            "填充"
        } else {
            "Fill"
        },
        true,
    );
    paint::rect(
        list,
        Rect::new(x + 34., area.min[1] + 9., 16., 14.),
        palette::TEXT,
    );
    label(
        list,
        atlas,
        Rect::new(x + 55., area.min[1], 32., 32.),
        if locale == Locale::Zh {
            "描边"
        } else {
            "Stroke"
        },
        true,
    );
    paint::rect(
        list,
        Rect::new(x + 90., area.min[1] + 9., 16., 14.),
        palette::ACCENT,
    );
    label(
        list,
        atlas,
        Rect::new(x + 112., area.min[1], 40., 32.),
        "3 px",
        true,
    );
    let right = (size[0] - 270.).max(400.);
    for (i, (text, action)) in [
        (locale.t(T::Default), Action::ResetWorkspace),
        (
            locale.t(T::Nodes),
            Action::ShowPanel(motion_ui::dock::Panel::Nodes.dock()),
        ),
        (
            if locale == Locale::Zh {
                "图表"
            } else {
                "Graph"
            },
            Action::ShowPanel(motion_ui::dock::Panel::Curves.dock()),
        ),
        (
            if locale == Locale::Zh {
                "中文"
            } else {
                "English"
            },
            Action::Language(if locale == Locale::Zh {
                Locale::En
            } else {
                Locale::Zh
            }),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        button(
            list,
            atlas,
            Rect::new(right + i as f32 * 64., area.min[1] + 4., 60., 24.),
            text,
            input,
            true,
            action,
            out,
        );
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
        if icons::button(list, r, icon, input, default_stylesheet(), states).clicked {
            out.push(action);
        }
        return;
    }
    let hovered = enabled && r.contains(input.mouse);
    if hovered {
        paint::rect(list, r, palette::CHROME);
        paint::stroke(list, r, palette::BORDER, 1.0);
    }
    let c = if enabled {
        palette::TEXT
    } else {
        palette::TEXT_MUTED.with_alpha(0.45)
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
            list.push_shape(paint::Shape::Quad {
                points: [
                    [x + 3., y],
                    [x + 16., y + 8.],
                    [x + 3., y + 16.],
                    [x + 3., y + 16.],
                ],
            }, c);
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
