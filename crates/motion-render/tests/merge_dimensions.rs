use motion_core::{
    Command, Curve, CurveShape, CurveSpace, Ease, Easing, Engine, Layer, LayerTimeline, Project,
    Property,
};
use motion_render::{effect_plan::PlanBuilder, Renderer, Scene};

#[test]
fn merged_tracks_preserve_gpu_pixels_and_frozen_render_plan_at_fractional_times() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    let mut plans =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut distinct_frames = std::collections::BTreeSet::new();
    for (ease, curve) in [
        (Ease::Linear, None),
        (Ease::Hold, None),
        (Ease::InOut, None),
        (
            Ease::Linear,
            Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Elastic {
                    oscillations: 2.,
                    damping: 5.,
                },
            }),
        ),
        (
            Ease::Linear,
            Some(Curve {
                space: CurveSpace::Velocity,
                shape: CurveShape::Cubic {
                    control1: [0.2, -0.5],
                    control2: [0.8, 1.5],
                    start: 0.,
                    end: 0.,
                },
            }),
        ),
    ] {
        let mut p = Project::new(64, 64, 30, 120).unwrap();
        p.background = [0.; 4];
        let mut layer = Layer::solid(
            1,
            "Animated card",
            [14., 22.],
            [20., 32., 0.],
            [0.2, 0.5, 1., 1.],
        );
        layer.timeline = Some(LayerTimeline {
            in_frame: 0,
            out_frame: 120,
            offset_frame: 10,
        });
        for (track, from, to) in [
            (
                &mut layer.transform.position,
                [18., 24., 0.],
                [46., 40., 0.],
            ),
            (
                &mut layer.transform.rotation,
                [0., 0., -720.],
                [0., 0., 1080.],
            ),
            (
                &mut layer.transform.scale,
                [60., 80., 100.],
                [120., 110., 100.],
            ),
        ] {
            track.upsert(-10, from, ease).unwrap();
            track.upsert(80, to, Ease::Linear).unwrap();
            track.set_curve(-10, Easing { ease, curve }).unwrap();
            track.separate().unwrap();
        }
        p.layers.push(layer);
        let mut engine = Engine::new(p).unwrap();
        let frozen = engine.snapshot();
        engine
            .apply_batch(
                [Property::Position, Property::Rotation, Property::Scale]
                    .into_iter()
                    .map(|property| Command::MergeDimensions {
                        object: 1,
                        property,
                    })
                    .collect(),
            )
            .unwrap();
        let merged = engine.snapshot();
        let mut before = Scene::new(&frozen).unwrap();
        let mut after = Scene::new(&merged).unwrap();
        for frame in [0., 12.5, 25.25, 39.75, 70.5, 95.99] {
            before
                .sample(&frozen, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            after
                .sample(&merged, frame, None, &motion_core::ExpressionEvaluator)
                .unwrap();
            let original = renderer.capture(&before, &target).unwrap().0;
            let result = renderer.capture(&after, &target).unwrap().0;
            assert!(
                original.chunks_exact(4).any(|p| p[3] != 0),
                "fixture must render a card"
            );
            assert_eq!(
                original, result,
                "GPU pixels changed at {frame}, {ease:?}, {curve:?}"
            );
            distinct_frames.insert(result);
            let plan = plans.build(&before, &[0], 64, 64, true).unwrap();
            let mut original = vec![0; plan.buffer_bytes(&before)];
            plan.write(&before, &mut original).unwrap();
            let plan = plans.build(&after, &[0], 64, 64, true).unwrap();
            let mut result = vec![0; plan.buffer_bytes(&after)];
            plan.write(&after, &mut result).unwrap();
            assert_eq!(original, result, "frozen render plan changed at {frame}");
        }
        engine
            .apply(Command::SetVector {
                object: 1,
                property: Property::Position,
                frame: 0,
                value: [100., 100., 0.],
            })
            .unwrap();
        assert_ne!(engine.project(), &frozen);
        assert!(frozen.layers[0].transform.position.axes.is_some());
    }
    assert!(
        distinct_frames.len() > 10,
        "pixel comparisons must include changing geometry"
    );
}
