//! Authored-property classification for the shared inspector/timeline filter.
//! Defaults describe a newly created layer in the current composition, rather
//! than the sampled value at the current playhead or the last saved document.
use motion_core::{Axis, Camera, Property};
use serde_json::Value;
use std::time::{Duration, Instant};

/// A first press is delivered immediately. Only a fresh second press in the
/// same interaction context becomes UU; callers discard repeat/IME events.
#[derive(Default)]
pub(super) struct RevealSequence {
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
    let camera_layer = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|layer| layer["id"] == object)
        .is_some_and(|layer| layer["content"]["kind"] == "camera");
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
