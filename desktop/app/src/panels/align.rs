use super::*;
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
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    paint::rect(list, area, palette::PANEL);
    let Some(layer) = auxiliary::selected(panels, state) else {
        label(
            list,
            atlas,
            area.shrink(10.),
            panels.locale.t(T::SelectLayer),
            true,
        );
        return;
    };
    let Some(sample) = state["sampledLayers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|s| s["id"] == layer["id"])
    else {
        return;
    };
    let width = state["project"]["width"].as_f64().unwrap_or(1920.) as f32;
    let height = state["project"]["height"].as_f64().unwrap_or(1080.) as f32;
    let enabled = layer["locked"] != true && layer["three_d"] != true;
    let mut y = area.min[1] + 10.;
    auxiliary::line(
        list,
        atlas,
        area,
        &mut y,
        auxiliary::localized(
            panels.locale,
            "对齐到合成中心",
            "Align to composition center",
        ),
        true,
    );
    for (zh, en, axes) in [
        ("水平居中", "Horizontal center", vec![(0, width / 2.)]),
        ("垂直居中", "Vertical center", vec![(1, height / 2.)]),
        ("中心", "Center", vec![(0, width / 2.), (1, height / 2.)]),
    ] {
        button(
            list,
            atlas,
            Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
            auxiliary::localized(panels.locale, zh, en),
            input,
            enabled,
            Action::EditBatch(commands(layer, sample, state, &axes)),
            out,
        );
        y += 30.;
    }
}
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
                engine.project().layers[0].transform.position.sample(21.),
                [160., 90., 45.]
            );
            engine.undo().unwrap();
            assert_eq!(engine.snapshot(), before);
        }
    }
}
