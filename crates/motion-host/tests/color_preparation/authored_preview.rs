//! Typed view forks and resolution changes retain the original CPU colour oracle.
use super::*;
use motion_core::FrameEvaluator;
use motion_core::{
    Content, Observer, Track,
    vector::{ShapeKind, VectorContent},
};
use motion_render::ViewCamera;
use std::{borrow::Cow, cell::Cell};

struct CountingEvaluator(Cell<u32>);
impl FrameEvaluator for CountingEvaluator {
    fn evaluate<'a>(
        &self,
        project: &'a Project,
        frame: f64,
    ) -> motion_core::Result<Cow<'a, Project>> {
        self.0.set(self.0.get() + 1);
        ExpressionEvaluator.evaluate(project, frame)
    }
}

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn typed_view_forks_and_bounded_previews_preserve_original_colors_without_resampling() {
    let reference = reference();
    let mut renderer = renderer();
    let mut checked = 0;
    let mut refused = 0;
    for (case_index, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, case)| case.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let mut project = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        let samples = [3, 4, 10, 11];
        for (tile, sample) in samples.into_iter().enumerate() {
            let mut layer = Layer::solid(
                tile as u64 + 1,
                "native preview quadrant",
                [32., 32.],
                [
                    16. + (tile % 2) as f32 * 32.,
                    16. + (tile / 2) as f32 * 32.,
                    0.,
                ],
                case.input[sample],
            );
            if tile % 2 == 1 {
                let mut vector = VectorContent::shape(ShapeKind::Rectangle);
                vector.fill = Some(Track::constant(case.input[sample]));
                layer.content = Content::Vector { vector };
            }
            project.layers.push(layer);
        }
        project.validate().unwrap();
        let original = serde_json::to_vec(&project).unwrap();
        let mut prepared = prep::prepare_native_authored_project(
            &project,
            root.path(),
            &runtime(),
            &Observation::new(9700 + case_index as u64),
        )
        .unwrap()
        .unwrap();
        let evaluator = CountingEvaluator(Cell::new(0));
        let frame = prepared
            .authored
            .sample(&project, 0.25, &evaluator)
            .unwrap();
        let contract = frame.contract();
        let mut observer = Observer::new(64, 64);
        observer.zoom(2.).unwrap();
        observer.pan(4., 3., 64, 64).unwrap();
        // The present admission is 2D. Distinct root camera contexts must keep
        // 2D paint/geometry unchanged; this does not claim 3D route qualification.
        let cameras = [
            ViewCamera::Composition,
            ViewCamera::Observer(&observer),
            ViewCamera::Composition,
            ViewCamera::Observer(&observer),
        ];
        for resource in project.color_management.resources().unwrap() {
            std::fs::remove_file(root.path().join(resource.path)).unwrap();
        }
        assert!(matches!(
            frame.fork_view(ViewCamera::Camera(u64::MAX)),
            Err(AuthoredSceneError::Model { phase: "view", .. })
        ));
        for bad in [
            Vec::new(),
            vec![ViewCamera::Composition; 3],
            vec![ViewCamera::Composition, ViewCamera::Camera(u64::MAX)],
        ] {
            assert!(matches!(
                frame.fork_views(&bad),
                Err(AuthoredSceneError::Model {
                    phase: "view_group",
                    ..
                })
            ));
            refused += 1;
        }
        let mut groups = Vec::new();
        for count in [1, 2, 4] {
            groups.extend(frame.fork_views(&cameras[..count]).unwrap());
        }
        assert_eq!(evaluator.0.get(), 1, "forking must not evaluate again");
        assert_eq!(
            frame.scene().camera.view_projection,
            groups[0].scene().camera.view_projection
        );
        assert_ne!(
            groups[1].scene().camera.view_projection,
            groups[2].scene().camera.view_projection,
            "observer contexts are independent"
        );
        // Advance the sampler while the original views remain owned by a group.
        prepared
            .authored
            .sample(&project, 1.75, &evaluator)
            .unwrap();
        assert_eq!(evaluator.0.get(), 2);
        for frame in groups {
            assert_eq!(frame.frame(), 0.25);
            assert_eq!(frame.size(), [64, 64]);
            assert_eq!(frame.contract(), contract);
            assert_eq!(frame.scene().color_contract(), Some(contract));
            for [width, height] in [[64, 64], [32, 32], [16, 16], [32, 48]] {
                let target = renderer
                    .color_image_target(width, height, contract)
                    .unwrap();
                let before = read(&renderer, &target);
                let readbacks = renderer.image_readback_info();
                if [width, height] != frame.size() {
                    assert!(matches!(
                        renderer.draw_authored_scene(&frame, &target),
                        Err(motion_render::RenderError::AuthoredScene(
                            AuthoredSceneError::Extent { .. }
                        ))
                    ));
                    assert_eq!(
                        read(&renderer, &target),
                        before,
                        "the exact frame consumer must still refuse changed dimensions"
                    );
                    refused += 1;
                }
                let reads_after_refusal = renderer.image_readback_info();
                renderer
                    .draw_authored_preview(&frame, &target, None)
                    .unwrap();
                assert_eq!(
                    renderer.image_readback_info(),
                    reads_after_refusal,
                    "preview composition has no intermediate readback"
                );
                assert!(reads_after_refusal.requested_calls >= readbacks.requested_calls);
                let pixels = read(&renderer, &target);
                let [vx, vy, vw, vh] = motion_render::fit_viewport(
                    [0., 0., width as f32, height as f32],
                    frame.size(),
                );
                for y in 0..height {
                    for x in 0..width {
                        let px = x as f32 + 0.5;
                        let py = y as f32 + 0.5;
                        let expected = if px < vx || px >= vx + vw || py < vy || py >= vy + vh {
                            [0.; 4]
                        } else {
                            let column = usize::from(px >= vx + vw * 0.5);
                            let row = usize::from(py >= vy + vh * 0.5);
                            premult(
                                case.expected[samples[row * 2 + column]],
                                case.processor.output_alpha,
                            )
                        };
                        let actual = pixels[(y * width + x) as usize];
                        assert!(
                            close(actual, expected),
                            "case={case_index} size={width}x{height} x={x} y={y} actual={actual:?} expected={expected:?}"
                        );
                        checked += 1;
                    }
                }
                let prior = pixels;
                assert!(
                    renderer
                        .draw(frame.scene(), target.image().view(), width, height)
                        .is_err(),
                    "the inspection scene must not authorize legacy color rendering"
                );
                assert_eq!(read(&renderer, &target), prior);
                renderer
                    .draw_authored_preview(&frame, &target, None)
                    .unwrap();
                assert_eq!(
                    read(&renderer, &target),
                    prior,
                    "explicit valid recovery restores the original bytes"
                );
            }
            let oversized = renderer.color_image_target(65, 64, contract).unwrap();
            let before = read(&renderer, &oversized);
            assert!(matches!(
                renderer.draw_authored_preview(&frame, &oversized, None),
                Err(motion_render::RenderError::AuthoredScene(
                    AuthoredSceneError::Extent { .. }
                ))
            ));
            assert_eq!(read(&renderer, &oversized), before);
            let mut wrong = contract;
            wrong.alpha = AlphaMode::Straight;
            let wrong = renderer.color_image_target(16, 16, wrong).unwrap();
            let before = read(&renderer, &wrong);
            assert!(matches!(
                renderer.draw_authored_preview(&frame, &wrong, None),
                Err(motion_render::RenderError::AuthoredScene(
                    AuthoredSceneError::Destination { .. }
                ))
            ));
            assert_eq!(read(&renderer, &wrong), before);
            refused += 2;
        }
        assert_eq!(prepared.authored.current().unwrap().frame(), 1.75);
        assert_eq!(evaluator.0.get(), 2);
        assert_eq!(serde_json::to_vec(&project).unwrap(), original);
    }
    renderer.check_health().unwrap();
    assert_eq!(checked, 290304);
    assert_eq!(refused, 228);
    println!(
        "typed view preview CPU pixels={checked}; typed contract/extent/group refusals={refused}; sampled once before forks; frozen source files absent"
    );
}
