use motion_core::*;
use serde_json::json;

fn project() -> Project {
    let mut p = Project::new(64, 64, 24, 72).unwrap();
    p.layers
        .push(Layer::solid(1, "layer", [32.; 2], [32., 32., 0.], [1.; 4]));
    p
}
fn set(e: &mut Engine, first: f64, last: f64) -> Result<()> {
    e.apply(Command::SetLayerRange {
        object: 1,
        in_frame: first,
        out_frame: last,
    })
}
#[test]
fn half_open_fractional_ranges_and_covering_bounds_roundtrip_without_changing_tracks() {
    let mut e = Engine::new(project()).unwrap();
    let transform = e.project().layers[0].transform.clone();
    set(&mut e, -0.25, 2.75).unwrap();
    let p = e.snapshot();
    let l = &p.layers[0];
    assert_eq!(p.version, 15);
    assert_eq!(l.clip(72).in_frame, -1);
    assert_eq!(l.clip(72).out_frame, 3);
    assert!(l.active(0., 72));
    assert!(l.active(2.749, 72));
    assert!(!l.active(2.75, 72));
    assert!(!l.active(-0.1, 72));
    assert_eq!(l.transform, transform);
    let restored: Project = serde_json::from_slice(&serde_json::to_vec(&p).unwrap()).unwrap();
    assert_eq!(restored, p);
    restored.validate().unwrap();
    set(&mut e, 0.25, 0.75).unwrap();
    assert!(!e.project().layers[0].active(0., 72));
    assert!(e.project().layers[0].active(0.5, 72));
    let before = e.snapshot();
    assert!(set(&mut e, 0.75, 0.25).is_err());
    assert_eq!(e.snapshot(), before);
}
#[test]
fn moving_trimming_splitting_and_undo_keep_exact_unedited_edges() {
    let mut e = Engine::new(project()).unwrap();
    set(&mut e, 0.25, 20.75).unwrap();
    let original = e.snapshot();
    e.apply(Command::MoveLayerClip {
        object: 1,
        in_frame: 10,
    })
    .unwrap();
    assert_eq!(
        e.project().layers[0].interval(72),
        ClipRange {
            in_frame: 10.25,
            out_frame: 30.75
        }
    );
    assert_eq!(e.project().layers[0].clip(72).offset_frame, 10);
    e.apply(Command::TrimLayerClip {
        object: 1,
        in_frame: 11,
        out_frame: 31,
    })
    .unwrap();
    assert_eq!(
        e.project().layers[0].interval(72),
        ClipRange {
            in_frame: 11.,
            out_frame: 30.75
        }
    );
    e.apply(Command::SplitLayerClip {
        object: 1,
        frame: 20,
    })
    .unwrap();
    assert_eq!(
        e.project().layers[0].interval(72),
        ClipRange {
            in_frame: 11.,
            out_frame: 20.
        }
    );
    assert_eq!(
        e.project().layers[1].interval(72),
        ClipRange {
            in_frame: 20.,
            out_frame: 30.75
        }
    );
    for _ in 0..3 {
        e.undo().unwrap();
    }
    assert_eq!(e.snapshot(), original);
    for _ in 0..3 {
        e.redo().unwrap();
    }
    assert_eq!(e.project().layers[1].interval(72).out_frame, 30.75);
    set(&mut e, 0., 10.).unwrap();
    assert_eq!(e.project().layers[0].subframe_range,Some(ClipRange{in_frame:0.,out_frame:10.}));
}
#[test]
fn settings_and_precomposition_preserve_ranges_and_explicit_shrink_policy() {
    let mut e = Engine::new(project()).unwrap();
    set(&mut e, 0.25, 70.75).unwrap();
    e.apply(Command::Composition {
        action: CompositionAction::Precompose {
            objects: vec![1],
            name: "child".into(),
            range: "composition".into(),
        },
    })
    .unwrap();
    let child = e.project().compositions[0].id.clone();
    assert_eq!(
        e.project().compositions[0].layers[0].interval(72).out_frame,
        70.75
    );
    let settings = |frames, shorten: &str| Command::InComposition {
        composition: child.clone(),
        command: Box::new(Command::Composition {
            action: CompositionAction::Settings {
                settings: CompositionSettings {
                    name: "child".into(),
                    width: 64,
                    height: 64,
                    fps: 48,
                    frames,
                    timing: "preserve_seconds".into(),
                    shorten: shorten.into(),
                },
            },
        }),
    };
    e.apply(settings(144, "reject")).unwrap();
    assert_eq!(
        e.project().compositions[0].layers[0].interval(144),
        ClipRange {
            in_frame: 0.5,
            out_frame: 141.5
        }
    );
    let before = e.snapshot();
    assert!(e.apply(settings(100, "reject")).is_err());
    assert_eq!(e.snapshot(), before);
    e.apply(settings(100, "trim")).unwrap();
    assert_eq!(
        e.project().compositions[0].layers[0].interval(100),
        ClipRange {
            in_frame: 0.5,
            out_frame: 100.
        }
    );
}
#[test]
fn exact_audio_interval_keeps_decoded_duration_precision_and_sample_boundaries() {
    let mut p = Project::new(64, 64, 24, 360).unwrap();
    p.version = 15;
    p.audio_assets.push(AudioAsset {
        id: 1,
        path: "assets/music.mp3".into(),
        bytes: 64,
        mime: "audio/mpeg".into(),
        track: 0,
        sample_rate: 44100,
        channels: 1,
        sample_frames: 441001,
        duration_us: 10000022,
    });
    let mut layer = Layer::solid(1, "audio", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: -12,
        out_frame: 217,
        offset_frame: -24,
    });
    let end = (-1. + 441001.0 / 44100.) * 24.;
    layer
        .set_interval(
            p.frames,
            ClipRange {
                in_frame: -12.,
                out_frame: end,
            },
        )
        .unwrap();
    p.layers.push(layer);
    p.validate().unwrap();
    let v = &p.audio_voices().unwrap()[0];
    assert_eq!(v.begin_sample, 0);
    assert_eq!(v.end_sample, (end * 2000.).ceil() as u64);
    assert_eq!(v.offset_sample, -48000);
    assert_eq!(p.layers[0].interval(p.frames).out_frame, end);
    let mut e = Engine::new(p).unwrap();
    assert!(set(&mut e, -12., end + 0.01).is_err());
}
#[test]
fn stored_cover_mismatch_unknown_formats_and_locked_targets_are_rejected() {
    let mut p = project();
    p.version = 15;
    p.layers[0].subframe_range = Some(ClipRange {
        in_frame: 0.25,
        out_frame: 10.75,
    });
    assert!(p.validate().is_err());
    p.layers[0]
        .set_interval(
            72,
            ClipRange {
                in_frame: 0.25,
                out_frame: 10.75,
            },
        )
        .unwrap();
    p.validate().unwrap();
    p.version = 14;
    assert!(p.validate().is_err());
    p.version = 18;
    p.validate().unwrap();
    p.version = 20;
    p.validate().unwrap();
    p.version = 21;
    assert!(p.validate().is_err());
    p.version = 15;
    p.layers[0].locked = true;
    let mut e = Engine::new(p).unwrap();
    assert!(set(&mut e, 0., 4.).is_err());
    assert!(parse_commands(
        &json!({"op":"move_layer_clip","object":1,"in_frame":0.25}).to_string()
    )
    .is_err());
}

#[test]
fn splitting_precise_audio_at_an_integer_does_not_change_boundary_convention() {
    let mut p=Project::new(64,64,59,59).unwrap();p.version=15;
    p.audio_assets.push(AudioAsset{id:1,path:"assets/tone.wav".into(),bytes:64,mime:"audio/wav".into(),track:0,
        sample_rate:48000,channels:1,sample_frames:48000,duration_us:1_000_000});
    let mut l=Layer::solid(1,"audio",[0.;2],[0.;3],[1.;4]);l.content=Content::Audio{audio:AudioClip::new(1)};
    l.set_interval(59,ClipRange{in_frame:0.,out_frame:2.25}).unwrap();p.layers.push(l);
    let mut e=Engine::new(p).unwrap();e.apply(Command::SplitLayerClip{object:1,frame:1}).unwrap();
    let voices=e.project().audio_voices().unwrap();assert_eq!(voices[0].end_sample,814);
    assert_eq!(voices[1].begin_sample,814);assert!(e.project().layers[0].subframe_range.is_some());
}
