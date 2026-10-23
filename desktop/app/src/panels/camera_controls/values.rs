//! Camera field units, authored defaults and the legacy composition-camera rails.
use super::*;
use motion_ui::widgets::NumberOptions;

pub(super) fn text(locale: Locale, zh: &'static str, en: &'static str) -> &'static str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
pub(super) fn bounds(property: Property) -> NumberOptions {
    let (min, max, speed) = match property {
        Property::Fov => (10., 120., 0.5),
        Property::Radius => (1., 10_000_000., 1.),
        Property::Elevation => (-89., 89., 0.5),
        Property::Roll | Property::Azimuth => (-1_000_000., 1_000_000., 0.5),
        _ => (-10_000_000., 10_000_000., 0.5),
    };
    NumberOptions {
        min,
        max,
        speed,
        decimals: 3,
    }
}
pub(super) fn range_note(property: Property, locale: Locale) -> String {
    let options = bounds(property);
    let unit = if matches!(
        property,
        Property::Fov | Property::Elevation | Property::Roll | Property::Azimuth
    ) {
        "°"
    } else {
        "px"
    };
    let extra = if property == Property::Elevation {
        text(locale, "；避开 ±90° 极点", "; excludes the ±90° poles")
    } else {
        ""
    };
    format!(
        "{}: {}…{} {unit}{extra}",
        text(locale, "范围", "Range"),
        options.min,
        options.max,
    )
}
pub(super) fn feedback(property: Property, locale: Locale, invalid: bool) -> String {
    let range = range_note(property, locale);
    if invalid {
        format!(
            "{} · {range}",
            text(
                locale,
                "请输入范围内的有限数值",
                "Enter a finite value in range"
            )
        )
    } else {
        format!(
            "{range} · {}",
            text(
                locale,
                "右键重置当前值",
                "Right-click to reset current value"
            )
        )
    }
}
pub(super) fn sampled_value(
    sample: &Value,
    property: Property,
    key: &str,
    axis: usize,
) -> Option<f64> {
    let options = bounds(property);
    // A coupled vector edit carries its other components too. Never fabricate
    // zeros when any component is missing or outside the authored domain.
    if !property.is_scalar()
        && !(0..3).all(|i| {
            sample[key][i]
                .as_f64()
                .is_some_and(|v| v.is_finite() && (options.min..=options.max).contains(&v))
        })
    {
        return None;
    }
    let value = if property.is_scalar() {
        sample[key].as_f64()?
    } else {
        sample[key][axis].as_f64()?
    };
    (value.is_finite() && (options.min..=options.max).contains(&value)).then_some(value)
}
pub(super) fn default_value(state: &Value, property: Property, axis: usize) -> Option<f32> {
    let width = u32::try_from(state["project"]["width"].as_u64()?).ok()?;
    let height = u32::try_from(state["project"]["height"].as_u64()?).ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    let camera = motion_core::Camera::new(width, height);
    Some(match property {
        Property::Position => camera.position.value[axis],
        Property::Target => camera.target.value[axis],
        Property::Roll => camera.roll.value,
        Property::Fov => camera.fov.value,
        Property::Radius => camera.radius.value,
        Property::Azimuth => camera.azimuth.value,
        Property::Elevation => camera.elevation.value,
        _ => return None,
    })
}
pub(super) fn sample_problem(sample: &Value, property: Property, key: &str) -> &'static str {
    let options = bounds(property);
    for axis in 0..if property.is_scalar() { 1 } else { 3 } {
        let value = if property.is_scalar() {
            &sample[key]
        } else {
            &sample[key][axis]
        };
        if value.is_null() {
            return "missing_sample_component";
        }
        let Some(value) = value.as_f64() else {
            return "nonnumeric_sample_component";
        };
        if !value.is_finite() {
            return "nonfinite_sample_component";
        }
        if !(options.min..=options.max).contains(&value) {
            return "sample_outside_authored_range";
        }
    }
    "invalid_sample"
}
pub(super) fn root_rows(
    camera: &Value,
    properties: Vec<(Property, &'static str, T, usize)>,
) -> Vec<super::super::timeline::PropertyRow> {
    let mut rows = vec![];
    for (property, key, title, dimensions) in properties {
        let separated = camera[key]["axes"].is_object();
        let axes = if separated {
            (0..dimensions).map(Some).collect()
        } else {
            vec![None]
        };
        for axis in axes {
            let source = axis.map_or(&camera[key], |i| &camera[key]["axes"][["x", "y", "z"][i]]);
            let keys = source["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|key| key["frame"].as_f64())
                .collect();
            rows.push(super::super::timeline::PropertyRow {
                property,
                key,
                title,
                axis,
                axes: axis.map_or_else(|| (0..dimensions).collect(), |i| vec![i]),
                keys,
                modified: false,
            });
        }
    }
    rows
}
pub(super) fn root_choice(
    state: &Value,
    property: Property,
    axis: Option<Axis>,
    choice: super::super::key_navigation::Choice,
    out: &mut Vec<Action>,
) {
    use super::super::key_navigation::Choice;
    match choice {
        Choice::Seek(frame) => out.push(Action::Seek(frame)),
        Choice::Toggle => {
            let Some(frame) = super::super::key_navigation::editable_frame(
                state["frame"].as_f64().unwrap_or(f64::NAN),
                super::super::last_frame(state),
            ) else {
                return;
            };
            let key = match property {
                Property::Position => "position",
                Property::Target => "target",
                Property::Roll => "roll",
                Property::Fov => "fov",
                Property::Radius => "radius",
                Property::Azimuth => "azimuth",
                Property::Elevation => "elevation",
                _ => return,
            };
            let component = match axis {
                Some(Axis::Y) => 1,
                Some(Axis::Z) => 2,
                _ => 0,
            };
            let source = axis.map_or(&camera(state, 0)[key], |_| {
                &camera(state, 0)[key]["axes"][["x", "y", "z"][component]]
            });
            if source["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|key| key["frame"].as_u64() == Some(u64::from(frame)))
            {
                out.push(Action::Edit(Command::DeleteKey {
                    object: 0,
                    property,
                    axis,
                    frame,
                }));
            } else if let Some(value) = sampled_value(sample(state, 0), property, key, component) {
                out.push(Action::EditBatch(vec![
                    Command::Animate {
                        object: 0,
                        property,
                        axis,
                        frame,
                        enabled: true,
                    },
                    camera_command(state, 0, property, component, value as f32),
                ]));
            }
        }
    }
}
