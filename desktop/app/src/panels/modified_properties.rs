//! Authored-property classification for the shared inspector/timeline filter.
//! Defaults describe a newly created layer in the current composition, rather
//! than the sampled value at the current playhead or the last saved document.
use motion_core::{Axis, Camera, Property};
use serde_json::Value;
use std::time::{Duration, Instant};

/// A first press is delivered immediately. Only a fresh second press in the
/// same interaction context becomes UU; callers discard repeat/IME events.
#[derive(Default)]
pub(crate) struct RevealSequence {
    last: Option<(u64, Instant)>,
}
impl RevealSequence {
    pub(super) fn interrupt(&mut self) {
        self.last = None;
    }
    pub(super) fn press(&mut self, context: u64, now: Instant) -> bool {
        let modified = self.last.take().is_some_and(|(previous, time)| {
            previous == context
                && now
                    .checked_duration_since(time)
                    .is_some_and(|elapsed| elapsed <= Duration::from_millis(350))
        });
        if !modified {
            self.last = Some((context, now));
        }
        modified
    }
}

pub(super) fn mask(mask: &motion_core::masks::LayerMask, channel: u8) -> bool {
    match channel {
        0 => mask.opacity.is_animated() || mask.opacity.value != 100.,
        1 => mask.feather.is_animated() || mask.feather.value != [0.; 2],
        2 => mask.expansion.is_animated() || mask.expansion.value != 0.,
        // A mask path is explicitly authored content, with no empty legal
        // geometry to compare against. Its existence is the modification.
        _ => !mask.path.nodes.is_empty(),
    }
}
pub(super) fn mask_visible(
    filter: super::types::PropertyFilter,
    mask_value: &motion_core::masks::LayerMask,
    channel: u8,
) -> bool {
    use super::types::PropertyFilter;
    match filter {
        PropertyFilter::All => true,
        PropertyFilter::Properties(_) => false,
        PropertyFilter::Modified => mask(mask_value, channel),
        PropertyFilter::Animated => match channel {
            0 => mask_value.opacity.is_animated(),
            1 => mask_value.feather.is_animated(),
            2 => mask_value.expansion.is_animated(),
            _ => mask_value
                .path
                .nodes
                .iter()
                .any(|node| node.geometry.is_animated()),
        },
    }
}

pub(super) fn effect_parameter_visible(
    filter: super::types::PropertyFilter,
    parameter: &Value,
    state: &Value,
    object: u64,
    effect: u64,
    name: &str,
) -> bool {
    let expressed = filter == super::types::PropertyFilter::Modified
        && state["project"]["expressions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|expression| {
                let target = &expression["target"];
                target["kind"] == "effect"
                    && target["object"] == object
                    && target["effect"] == effect
                    && target["param"] == name
            });
    effect_value_visible(filter, parameter, object, effect, name, expressed)
}

pub(super) fn effect_value_visible(
    filter: super::types::PropertyFilter,
    parameter: &Value,
    object: u64,
    effect: u64,
    name: &str,
    expressed: bool,
) -> bool {
    use super::types::PropertyFilter;
    let animated = [&parameter["track"], &parameter["curve"]]
        .into_iter()
        .any(|track| {
            track["keys"]
                .as_array()
                .is_some_and(|keys| !keys.is_empty())
        });
    match filter {
        PropertyFilter::All => true,
        PropertyFilter::Properties(_) => false,
        PropertyFilter::Animated => animated,
        PropertyFilter::Modified => {
            if animated || expressed {
                return true;
            }
            if parameter["kind"] == "curve" {
                let value = observe!(
                    "properties.modified_curve",
                    serde_json::from_value::<motion_core::CurveObject>(
                        parameter["curve"]["value"].clone()
                    ),
                    "object={object} effect={effect} parameter={name}"
                );
                return value.map_or(true, |value| value != motion_core::CurveObject::default());
            }
            let values = parameter["track"]["value"]
                .as_array()
                .zip(parameter["default"].as_array());
            let values = observe!(
                "properties.modified_effect_value",
                values
                    .filter(|(actual, default)| actual.len() == 4
                        && default.len() == 4
                        && actual
                            .iter()
                            .chain(default.iter())
                            .all(|value| value.as_f64().is_some_and(f64::is_finite)))
                    .ok_or("effect authored/default numeric values unavailable"),
                "object={object} effect={effect} parameter={name}"
            );
            values.map_or(true, |(actual, default)| {
                actual
                    .iter()
                    .zip(default)
                    .any(|(a, b)| a.as_f64() != b.as_f64())
            })
        }
    }
}

#[cfg(test)]
pub(super) fn transform(
    state: &Value,
    object: u64,
    property: Property,
    axis: Option<Axis>,
    visible_axes: &[usize],
) -> bool {
    let Some(source) = super::animation_data::channel(state, object, property, axis) else {
        return false; // This object has no such authored channel.
    };
    let layer = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|layer| layer["id"] == object);
    transform_source(state, layer, source, property, axis, visible_axes)
}

pub(super) fn transform_source(
    state: &Value,
    layer: Option<&Value>,
    source: &Value,
    property: Property,
    axis: Option<Axis>,
    visible_axes: &[usize],
) -> bool {
    let object = layer.and_then(|layer| layer["id"].as_u64()).unwrap_or(0);
    if source.is_null() {
        return false;
    }
    if source["keys"]
        .as_array()
        .is_some_and(|keys| !keys.is_empty())
    {
        return true;
    }
    let key = super::animation_data::name(property);
    if state["project"]["expressions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|expression| {
            let target = &expression["target"];
            target["kind"] == "property"
                && target["object"] == object
                && target["property"] == key
                && target["axis"].as_str() == axis.map(|axis| ["x", "y", "z"][axis.index()])
        })
    {
        // A disabled expression is still authored and must remain discoverable.
        return true;
    }
    let width = state["project"]["width"].as_u64().unwrap_or(0) as u32;
    let height = state["project"]["height"].as_u64().unwrap_or(0) as u32;
    let camera_layer = layer.is_some_and(|layer| layer["content"]["kind"] == "camera");
    let camera = Camera::new(width, height);
    let defaults = match property {
        Property::Anchor => [0.5, 0.5, 0.0],
        Property::Position if camera_layer => camera.position.value,
        Property::Position => [width as f32 / 2.0, height as f32 / 2.0, 0.0],
        Property::Scale => [100.0; 3],
        Property::Rotation => [0.0; 3],
        Property::Opacity => [1.0; 3],
        Property::Target => camera.target.value,
        Property::Roll => [camera.roll.value; 3],
        Property::Fov => [camera.fov.value; 3],
        Property::Radius => [camera.radius.value; 3],
        Property::Azimuth => [camera.azimuth.value; 3],
        Property::Elevation => [camera.elevation.value; 3],
    };
    visible_axes.iter().any(|component| {
        let value = if axis.is_some() || !source["value"].is_array() {
            &source["value"]
        } else {
            &source["value"][*component]
        };
        // Retain an undecodable authored field and publish the existing bounded
        // degradation signal; hiding it would conceal a broken state response.
        let value = observe!(
            "properties.modified_value",
            value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or("missing or nonfinite authored number"),
            "object={object} property={property:?} axis={axis:?} component={component}"
        );
        value.map_or(true, |value| value != f64::from(defaults[*component]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_core::{Command, Engine, Layer, Project};

    #[test]
    fn route_u_is_immediate_and_uu_reveals_static_edits_without_document_edits() {
        use super::super::{keyboard, timeline, types::PropertyFilter, Panels};
        use motion_ui::{dock::Panel, Input, Key};
        let mut engine = fixture();
        engine
            .apply(Command::SetVector {
                object: 7,
                property: Property::Rotation,
                frame: 0,
                value: [0., 0., 12.],
            })
            .unwrap();
        engine
            .apply(Command::Animate {
                object: 7,
                property: Property::Opacity,
                axis: None,
                frame: 0,
                enabled: true,
            })
            .unwrap();
        let before = engine.snapshot();
        let snapshot = state(&engine, 30.);
        for locale in [crate::Locale::Zh, crate::Locale::En] {
            for owner in [None, Some(Panel::Timeline), Some(Panel::Properties)] {
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(owner.unwrap_or(Panel::Timeline)),
                    locale,
                    ..Default::default()
                };
                let input = Input {
                    keys: vec![(Key::U, Default::default())],
                    ..Default::default()
                };
                let mut actions = vec![];
                keyboard::route(&input, &mut panels, &snapshot, false, owner, &mut actions);
                assert_eq!(panels.timeline.filters[&7], PropertyFilter::Animated);
                let rows = timeline::filtered_properties(&snapshot, 7, panels.timeline.filters[&7]);
                assert_eq!(
                    rows.iter().map(|row| row.property).collect::<Vec<_>>(),
                    [Property::Opacity]
                );
                keyboard::route(&input, &mut panels, &snapshot, false, owner, &mut actions);
                assert_eq!(panels.timeline.filters[&7], PropertyFilter::Modified);
                let rows = timeline::filtered_properties(&snapshot, 7, panels.timeline.filters[&7]);
                assert_eq!(
                    rows.iter().map(|row| row.property).collect::<Vec<_>>(),
                    [Property::Rotation, Property::Opacity]
                );
                assert!(
                    panels.timeline.expanded.contains(&7) && !panels.properties.transform_collapsed
                );
                assert!(actions.is_empty());
                assert_eq!(engine.project(), &before);
            }
        }
    }

    #[test]
    fn interrupted_or_repeated_u_does_not_become_uu() {
        use super::super::{keyboard, types::PropertyFilter, Panels};
        use motion_ui::{dock::Panel, Input, Key};
        let snapshot = state(&fixture(), 0.);
        let press = Input {
            keys: vec![(Key::U, Default::default())],
            ..Default::default()
        };
        for event in [
            Input {
                focus_lost: true,
                ..Default::default()
            },
            Input {
                composing: true,
                ..Default::default()
            },
            Input {
                ime_consumed: true,
                ..Default::default()
            },
            Input {
                wheel: [0., 1.],
                ..Default::default()
            },
            Input {
                keys: press.keys.clone(),
                key_repeats: vec![true],
                ..Default::default()
            },
        ] {
            for owner in [None, Some(Panel::Timeline), Some(Panel::Properties)] {
                let mut panels = Panels {
                    selected: Some(7),
                    focused: Some(owner.unwrap_or(Panel::Timeline)),
                    ..Default::default()
                };
                let mut actions = vec![];
                keyboard::route(&press, &mut panels, &snapshot, false, owner, &mut actions);
                assert_eq!(panels.timeline.filters[&7], PropertyFilter::Animated);
                keyboard::route(&event, &mut panels, &snapshot, false, owner, &mut actions);
                assert_eq!(panels.timeline.filters[&7], PropertyFilter::Animated);
                keyboard::route(&press, &mut panels, &snapshot, false, owner, &mut actions);
                assert_ne!(panels.timeline.filters[&7], PropertyFilter::Modified);
                assert!(actions.is_empty());
            }
        }
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.select_only(Some(7));
        let mut actions = vec![];
        keyboard::route(&press, &mut panels, &snapshot, false, None, &mut actions);
        panels.select_only(Some(8));
        panels.select_only(Some(7));
        keyboard::route(&press, &mut panels, &snapshot, false, None, &mut actions);
        assert_ne!(panels.timeline.filters[&7], PropertyFilter::Modified);
        for text_editor in [false, true] {
            let mut panels = Panels {
                selected: Some(7),
                focused: Some(Panel::Timeline),
                ..Default::default()
            };
            keyboard::route(&press, &mut panels, &snapshot, false, None, &mut actions);
            if text_editor {
                panels.timeline.timeline_search = true;
            } else {
                panels.menu = Some(0);
            }
            keyboard::route(
                &Input::default(),
                &mut panels,
                &snapshot,
                false,
                None,
                &mut actions,
            );
            panels.timeline.timeline_search = false;
            panels.menu = None;
            keyboard::route(&press, &mut panels, &snapshot, false, None, &mut actions);
            assert_ne!(panels.timeline.filters[&7], PropertyFilter::Modified);
        }
    }

    #[test]
    fn effect_defaults_static_values_keys_expressions_and_curve_reset_are_distinct() {
        use super::super::types::PropertyFilter as F;
        let mut parameter = serde_json::json!({"kind":"float","default":[1.,0.,0.,0.],"track":{"value":[1.,0.,0.,0.],"keys":[]}});
        assert!(!effect_value_visible(
            F::Modified,
            &parameter,
            7,
            1,
            "amount",
            false
        ));
        parameter["track"]["value"][0] = 2.into();
        assert!(effect_value_visible(
            F::Modified,
            &parameter,
            7,
            1,
            "amount",
            false
        ));
        assert!(!effect_value_visible(
            F::Animated,
            &parameter,
            7,
            1,
            "amount",
            false
        ));
        parameter["track"]["value"][0] = 1.into();
        parameter["track"]["keys"] = serde_json::json!([{"frame":0,"value":[1.,0.,0.,0.]}]);
        assert!(effect_value_visible(
            F::Modified,
            &parameter,
            7,
            1,
            "amount",
            false
        ));
        parameter["track"]["keys"] = serde_json::json!([]);
        assert!(effect_value_visible(
            F::Modified,
            &parameter,
            7,
            1,
            "amount",
            true
        ));
        let mut curve = serde_json::json!({"kind":"curve","curve":{"value":motion_core::CurveObject::default(),"keys":[]}});
        assert!(!effect_value_visible(
            F::Modified,
            &curve,
            7,
            1,
            "curve",
            false
        ));
        curve["curve"]["value"]["channels"][0] =
            serde_json::json!([[0., 0.], [0.5, 0.8], [1., 1.]]);
        assert!(effect_value_visible(
            F::Modified,
            &curve,
            7,
            1,
            "curve",
            false
        ));
        curve["curve"]["value"] =
            serde_json::to_value(motion_core::CurveObject::default()).unwrap();
        assert!(!effect_value_visible(
            F::Modified,
            &curve,
            7,
            1,
            "curve",
            false
        ));
    }

    #[test]
    fn masks_keep_authored_geometry_and_independent_modified_tracks() {
        use super::super::types::PropertyFilter as F;
        use motion_core::{
            masks::LayerMask,
            vector::{PathNode, VectorPath},
            Ease, Track,
        };
        let mut value = LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: true,
                nodes: vec![PathNode {
                    id: 1,
                    geometry: Track::constant([0.; 6]),
                }],
            },
        );
        assert!(mask_visible(F::Modified, &value, 3));
        assert!(!mask_visible(F::Modified, &value, 0));
        value.feather.value = [2., 0.];
        assert!(mask_visible(F::Modified, &value, 1));
        assert!(!mask_visible(F::Animated, &value, 1));
        value.opacity.upsert(0, 100., Ease::Linear).unwrap();
        assert!(mask_visible(F::Modified, &value, 0));
        value.feather.value = [0.; 2];
        assert!(!mask_visible(F::Modified, &value, 1));
    }

    #[test]
    fn malformed_effect_values_remain_visible_and_use_bounded_default_error_sinks() {
        use super::super::types::PropertyFilter as F;
        const CHILD: &str = "MOTION_MODIFIED_EFFECT_FAILURE_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let bad = serde_json::json!({"kind":"float","default":[0,0,0,0],"track":{"value":["bad",0,0,0],"keys":[]}});
            let good = serde_json::json!({"kind":"float","default":[0,0,0,0],"track":{"value":[0,0,0,0],"keys":[]}});
            let bad_curve =
                serde_json::json!({"kind":"curve","curve":{"value":{"channels":[]},"keys":[]}});
            let good_curve = serde_json::json!({"kind":"curve","curve":{"value":motion_core::CurveObject::default(),"keys":[]}});
            let file = root.join("motion-studio.log");
            for count in 1..=180 {
                assert!(effect_value_visible(
                    F::Modified,
                    &bad,
                    7,
                    3,
                    "amount",
                    false
                ));
                assert!(effect_value_visible(
                    F::Modified,
                    &bad_curve,
                    7,
                    3,
                    "curve",
                    false
                ));
                assert!(!effect_value_visible(
                    F::Modified,
                    &good,
                    7,
                    3,
                    "amount",
                    false
                ));
                assert!(!effect_value_visible(
                    F::Modified,
                    &good_curve,
                    7,
                    3,
                    "curve",
                    false
                ));
                if count == 59 {
                    assert!(!std::fs::read_to_string(&file)
                        .unwrap()
                        .contains("persistent panel degradation"));
                }
            }
            let text = std::fs::read_to_string(file).unwrap();
            for site in [
                "properties.modified_effect_value",
                "properties.modified_curve",
            ] {
                assert_eq!(text.matches(&format!("site={site} ")).count(), 2);
            }
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let result=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::modified_properties::tests::malformed_effect_values_remain_visible_and_use_bounded_default_error_sinks"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
        let stdout = String::from_utf8(result.stdout).unwrap();
        let stderr = String::from_utf8(result.stderr).unwrap();
        assert!(result.status.success(), "{stdout}\n{stderr}");
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        for site in [
            "properties.modified_effect_value",
            "properties.modified_curve",
        ] {
            assert_eq!(stderr.matches(&format!("site={site} ")).count(), 1);
            assert!(!stdout.contains(site));
            let error = file
                .lines()
                .find(|line| line.contains("ERROR") && line.contains(&format!("site={site} ")))
                .unwrap();
            assert!(
                error.contains("count=60 ")
                    && error.contains("object=7 effect=3 parameter=")
                    && error.contains("reason=")
            );
        }
        assert!(!stderr.contains("count=1 "));
    }

    #[test]
    fn repeated_press_requires_a_fresh_pair_in_the_same_context_without_delaying_u() {
        let now = Instant::now();
        let mut sequence = RevealSequence::default();
        assert!(!sequence.press(7, now));
        assert!(sequence.press(7, now + Duration::from_millis(150)));
        assert!(!sequence.press(7, now + Duration::from_millis(200)));
        assert!(!sequence.press(7, now + Duration::from_secs(1)));
        assert!(!sequence.press(8, now + Duration::from_millis(1100)));
        sequence.interrupt();
        assert!(!sequence.press(8, now + Duration::from_millis(1200)));
        assert!(sequence.press(8, now + Duration::from_millis(1300)));
    }

    #[test]
    fn malformed_authored_value_remains_visible_and_reaches_default_error_sinks() {
        const CHILD: &str = "MOTION_MODIFIED_PROPERTY_FAILURE_CHILD";
        if let Some(root) = std::env::var_os(CHILD) {
            let root = std::path::PathBuf::from(root);
            let _logger = crate::logging::init(root.clone()).unwrap();
            let healthy = state(&fixture(), 0.);
            let mut malformed = healthy.clone();
            let timeline = malformed["timeline_layers"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|layer| layer["object"] == 7)
                .unwrap();
            timeline["properties"]["opacity"]["value"] = serde_json::json!("invalid");
            let file = root.join("motion-studio.log");
            for _ in 0..59 {
                assert!(transform(&malformed, 7, Property::Opacity, None, &[0]));
                assert!(!transform(&healthy, 7, Property::Opacity, None, &[0]));
            }
            assert!(!std::fs::read_to_string(&file)
                .unwrap()
                .contains("persistent panel degradation"));
            assert!(transform(&malformed, 7, Property::Opacity, None, &[0]));
            let text = std::fs::read_to_string(&file).unwrap();
            assert_eq!(text.matches("site=properties.modified_value ").count(), 2);
            assert!(text.contains("count=1 ") && text.contains("count=60 "));
            // This shared observation API counts failures over the process
            // lifetime. Interleaved good rows never reset a bad row's budget.
            for _ in 0..120 {
                assert!(!transform(&healthy, 7, Property::Opacity, None, &[0]));
                assert!(transform(&malformed, 7, Property::Opacity, None, &[0]));
            }
            assert_eq!(std::fs::read_to_string(file).unwrap(), text);
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "panels::modified_properties::tests::malformed_authored_value_remains_visible_and_reaches_default_error_sinks"])
            .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stdout}\n{stderr}");
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        let error = file
            .lines()
            .find(|line| {
                line.contains("persistent panel degradation site=properties.modified_value ")
            })
            .unwrap();
        assert!(
            error.contains("ERROR")
                && error.contains("object=7 property=Opacity")
                && error.contains("reason=missing or nonfinite authored number")
        );
        assert_eq!(stderr.matches("site=properties.modified_value ").count(), 1);
        assert!(stderr.contains("count=60 ") && !stderr.contains("count=1 "));
        assert!(
            !stdout.contains("properties.modified_value")
                && !stdout.contains("logging initialized")
        );
    }

    fn fixture() -> Engine {
        let mut project = Project::new(640, 480, 30, 120).unwrap();
        project.version = 19; // Directly authored camera layers require format 19.
        project.layers.push(Layer::solid(
            7,
            "Defaults",
            [100.; 2],
            [320., 240., 0.],
            [1.; 4],
        ));
        project.layers.push(Layer::camera(8, "Camera", 640, 480));
        Engine::new(project).unwrap()
    }
    fn state(engine: &Engine, frame: f64) -> Value {
        serde_json::json!({"project":engine.project(), "timeline_layers":engine.project().timeline_layers(frame)})
    }
    #[test]
    fn defaults_static_edits_and_reset_use_authored_values() {
        let mut engine = fixture();
        for property in [
            Property::Anchor,
            Property::Position,
            Property::Scale,
            Property::Rotation,
            Property::Opacity,
        ] {
            let axes = if property == Property::Opacity {
                vec![0]
            } else if property == Property::Anchor {
                vec![0, 1]
            } else {
                vec![0, 1, 2]
            };
            assert!(
                !transform(&state(&engine, 0.), 7, property, None, &axes),
                "{property:?}"
            );
        }
        engine
            .apply(Command::SetVector {
                object: 7,
                property: Property::Position,
                frame: 0,
                value: [321., 240., 0.],
            })
            .unwrap();
        assert!(transform(
            &state(&engine, 30.),
            7,
            Property::Position,
            None,
            &[0, 1]
        ));
        engine
            .apply(Command::SetVector {
                object: 7,
                property: Property::Position,
                frame: 0,
                value: [320., 240., 0.],
            })
            .unwrap();
        assert!(!transform(
            &state(&engine, 30.),
            7,
            Property::Position,
            None,
            &[0, 1]
        ));
        for property in [
            Property::Position,
            Property::Target,
            Property::Roll,
            Property::Fov,
            Property::Radius,
            Property::Azimuth,
            Property::Elevation,
        ] {
            let axes = if matches!(property, Property::Position | Property::Target) {
                vec![0, 1, 2]
            } else {
                vec![0]
            };
            // Orbit-only tracks are absent in a position-mode camera.
            if super::super::animation_data::channel(&state(&engine, 0.), 8, property, None)
                .is_some()
            {
                assert!(
                    !transform(&state(&engine, 0.), 8, property, None, &axes),
                    "camera {property:?}"
                );
            }
        }
    }
    #[test]
    fn key_at_the_default_value_stays_modified_on_every_frame() {
        let mut engine = fixture();
        engine
            .apply(Command::Animate {
                object: 7,
                property: Property::Scale,
                axis: None,
                frame: 0,
                enabled: true,
            })
            .unwrap();
        for frame in [0., 30., 119.] {
            assert!(transform(
                &state(&engine, frame),
                7,
                Property::Scale,
                None,
                &[0, 1]
            ));
        }
        engine
            .apply(Command::Animate {
                object: 7,
                property: Property::Scale,
                axis: None,
                frame: 0,
                enabled: false,
            })
            .unwrap();
        assert!(!transform(
            &state(&engine, 0.),
            7,
            Property::Scale,
            None,
            &[0, 1]
        ));
    }

    #[test]
    fn separated_axes_do_not_inherit_a_siblings_static_edit_or_keys() {
        let mut engine = fixture();
        engine
            .apply(Command::SeparateDimensions {
                object: 7,
                property: Property::Position,
            })
            .unwrap();
        engine
            .apply(Command::SetComponent {
                object: 7,
                property: Property::Position,
                axis: Axis::X,
                frame: 0,
                value: 321.,
            })
            .unwrap();
        engine
            .apply(Command::Animate {
                object: 7,
                property: Property::Position,
                axis: Some(Axis::Z),
                frame: 0,
                enabled: true,
            })
            .unwrap();
        let snapshot = state(&engine, 50.);
        assert!(transform(
            &snapshot,
            7,
            Property::Position,
            Some(Axis::X),
            &[0]
        ));
        assert!(!transform(
            &snapshot,
            7,
            Property::Position,
            Some(Axis::Y),
            &[1]
        ));
        assert!(transform(
            &snapshot,
            7,
            Property::Position,
            Some(Axis::Z),
            &[2]
        ));
        engine
            .apply(Command::SetComponent {
                object: 7,
                property: Property::Position,
                axis: Axis::X,
                frame: 0,
                value: 320.,
            })
            .unwrap();
        assert!(!transform(
            &state(&engine, 50.),
            7,
            Property::Position,
            Some(Axis::X),
            &[0]
        ));
    }

    #[test]
    fn disabled_expression_at_a_default_value_remains_authored_after_reopen() {
        let mut engine = fixture();
        let target = motion_core::ExpressionTarget::Property {
            object: 7,
            property: Property::Position,
            axis: None,
        };
        engine
            .apply(Command::SetExpression {
                expression: motion_core::PropertyExpression {
                    target: target.clone(),
                    source: "[320,240,0]".into(),
                    enabled: false,
                    seed: 0,
                    profile: motion_core::EXPRESSION_PROFILE.into(),
                },
                frame: 0,
            })
            .unwrap();
        assert!(transform(
            &state(&engine, 0.),
            7,
            Property::Position,
            None,
            &[0, 1]
        ));
        let directory = tempfile::tempdir().unwrap();
        motion_core::storage::save(directory.path(), engine.project()).unwrap();
        let reopened = Engine::new(motion_core::storage::load(directory.path()).unwrap()).unwrap();
        assert!(transform(
            &state(&reopened, 0.),
            7,
            Property::Position,
            None,
            &[0, 1]
        ));
        engine.apply(Command::RemoveExpression { target }).unwrap();
        assert!(!transform(
            &state(&engine, 0.),
            7,
            Property::Position,
            None,
            &[0, 1]
        ));
    }
}
