use super::*;
#[cfg(test)]
pub(super) fn commands(
    layer: &Value,
    sample: &Value,
    state: &Value,
    axes: &[(usize, f32)],
) -> Vec<Command> {
    let object = layer["id"].as_u64().unwrap();
    let frame = state["frame"].as_f64().unwrap_or(0.).floor() as u32;
    let separated = state["timeline_layers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|t| {
            t["object"].as_u64() == Some(object) && t["properties"]["position"]["separated"] == true
        });
    if separated {
        axes.iter()
            .map(|(axis, value)| Command::SetComponent {
                object,
                property: Property::Position,
                axis: [
                    motion_core::Axis::X,
                    motion_core::Axis::Y,
                    motion_core::Axis::Z,
                ][*axis],
                frame,
                value: *value,
            })
            .collect()
    } else {
        let mut value =
            std::array::from_fn(|i| sample["position"][i].as_f64().unwrap_or(0.) as f32);
        for (axis, v) in axes {
            value[*axis] = *v;
        }
        vec![Command::SetVector {
            object,
            property: Property::Position,
            frame,
            value,
        }]
    }
}
#[derive(Default)]
pub(super) struct Controls {
    pub(super) target: motion_ui::widgets::Dropdown,
    value: motion_host::ops::alignment::Target,
}
fn unavailable(panels: &Panels, state: &Value) -> Option<(&'static str, &'static str)> {
    let ids = panels.selection_ids();
    if ids.is_empty() {
        return Some(("请选择图层", "Select layers"));
    }
    let Some(layers) = state["project"]["layers"].as_array() else {
        return Some(("工程状态尚未就绪", "Project state is not ready"));
    };
    for id in ids {
        let Some(layer) = layers.iter().find(|l| l["id"].as_u64() == Some(id)) else {
            return Some(("选择的图层已不存在", "A selected layer is missing"));
        };
        if layer["locked"] == true {
            return Some(("所选包含锁定图层，请先解锁", "Unlock selected layers first"));
        }
        if layer["three_d"] == true {
            return Some(("目前仅支持二维图层对齐", "Alignment supports 2D layers"));
        }
        if layer["visible"] == false
            || ["audio", "camera", "null", "adjustment"]
                .iter()
                .any(|kind| layer["content"]["kind"] == *kind)
        {
            return Some((
                "请选择可见的二维画面图层",
                "Select visible 2D drawable layers",
            ));
        }
        if state["project"]["expressions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|e| {
                e["enabled"] == true
                    && e["target"]["object"].as_u64() == Some(id)
                    && e["target"]["property"] == "position"
            })
        {
            return Some(("请先停用位置表达式", "Disable position expressions first"));
        }
        if !state["projectedLayers"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|l| l["id"].as_u64() == Some(id))
        {
            return Some((
                "所选图层在当前帧没有画面边界",
                "No drawable bounds at this frame",
            ));
        }
    }
    if panels.auxiliary.align.value == motion_host::ops::alignment::Target::Selection
        && panels.selection_ids().len() < 2
    {
        return Some((
            "对齐到选区需要至少两个图层",
            "Selection target needs two layers",
        ));
    }
    None
}
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    use motion_host::ops::alignment::{Distribution, Edge, Target};
    use motion_ui::widgets::DropdownItem;
    let styles = list.styles();
    paint::rect(list, area, styles.color("panel"));
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("auxiliary.gap", 6.);
    let row = styles.metric("row.height", 22.);
    let control = styles.metric("auxiliary.control.height", 26.);
    let owned = panels.auxiliary.align.target.open;
    let neutral = Input::default();
    let controls = if owned { &neutral } else { input };
    let mut y = auxiliary::body_y(
        &styles,
        Panel::Align,
        area,
        row * 8. + control * 4. + gap * 9. + pad * 2.,
        panels,
        controls,
    );
    let x = area.min[0] + pad;
    let width = (area.width() - pad * 2.).max(0.);
    let label_width = width * 0.3;
    label(
        list,
        atlas,
        Rect::new(x, y, label_width, row),
        auxiliary::localized(panels.locale, "对齐到", "Align to"),
        false,
    );
    let choices = [
        DropdownItem {
            label: auxiliary::localized(panels.locale, "合成", "Composition"),
            enabled: true,
        },
        DropdownItem {
            label: auxiliary::localized(panels.locale, "选区边界", "Selection bounds"),
            enabled: true,
        },
    ];
    let mut overlay = list.child();
    let selected = usize::from(panels.auxiliary.align.value == Target::Selection);
    if let Some(index) = panels.auxiliary.align.target.draw(
        &mut overlay,
        atlas,
        Rect::new(
            x + label_width + gap,
            y,
            (width - label_width - gap).max(0.),
            row,
        ),
        area,
        &choices,
        selected,
        input,
        true,
        false,
    ) {
        panels.auxiliary.align.value = if index == 0 {
            Target::Composition
        } else {
            Target::Selection
        };
    }
    y += row + gap;
    let reason = unavailable(panels, state);
    for (title, entries) in [
        (
            auxiliary::localized(panels.locale, "水平边界", "Horizontal"),
            [
                ("左对齐", "Left", Edge::Left),
                ("水平居中", "Center", Edge::HorizontalCenter),
                ("右对齐", "Right", Edge::Right),
            ],
        ),
        (
            auxiliary::localized(panels.locale, "垂直边界", "Vertical"),
            [
                ("顶对齐", "Top", Edge::Top),
                ("垂直居中", "Center", Edge::VerticalCenter),
                ("底对齐", "Bottom", Edge::Bottom),
            ],
        ),
    ] {
        label(list, atlas, Rect::new(x, y, width, row), title, true);
        y += row + gap;
        let button_width = ((width - gap * 2.) / 3.).max(0.);
        for (index, (zh, en, edge)) in entries.into_iter().enumerate() {
            if ui::button(
                list,
                atlas,
                Rect::new(
                    x + index as f32 * (button_width + gap),
                    y,
                    button_width,
                    control,
                ),
                auxiliary::localized(panels.locale, zh, en),
                controls,
                reason.is_none(),
            )
            .clicked
            {
                out.push(Action::AlignLayers {
                    objects: panels.selection_ids(),
                    edge,
                    target: panels.auxiliary.align.value,
                });
            }
        }
        y += control + gap;
    }
    for text in if let Some((zh, en)) = reason {
        [
            auxiliary::localized(panels.locale, zh, en),
            auxiliary::localized(
                panels.locale,
                "整组不移动；请调整选择后重试。",
                "No layers move; adjust the selection.",
            ),
        ]
    } else if panels.auxiliary.align.value == Target::Selection {
        [
            auxiliary::localized(
                panels.locale,
                "以全部所选图层的边界为基准。",
                "Uses the bounds of all selected layers.",
            ),
            auxiliary::localized(
                panels.locale,
                "主选不固定；整组操作可一步撤销。",
                "Primary is movable; undo restores the batch.",
            ),
        ]
    } else {
        [
            auxiliary::localized(
                panels.locale,
                "按当前帧画面边界对齐到合成。",
                "Aligns current-frame bounds to composition.",
            ),
            auxiliary::localized(
                panels.locale,
                "保留其他变换；整组可一步撤销。",
                "Preserves other transforms; one-step undo.",
            ),
        ]
    } {
        label(list, atlas, Rect::new(x, y, width, row), text, true);
        y += row;
    }
    y += gap;
    label(
        list,
        atlas,
        Rect::new(x, y, width, row),
        auxiliary::localized(
            panels.locale,
            "分布（首末固定）",
            "Distribute (fixed endpoints)",
        ),
        true,
    );
    y += row + gap;
    let button_width = ((width - gap) / 2.).max(0.);
    let enabled = reason.is_none() && panels.selection_ids().len() >= 3;
    for entries in [
        [
            (
                "水平位置",
                "Horizontal centers",
                Distribution::HorizontalCenters,
            ),
            ("水平间距", "Horizontal gaps", Distribution::HorizontalGaps),
        ],
        [
            (
                "垂直位置",
                "Vertical centers",
                Distribution::VerticalCenters,
            ),
            ("垂直间距", "Vertical gaps", Distribution::VerticalGaps),
        ],
    ] {
        for (index, (zh, en, distribution)) in entries.into_iter().enumerate() {
            if ui::button(
                list,
                atlas,
                Rect::new(
                    x + index as f32 * (button_width + gap),
                    y,
                    button_width,
                    control,
                ),
                auxiliary::localized(panels.locale, zh, en),
                controls,
                enabled,
            )
            .clicked
            {
                out.push(Action::DistributeLayers {
                    objects: panels.selection_ids(),
                    distribution,
                });
            }
        }
        y += control + gap;
    }
    for (zh, en) in [
        (
            "位置按中心、间距按边缘排序。",
            "Centers / gaps sort by center / edge.",
        ),
        (
            "至少三个图层；负间距表示重叠。",
            "Needs 3+ layers; negative gaps overlap.",
        ),
    ] {
        label(
            list,
            atlas,
            Rect::new(x, y, width, row),
            auxiliary::localized(panels.locale, zh, en),
            true,
        );
        y += row;
    }
    list.append(overlay);
}
#[cfg(test)]
#[path = "align_interaction_tests.rs"]
mod interaction_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn centered_components_do_not_overwrite_each_other_or_touch_z() {
        let mut project = motion_core::Project::new(320, 180, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            7,
            "Shape",
            [30., 30.],
            [10., 20., 45.],
            [1.; 4],
        ));
        let mut engine = motion_core::Engine::new(project).unwrap();
        for separated in [false, true] {
            if separated {
                engine
                    .apply(Command::SeparateDimensions {
                        object: 7,
                        property: Property::Position,
                    })
                    .unwrap();
            }
            let before = engine.snapshot();
            let state = serde_json::json!({"frame":21,"timeline_layers":[{"object":7,"properties":{"position":{"separated":separated}}}]});
            let commands = commands(
                &serde_json::json!({"id":7}),
                &serde_json::json!({"position":[10.,20.,45.]}),
                &state,
                &[(0, 160.), (1, 90.)],
            );
            engine.apply_batch(commands).unwrap();
            assert_eq!(
                engine.project().layers[0].transform.position.sample(21.).unwrap(),
                [160., 90., 45.]
            );
            engine.undo().unwrap();
            assert_eq!(engine.snapshot(), before);
        }
    }
}
