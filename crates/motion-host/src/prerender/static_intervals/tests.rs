use super::*;
use motion_core::{Layer, LayerTimeline};

fn project() -> Project {
    let mut value = Project::new(64, 64, 30, 120).unwrap();
    value.layers.push(Layer::solid(
        1,
        "constant",
        [32.; 2],
        [32., 32., 0.],
        [1.; 4],
    ));
    value
}

#[test]
fn constant_clips_reduce_only_playback_lookahead_inside_each_half_open_interval() {
    let mut project = project();
    project.layers[0].timeline = Some(LayerTimeline {
        in_frame: 20,
        out_frame: 80,
        offset_frame: -5,
    });
    project.validate().unwrap();
    let intervals = StaticIntervals::from_project(&project).unwrap();
    assert_eq!(intervals.at(0.), Some([0., 20.]));
    assert_eq!(intervals.at(20.), Some([20., 80.]));
    assert_eq!(intervals.at(80.), Some([80., 120.]));
    for frame in [-1., 120., f64::NAN, f64::INFINITY] {
        assert_eq!(intervals.at(frame), None);
    }
    assert_eq!(intervals.window(40., Direction::Forward, 3), 2);
    assert_eq!(intervals.window(40., Direction::Backward, 3), 2);
    assert_eq!(intervals.window(18., Direction::Forward, 3), 3);
    assert_eq!(intervals.window(20., Direction::Backward, 3), 3);
    assert_eq!(intervals.window(77., Direction::Forward, 3), 3);
    assert_eq!(intervals.window(80., Direction::Backward, 3), 3);
    assert_eq!(intervals.window(40., Direction::Forward, 0), 0);
    assert_eq!(intervals.window(40., Direction::Forward, 1), 1);
    assert_eq!(intervals.window(40., Direction::Forward, 2), 2);
}

#[test]
fn authored_animation_and_unknown_temporal_sources_keep_the_full_window() {
    let mut animated = project();
    animated.layers[0]
        .transform
        .position
        .set_animated(0, true)
        .unwrap();
    assert!(StaticIntervals::from_project(&animated).is_none());

    let mut camera = project();
    camera.camera.fov.set_animated(0, true).unwrap();
    assert!(StaticIntervals::from_project(&camera).is_none());

    let mut expression = project();
    expression
        .expressions
        .push(motion_core::PropertyExpression {
            target: motion_core::ExpressionTarget::Property {
                object: 1,
                property: motion_core::Property::Opacity,
                axis: None,
            },
            source: "50 + Math.sin(time) * 10".into(),
            enabled: true,
            seed: 0,
            profile: motion_core::EXPRESSION_PROFILE.into(),
        });
    expression.validate().unwrap();
    assert!(StaticIntervals::from_project(&expression).is_none());

    // The clip's own clock is independent of authored transform keys.
    for content in [
        Content::Video {
            video: motion_core::VideoClip::new(2),
        },
        Content::Composition {
            clip: motion_core::CompositionClip::new("nested".into()),
        },
        Content::Model {
            model: motion_core::ModelClip {
                asset: 2,
                scene: 0,
                pixels_per_metre: 100.,
                pivot: [0.; 3],
                samples: 1,
                animation: None,
            },
        },
    ] {
        let mut source = project();
        source.layers[0].content = content;
        assert!(StaticIntervals::from_project(&source).is_none());
    }
}
