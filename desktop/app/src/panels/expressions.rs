use super::*;
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.PANEL);
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
    let object = layer["id"].as_u64().unwrap();
    let mut y = area.min[1] + 10.;
    for (i, text) in [T::Position, T::Rotation, T::Scale, T::Opacity]
        .into_iter()
        .enumerate()
    {
        let row = Rect::new(area.min[0] + 10., y, area.width() - 20., 24.);
        if ui::button(list, atlas, row, panels.locale.t(text), input, true).clicked {
            panels.auxiliary.expression_property = i;
            panels.auxiliary.expression_object = None;
        }
        y += 28.;
    }
    let property = [
        Property::Position,
        Property::Rotation,
        Property::Scale,
        Property::Opacity,
    ][panels.auxiliary.expression_property];
    let target = motion_core::ExpressionTarget::Property {
        object,
        property,
        axis: None,
    };
    if panels.auxiliary.expression_object != Some(object) {
        panels.auxiliary.expression = state["project"]["expressions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| {
                serde_json::from_value::<motion_core::ExpressionTarget>(e["target"].clone())
                    .ok()
                    .as_ref()
                    == Some(&target)
            })
            .and_then(|e| e["source"].as_str())
            .unwrap_or("value")
            .to_owned();
        panels.auxiliary.expression_object = Some(object);
    }
    if panels.focused == Some(Panel::Expression) {
        panels.auxiliary.expression.push_str(&input.text);
        if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
            panels.auxiliary.expression.pop();
        }
    }
    let field = Rect::new(area.min[0] + 10., y, area.width() - 20., 72.);
    paint::rect(list, field, colors.SUNKEN);
    label(
        list,
        atlas,
        field.shrink(6.),
        &panels.auxiliary.expression,
        false,
    );
    y += 80.;
    let enabled = layer["locked"] != true;
    button(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
        auxiliary::localized(panels.locale, "应用", "Apply"),
        input,
        enabled,
        Action::Edit(Command::SetExpression {
            expression: motion_core::PropertyExpression {
                target: target.clone(),
                source: panels.auxiliary.expression.clone(),
                enabled: true,
                seed: 0,
                profile: motion_core::EXPRESSION_PROFILE.into(),
            },
            frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
        }),
        out,
    );
    y += 30.;
    button(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
        auxiliary::localized(panels.locale, "移除表达式", "Remove expression"),
        input,
        enabled,
        Action::Edit(Command::RemoveExpression { target }),
        out,
    );
}
