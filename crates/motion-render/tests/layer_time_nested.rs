use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    Camera, Composition, CompositionClip, Content, Ease, Easing, Layer, Project, VideoAsset,
    VideoClip, WorkArea,
};
use motion_render::Scene;

fn reverse(pivot: f64) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., pivot).unwrap(),
        remap: None,
    }
}
fn remap(points: &[(f64, f64, Ease)]) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::default(),
        remap: Some(
            TimeRemap::new(
                points
                    .iter()
                    .map(|&(frame, source_frame, ease)| TimeRemapKey {
                        frame,
                        source_frame,
                        easing: Easing { ease, curve: None },
                    })
                    .collect(),
            )
            .unwrap(),
        ),
    }
}
fn fixture() -> Project {
    let mut p = Project::new(32, 32, 24, 48).unwrap();
    let mut reference = Layer::solid(1, "reference", [32.; 2], [16., 16., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    let mut video = Layer::solid(7, "video", [32.; 2], [16., 16., 0.], [1.; 4]);
    video.content = Content::Video {
        video: VideoClip::new(1),
    };
    p.layers.push(reference);
    p.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/source.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 32,
        height: 32,
        rotation: 0,
        display_width: 32,
        display_height: 32,
        video_start_us: 0,
        video_end_us: 2_000_000,
        duration_us: 2_000_000,
        frame_count: 96,
        variable_frame_rate: false,
        nominal_frame_rate: 48.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    p.compositions.push(Composition {
        id: "comp-child".into(),
        name: "child".into(),
        width: 32,
        height: 32,
        fps: 48,
        frames: 96,
        work_area: WorkArea::full(96),
        guides: vec![],
        background: [0.; 4],
        camera: Camera::new(32, 32),
        layers: vec![video],
        expressions: vec![],
    });
    p.validate().unwrap();
    p
}
fn check(p: &Project, cases: &[(f64, f64, u64)]) {
    let before = p.clone();
    let mut scene = Scene::new(p).unwrap();
    for &(frame, child_frame, expected_us) in cases {
        scene
            .sample(p, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let child = &scene.nested.first().expect("child must be visible").scene;
        assert_eq!(child.frame, child_frame);
        let video = child
            .layers
            .iter()
            .find_map(|layer| layer.video.as_ref())
            .expect("video visible");
        assert_eq!(video.source_time_us, expected_us, "root C={frame}");
    }
    assert_eq!(p, &before);
}

#[test]
fn nested_reverse_enters_the_child_end_and_quantizes_once_in_decoder_microseconds() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(reverse(48.));
    check(
        &p,
        &[
            (0., 96., 1_999_999),
            (24., 48., 999_999),
            (47.5, 1., 20_832),
        ],
    );
    p.compositions[0].layers[0].time_mapping = Some(reverse(96.));
    check(
        &p,
        &[(0., 96., 0), (24., 48., 1_000_000), (47.5, 1., 1_979_167)],
    );
}

#[test]
fn zero_speed_reverse_ease_enters_the_child_end_but_a_real_freeze_does_not() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(remap(&[(0., 48., Ease::In), (48., 0., Ease::Linear)]));
    check(&p, &[(0., 96., 1_999_999), (24., 72., 1_499_999)]);
    p.compositions[0].layers[0].time_mapping = Some(reverse(96.));
    check(&p, &[(0., 96., 0), (24., 72., 500_000)]);
    p.layers[0].time_mapping = Some(remap(&[(0., 48., Ease::Linear)]));
    let before = p.clone();
    let mut scene = Scene::new(&p).unwrap();
    scene
        .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(
        scene.nested.is_empty(),
        "a genuine freeze must keep exact-point membership"
    );
    assert_eq!(p, before);
}

#[test]
fn zero_speed_curve_cursor_is_retained_when_shared_scenes_are_resampled_for_another_view() {
    use motion_core::Observer;
    use motion_render::ViewCamera;
    let mut p = fixture();
    p.layers[0].time_mapping = Some(remap(&[(0., 48., Ease::In), (48., 0., Ease::Linear)]));
    p.layers.push(Layer::camera(2, "camera", 32, 32));
    p.validate().unwrap();
    let before = p.clone();
    let mut scene = Scene::new(&p).unwrap();
    scene
        .sample_view(
            &p,
            0.,
            ViewCamera::Composition,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    let retained = scene.clone();
    let first_child = scene.nested[0].scene.clone();
    let observer = Observer::new(32, 32);
    for view in [ViewCamera::Observer(&observer), ViewCamera::Camera(2)] {
        scene
            .sample_view(&p, 24., view, &motion_core::ExpressionEvaluator)
            .unwrap();
        assert_eq!(scene.nested[0].scene.frame, 72.);
        assert_eq!(
            scene.nested[0].scene.layers[0]
                .video
                .as_ref()
                .unwrap()
                .source_time_us,
            1_499_999
        );
        assert_eq!(retained.nested[0].scene.frame, 96.);
        assert_eq!(
            retained.nested[0].scene.layers[0]
                .video
                .as_ref()
                .unwrap()
                .source_time_us,
            1_999_999
        );
        assert!(std::sync::Arc::ptr_eq(
            &first_child,
            &retained.nested[0].scene
        ));
        assert!(!std::sync::Arc::ptr_eq(
            &first_child,
            &scene.nested[0].scene
        ));
    }
    assert_eq!(p, before);
}

#[test]
fn nested_nonmonotonic_remap_keeps_its_turn_and_freeze_in_source_time() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(remap(&[
        (0., 0., Ease::Linear),
        (12., 48., Ease::Linear),
        (24., 24., Ease::Linear),
        (36., 24., Ease::Linear),
        (48., 48., Ease::Linear),
    ]));
    check(
        &p,
        &[
            (0., 0., 0),
            (8., 64., 1_333_333),
            (12., 96., 1_999_999),
            (18., 72., 1_499_999),
            (24., 48., 1_000_000),
            (30., 48., 1_000_000),
            (36., 48., 1_000_000),
        ],
    );
}

#[test]
fn nested_negative_input_selects_the_left_remap_interval_and_hold_value() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(reverse(48.));
    p.compositions[0].layers[0].time_mapping = Some(remap(&[
        (0., 0., Ease::Linear),
        (48., 96., Ease::Linear),
        (96., 0., Ease::Linear),
    ]));
    check(
        &p,
        &[(0., 96., 0), (24., 48., 1_999_999), (36., 24., 999_999)],
    );
    p.compositions[0].layers[0].time_mapping = Some(remap(&[
        (0., 0., Ease::Hold),
        (24., 48., Ease::Linear),
        (48., 96., Ease::Linear),
        (96., 0., Ease::Linear),
    ]));
    check(&p, &[(36., 24., 0), (36.5, 23., 0), (35.5, 25., 1_041_666)]);
}

#[test]
fn reverse_parent_respects_precise_child_clip_edges_without_a_composition_epsilon() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(reverse(48.));
    p.compositions[0].layers[0]
        .set_interval(
            96,
            motion_core::ClipRange {
                in_frame: 24.25,
                out_frame: 48.75,
            },
        )
        .unwrap();
    p.validate().unwrap();
    check(&p, &[(23.625, 48.75, 1_015_624), (35.75, 24.5, 510_416)]);
    let mut scene = Scene::new(&p).unwrap();
    for frame in [23.5, 35.875, 36.] {
        scene
            .sample(&p, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        assert!(
            scene.nested[0]
                .scene
                .layers
                .iter()
                .all(|layer| layer.video.is_none()),
            "C={frame}"
        );
    }
}

#[test]
fn three_composition_rates_keep_two_reversals_forward_until_the_video_leaf() {
    let mut p = fixture();
    p.layers[0].time_mapping = Some(reverse(48.));
    let Content::Composition { clip } = &mut p.layers[0].content else {
        unreachable!()
    };
    clip.composition = "comp-middle".into();
    let mut reference = Layer::solid(1, "middle", [32.; 2], [16., 16., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    reference.time_mapping = Some(reverse(60.));
    p.compositions.push(Composition {
        id: "comp-middle".into(),
        name: "middle".into(),
        width: 32,
        height: 32,
        fps: 30,
        frames: 60,
        work_area: WorkArea::full(60),
        guides: vec![],
        background: [0.; 4],
        camera: Camera::new(32, 32),
        layers: vec![reference],
        expressions: vec![],
    });
    p.validate().unwrap();
    let mut scene = Scene::new(&p).unwrap();
    for (frame, middle_frame, leaf_frame, source) in [
        (0., 60., 0., 0),
        (24., 30., 48., 1_000_000),
        (47.5, 0.625, 95., 1_979_167),
    ] {
        scene
            .sample(&p, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let middle = &scene.nested[0].scene;
        assert_eq!(middle.frame, middle_frame);
        let leaf = &middle.nested[0].scene;
        assert_eq!(leaf.frame, leaf_frame);
        assert_eq!(
            leaf.layers[0].video.as_ref().unwrap().source_time_us,
            source
        );
    }
}

#[test]
fn invalid_exposure_requests_keep_the_original_composition_error_context() {
    use motion_render::exposure_scene::{ExposureDocument, ExposureScenes};
    let p = fixture();
    for frame in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 48.] {
        let document = ExposureDocument::new(std::sync::Arc::new(p.clone())).unwrap();
        let error = match ExposureScenes::new(
            document,
            &p.composition_id,
            frame,
            false,
            None,
            motion_core::ExpressionEvaluator,
        ) {
            Err(error) => error,
            Ok(_) => panic!("invalid exposure request accepted"),
        };
        let motion_core::Error::Composition(error) = error else {
            panic!("lost composition context")
        };
        assert_eq!(error.code, "sample_failed");
        assert_eq!(error.composition, p.composition_id);
        assert_eq!(error.details["phase"], "exposure_scene_init");
        assert_eq!(error.details["sample_frame"], frame.to_string());
    }
}
