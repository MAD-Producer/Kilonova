use motion_model::{
    AudioAsset, AudioClip, Camera, ClipRange, Composition, CompositionClip, Content, Error, Layer,
    LayerTimeline, Project, MAX_COMPOSITION_FPS, MAX_FRAMES,
};
use std::error::Error as _;

fn project() -> Project {
    let mut project = Project::new(32, 24, 30, 90).unwrap();
    project.audio_assets.push(AudioAsset {
        id: 83,
        path: "assets/audio.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 144_000,
        duration_us: 3_000_000,
    });
    let mut layer = Layer::solid(7, "Audio", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(83),
    };
    project.layers.push(layer);
    project.validate().unwrap();
    project
}

fn nested() -> Project {
    let mut project = project();
    project.compositions.push(Composition {
        id: "comp-child".into(),
        name: "Audio child".into(),
        width: 32,
        height: 24,
        fps: 30,
        frames: 90,
        background: [0.; 4],
        camera: Camera::new(32, 24),
        layers: std::mem::take(&mut project.layers),
        expressions: vec![],
    });
    let mut reference = Layer::solid(17, "Reference", [32., 24.], [16., 12., 0.], [1.; 4]);
    reference.content = Content::Composition {
        clip: CompositionClip::new("comp-child".into()),
    };
    project.layers.push(reference);
    project.validate().unwrap();
    project
}

fn timeline_failure(project: &Project, composition: &str) {
    let before = format!("{project:?}");
    let failure = project.audio_voices().unwrap_err();
    let text = failure.to_string();
    assert!(
        text.contains(&format!("composition={composition}")),
        "{text}"
    );
    assert!(text.contains("object=7"), "{text}");
    let source = failure
        .source()
        .expect("audio timeline lost the native validation error");
    let native = source
        .downcast_ref::<Box<Error>>()
        .expect("audio timeline source lost its declared native Box<Error>");
    assert!(matches!(native.as_ref(), Error::Invalid(_)), "{native}");
    assert_eq!(format!("{project:?}"), before);
}

#[test]
fn invalid_precise_audio_intervals_keep_the_original_cause_and_do_not_mutate_input() {
    for nested_document in [false, true] {
        for range in [
            ClipRange {
                in_frame: f64::NAN,
                out_frame: 2.,
            },
            ClipRange {
                in_frame: 0.,
                out_frame: f64::NAN,
            },
            ClipRange {
                in_frame: f64::NEG_INFINITY,
                out_frame: 2.,
            },
            ClipRange {
                in_frame: 0.,
                out_frame: f64::INFINITY,
            },
            ClipRange {
                in_frame: 2.,
                out_frame: 1.,
            },
            ClipRange {
                in_frame: 1.,
                out_frame: 1.,
            },
            ClipRange {
                in_frame: 0.,
                out_frame: f64::MAX,
            },
            // Finite and ordered, but outside the authored integer envelope.
            ClipRange {
                in_frame: 0.25,
                out_frame: 1.25,
            },
        ] {
            let mut project = if nested_document { nested() } else { project() };
            let layer = if nested_document {
                &mut project.compositions[0].layers[0]
            } else {
                &mut project.layers[0]
            };
            layer.timeline = Some(LayerTimeline {
                in_frame: 0,
                out_frame: 3,
                offset_frame: 0,
            });
            layer.subframe_range = Some(range);
            timeline_failure(
                &project,
                if nested_document {
                    "comp-child"
                } else {
                    "comp-main"
                },
            );
            let actual = if nested_document {
                project.compositions[0].layers[0].subframe_range.unwrap()
            } else {
                project.layers[0].subframe_range.unwrap()
            };
            assert_eq!(actual.in_frame.to_bits(), range.in_frame.to_bits());
            assert_eq!(actual.out_frame.to_bits(), range.out_frame.to_bits());
        }
    }
}

#[test]
fn invalid_integer_audio_clips_are_errors_even_when_the_bad_interval_would_be_silent() {
    for timeline in [
        LayerTimeline {
            in_frame: 2,
            out_frame: 1,
            offset_frame: 0,
        },
        LayerTimeline {
            in_frame: 1,
            out_frame: 1,
            offset_frame: 0,
        },
        LayerTimeline {
            in_frame: 0,
            out_frame: 2,
            offset_frame: i32::MIN,
        },
    ] {
        let mut project = project();
        project.layers[0].timeline = Some(timeline);
        timeline_failure(&project, "comp-main");
    }
}

#[test]
fn invalid_audio_composition_bounds_never_panic_or_return_a_partial_plan() {
    for nested_document in [false, true] {
        for (fps, frames) in [
            (0, 90),
            (MAX_COMPOSITION_FPS + 1, 90),
            (30, 0),
            (30, MAX_FRAMES + 1),
        ] {
            let mut project = if nested_document { nested() } else { project() };
            if nested_document {
                project.compositions[0].fps = fps;
                project.compositions[0].frames = frames;
            } else {
                project.fps = fps;
                project.frames = frames;
            }
            let before = serde_json::to_vec(&project).unwrap();
            let result = std::panic::catch_unwind(|| project.audio_voices())
                .expect("audio planning panicked");
            assert!(
                result.is_err(),
                "fps={fps} frames={frames} nested={nested_document}"
            );
            assert_eq!(serde_json::to_vec(&project).unwrap(), before);
        }
    }
}

#[test]
fn valid_fractional_boundaries_offsets_and_normal_silence_keep_exact_samples() {
    let mut project = project();
    project.layers[0]
        .set_interval(
            90,
            ClipRange {
                in_frame: -0.25,
                out_frame: 1.25,
            },
        )
        .unwrap();
    project.layers[0].timeline.as_mut().unwrap().offset_frame = -1;
    project.validate().unwrap();
    let before = project.clone();
    let voices = project.audio_voices().unwrap();
    assert_eq!(voices.len(), 1);
    assert_eq!(
        (
            voices[0].begin_sample,
            voices[0].end_sample,
            voices[0].offset_sample
        ),
        (0, 2_000, -1_600)
    );
    assert_eq!(voices[0].asset, 83);
    assert_eq!(project, before);
    project.layers[0]
        .set_interval(
            90,
            ClipRange {
                in_frame: -10.,
                out_frame: -1.,
            },
        )
        .unwrap();
    project.layers[0].timeline.as_mut().unwrap().offset_frame = -10;
    project.validate().unwrap();
    assert!(project.audio_voices().unwrap().is_empty());
    project.layers[0]
        .set_interval(
            90,
            ClipRange {
                in_frame: 0.,
                out_frame: 2.,
            },
        )
        .unwrap();
    let Content::Audio { audio } = &mut project.layers[0].content else {
        unreachable!()
    };
    audio.muted = true;
    project.validate().unwrap();
    assert!(project.audio_voices().unwrap().is_empty());
}

#[test]
fn audio_graph_instance_budget_accepts_the_boundary_and_rejects_excess_without_mutation() {
    fn reference(id: u64, target: usize) -> Layer {
        let mut layer = Layer::solid(id, "Reference", [32., 24.], [16., 12., 0.], [1.; 4]);
        layer.content = Content::Composition {
            clip: CompositionClip::new(format!("comp-level-{target}")),
        };
        layer
    }
    // A shared binary DAG has 1023 expanded instances, including its root.
    let mut project = Project::new(32, 24, 30, 90).unwrap();
    project.layers = vec![reference(1, 1), reference(2, 1), reference(3, 9)];
    for level in 1..=9 {
        project.compositions.push(Composition {
            id: format!("comp-level-{level}"),
            name: "Shared audio graph".into(),
            width: 32,
            height: 24,
            fps: 30,
            frames: 90,
            background: [0.; 4],
            camera: Camera::new(32, 24),
            layers: if level == 9 {
                vec![]
            } else {
                vec![reference(1, level + 1), reference(2, level + 1)]
            },
            expressions: vec![],
        });
    }
    assert_eq!(motion_model::composition::MAX_COMPOSITION_INSTANCES, 1024);
    project.validate().unwrap();
    let before = project.clone();
    assert!(project.audio_voices().unwrap().is_empty());
    assert_eq!(project, before);

    project.layers.push(reference(4, 9));
    assert!(project.validate().is_err());
    let before = project.clone();
    let failure = project.audio_voices().unwrap_err();
    let Error::Composition(detail) = failure else {
        panic!("audio graph lost its native composition limit error");
    };
    assert_eq!(detail.code, "resource_limit");
    assert_eq!(detail.composition, "comp-level-9");
    assert_eq!(detail.details["max_instances"], 1024);
    assert_eq!(project, before);
}
