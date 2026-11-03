//! Explicit hardware acceptance: observe PCM consumed by the real callback,
//! rather than treating an offline mixer result as device playback evidence.
use super::*;
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    AudioAsset, AudioClip, Composition, CompositionClip, Content, Easing, Layer, WorkArea,
};
use std::{fs, io::Write};

const LOW: f32 = 1. / 64.;
const HIGH: f32 = 1. / 32.;

fn fixture(root: &Path, case: &str) -> (Project, f32) {
    let mut p = Project::new(32, 32, 30, 15).unwrap();
    let asset = AudioAsset {
        id: 83,
        path: "assets/device-clock.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 48_000,
        duration_us: 1_000_000,
    };
    let cache = motion_media::cache_path(root, &asset).unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut file = fs::File::create(cache).unwrap();
    for i in 0..48_000 {
        // Quiet plateaus exercise time mapping and gain in the real callback.
        // Smooth every edge so a peak assertion measures those plateaus rather
        // than the expected ringing of the speed-dependent low-pass filter.
        let smooth = |distance: f32| {
            let phase = (distance / 256.).clamp(0., 1.);
            0.5 - 0.5 * (std::f32::consts::PI * phase).cos()
        };
        let transition = smooth(i as f32 - 23_872.);
        let envelope = smooth(i as f32) * smooth((47_999 - i) as f32);
        let sample = (LOW + (HIGH - LOW) * transition) * envelope;
        file.write_all(&sample.to_le_bytes().repeat(2)).unwrap();
    }
    drop(file);
    p.audio_assets.push(asset);
    let mut layer = Layer::solid(7, "Mapped device PCM", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(83),
    };
    let (mapping, peak) = match case {
        "fast" => (
            LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(1, 2).unwrap(),
                    TimeDirection::Forward,
                    0.,
                    0.,
                )
                .unwrap(),
                remap: None,
            },
            HIGH,
        ),
        "slow" => (
            LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(2, 1).unwrap(),
                    TimeDirection::Forward,
                    0.,
                    0.,
                )
                .unwrap(),
                remap: None,
            },
            LOW,
        ),
        "reverse" | "nested-reverse" => (
            LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::default(),
                    TimeDirection::Reverse,
                    0.,
                    if case == "nested-reverse" { 60. } else { 30. },
                )
                .unwrap(),
                remap: None,
            },
            if case == "nested-reverse" {
                HIGH * 0.25
            } else {
                HIGH
            },
        ),
        "freeze" | "turn" => {
            let points = if case == "freeze" {
                vec![(0., 6.)]
            } else {
                vec![(0., 0.), (7.5, 30.), (15., 0.)]
            };
            (
                LayerTimeMapping {
                    clock: LayerClock::default(),
                    remap: Some(
                        TimeRemap::new(
                            points
                                .into_iter()
                                .map(|(frame, source_frame)| TimeRemapKey {
                                    frame,
                                    source_frame,
                                    easing: Easing::default(),
                                })
                                .collect(),
                        )
                        .unwrap(),
                    ),
                },
                if case == "freeze" { 0. } else { HIGH },
            )
        }
        other => panic!("unknown hardware fixture {other}"),
    };
    layer.time_mapping = Some(mapping);
    p.layers.push(layer);
    if case == "nested-reverse" {
        if let Content::Audio { audio } = &mut p.layers[0].content {
            audio.volume = 0.5;
        }
        p.compositions.push(Composition {
            id: "comp-device".into(),
            name: "60 fps source".into(),
            width: 32,
            height: 32,
            fps: motion_core::FrameRate::integer(60).unwrap(),
            frames: 60,
            work_area: WorkArea::full(60),
            background: p.background,
            camera: p.camera.clone(),
            guides: vec![],
            expressions: vec![],
            layers: std::mem::take(&mut p.layers),
        });
        let mut reference = Layer::solid(17, "30 fps parent", [32.; 2], [0.; 3], [1.; 4]);
        let mut clip = CompositionClip::new("comp-device".into());
        clip.volume = 0.5;
        reference.content = Content::Composition { clip };
        p.layers.push(reference);
    }
    p.validate().unwrap();
    (p, peak)
}

#[test]
#[ignore = "requires an actual native audio output device; explicitly run with --ignored"]
fn mapped_audio_reaches_the_real_device_for_stretch_reverse_freeze_turn_and_nested_fps() {
    let root = tempfile::tempdir().unwrap();
    let _logging = crate::logging::init(root.path().join("logs")).unwrap();
    let mut report = vec![];
    for case in [
        "fast",
        "slow",
        "reverse",
        "freeze",
        "turn",
        "nested-reverse",
    ] {
        let (project, expected_peak) = fixture(root.path(), case);
        let mut output = AudioOutput::start(project, root.path(), 0.).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut previous = 0.;
        let mut advanced = false;
        let mut peak = [0f32; 2];
        let mut polls = 0;
        loop {
            let frame = output.frame(motion_core::FrameRate::integer(30).unwrap());
            assert!(
                frame >= previous && frame <= 15.,
                "case={case} frame={frame} previous={previous}"
            );
            advanced |= frame > 0. && frame < 14.;
            previous = frame;
            polls += 1;
            for (peak, value) in peak.iter_mut().zip(output.take_levels().peak) {
                *peak = peak.max(value);
            }
            assert!(
                output.error().is_none(),
                "case={case} error={:?}",
                output.error()
            );
            if output.finished(frame, motion_core::FrameRate::integer(30).unwrap()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "case={case} device did not finish: frame={frame}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            advanced,
            "case={case} never observed advancing device clock"
        );
        assert!(
            output.shared.done.load(Ordering::Acquire),
            "case={case} producer did not finish"
        );
        let submitted = output.shared.submitted_frames.load(Ordering::Acquire);
        assert!(
            submitted > 0,
            "case={case} no PCM consumed by real callback"
        );
        // Meter measures converted samples in the actual callback. Allow the
        // supported integer device formats' quantization, not mapping errors.
        for value in peak {
            assert!(
                (value - expected_peak).abs() < 0.0001,
                "case={case} peak={peak:?} expected={expected_peak}"
            );
        }
        report.push(serde_json::json!({"case":case,"deviceRate":output.rate,"submittedFrames":submitted,
            "finalFrame":previous,"peak":peak,"expectedPeak":expected_peak,"polls":polls,"underruns":output.underruns()}));
        output.stop();
        assert!(output.shared.stop.load(Ordering::Acquire));
    }
    let log = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    assert_eq!(log.matches("audio preview start phase=start").count(), 6);
    assert!(!log.contains(" ERROR "), "{log}");
    let report = serde_json::to_string_pretty(&report).unwrap();
    println!("{report}");
    if let Some(path) = std::env::var_os("MOTION_MAPPED_DEVICE_REPORT") {
        fs::write(path, report).unwrap();
    }
}
