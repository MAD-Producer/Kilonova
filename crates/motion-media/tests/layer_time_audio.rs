use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    AudioAsset, AudioClip, Content, Easing, Layer, Project,
};
use motion_media::AudioMixer;
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
};

fn value(index: usize, channel: usize) -> f32 {
    let value = ((index % 97) as f32 - 48.) / 128.;
    if channel == 0 {
        value
    } else {
        -value * 0.5
    }
}
fn fixture() -> (tempfile::TempDir, Project) {
    let root = tempfile::tempdir().unwrap();
    let mut p = Project::new(32, 32, 10, 1).unwrap();
    let asset = AudioAsset {
        id: 1,
        path: "assets/clock.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 4_800,
        duration_us: 100_000,
    };
    let cache = motion_media::cache_path(root.path(), &asset).unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut file = fs::File::create(cache).unwrap();
    for index in 0..4_800 {
        for channel in 0..2 {
            file.write_all(&value(index, channel).to_le_bytes())
                .unwrap();
        }
    }
    let mut layer = Layer::solid(7, "clock", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    p.layers.push(layer);
    p.audio_assets.push(asset);
    p.validate().unwrap();
    (root, p)
}
fn reverse(pivot: f64) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., pivot).unwrap(),
        remap: None,
    }
}
fn remap(points: &[(f64, f64)]) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::default(),
        remap: Some(
            TimeRemap::new(
                points
                    .iter()
                    .map(|&(frame, source_frame)| TimeRemapKey {
                        frame,
                        source_frame,
                        easing: Easing::default(),
                    })
                    .collect(),
            )
            .unwrap(),
        ),
    }
}

#[test]
fn unmapped_pcm_stays_bit_exact_and_reverse_keeps_both_native_end_samples() {
    let (root, mut p) = fixture();
    let mut normal = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut expected = vec![0.; 9_600];
    assert_eq!(normal.mix(0, &mut expected).unwrap(), 4_800);
    for (index, actual) in expected.chunks_exact(2).enumerate() {
        assert_eq!(actual, [value(index, 0), value(index, 1)]);
    }
    p.layers[0].time_mapping = Some(reverse(1.));
    let before = p.clone();
    let mut mapped = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut actual = vec![0.; 9_600];
    assert_eq!(mapped.mix(0, &mut actual).unwrap(), 4_800);
    for (index, frame) in actual.chunks_exact(2).enumerate() {
        assert_eq!(
            frame,
            [value(4_799 - index, 0), value(4_799 - index, 1)],
            "output sample={index}"
        );
    }
    assert_eq!(p, before);
}

#[test]
fn genuine_freezes_are_silent_and_outside_source_time_is_zero_padded() {
    let (root, mut p) = fixture();
    for mapping in [remap(&[(0., 0.5)]), remap(&[(0., -10.), (1., -9.)])] {
        p.layers[0].time_mapping = Some(mapping);
        let mut mixer = AudioMixer::new(p.clone(), root.path()).unwrap();
        let mut output = vec![1.; 9_600];
        assert_eq!(mixer.mix(0, &mut output).unwrap(), 4_800);
        assert!(output.iter().all(|sample| *sample == 0.));
    }
}

#[test]
fn nonmonotonic_remap_returns_from_the_source_end_without_reading_the_entire_span() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(remap(&[(0., 0.), (0.5, 1.), (1., 0.)]));
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut output = vec![0.; 9_600];
    mixer.mix(0, &mut output).unwrap();
    for index in 0..4_800 {
        let source = if index < 2_400 {
            2 * index
        } else {
            9_599 - 2 * index
        };
        assert_eq!(
            &output[index * 2..index * 2 + 2],
            [value(source, 0), value(source, 1)],
            "sample={index}"
        );
    }
    let memory = mixer.memory_info();
    // Existing one-second PCM scratch plus bounded per-chunk mapping/window data.
    assert!(memory.known_payload_bytes < 1_000_000, "{memory:?}");
}

#[test]
fn mapped_plans_follow_local_level_keys_and_keep_the_unmapped_rational_path() {
    let (_root, mut p) = fixture();
    assert!(p.audio_voices().unwrap()[0].time_mapping.is_none());
    p.layers[0].time_mapping = Some(reverse(1.));
    let mut levels = motion_core::AudioLevels::default();
    levels
        .left
        .upsert(0, -20., motion_core::Ease::Linear)
        .unwrap();
    levels
        .left
        .upsert(1, 0., motion_core::Ease::Linear)
        .unwrap();
    p.layers[0].audio_levels = Some(levels);
    let voice = p.audio_voices().unwrap().remove(0);
    assert!(voice.animated_levels());
    let first = voice.mapped_sample_at(0, 48_000).unwrap().unwrap();
    assert_eq!(first.position, 4_799.);
    assert_eq!(first.gain, [1.; 2]);
    let middle = voice.mapped_sample_at(2_400, 48_000).unwrap().unwrap();
    assert!((middle.gain[0] - 10_f32.powf(-10. / 20.)).abs() < 1e-7);
}

fn nest(p: &mut Project, parent_fps: u32) {
    let child = motion_core::Composition {
        id: "comp-child".into(),
        name: "Audio child".into(),
        width: p.width,
        height: p.height,
        fps: p.fps,
        frames: p.frames,
        work_area: p.work_area,
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: std::mem::take(&mut p.layers),
        expressions: vec![],
    };
    p.compositions.push(child);
    p.fps = parent_fps;
    let mut reference = Layer::solid(17, "Reference", [32.; 2], [0.; 3], [1.; 4]);
    reference.content = Content::Composition {
        clip: motion_core::CompositionClip::new("comp-child".into()),
    };
    p.layers.push(reference);
}

#[test]
fn two_nested_reversals_with_mixed_fps_restore_forward_pcm_and_gate_the_child_end() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(reverse(1.));
    if let Content::Audio { audio } = &mut p.layers[0].content {
        audio.volume = 0.25;
    }
    nest(&mut p, 5);
    p.layers[0].time_mapping = Some(reverse(0.5));
    if let Content::Composition { clip } = &mut p.layers[0].content {
        clip.volume = 0.5;
    }
    let before = p.clone();
    let mut mixer = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut output = vec![1.; 19_200];
    assert_eq!(mixer.mix(0, &mut output).unwrap(), 9_600);
    for (index, actual) in output[..9_600].chunks_exact(2).enumerate() {
        assert_eq!(
            actual,
            [value(index, 0) * 0.125, value(index, 1) * 0.125],
            "sample={index}"
        );
    }
    assert!(output[9_600..].iter().all(|v| *v == 0.));
    assert_eq!(p, before);
}

#[test]
fn unmapped_parent_origin_and_source_start_precede_the_mapped_leaf() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(reverse(1.));
    nest(&mut p, 5);
    p.frames = 2;
    p.work_area = motion_core::WorkArea::full(2);
    p.layers[0].timeline = Some(motion_core::LayerTimeline {
        in_frame: 0,
        out_frame: 2,
        offset_frame: 1,
    });
    let voice = p.audio_voices().unwrap().remove(0);
    assert!(voice.mapped_sample_at(9_599, 48_000).unwrap().is_none());
    assert_eq!(
        voice
            .mapped_sample_at(9_600, 48_000)
            .unwrap()
            .unwrap()
            .position,
        4_799.
    );
    assert!(voice.mapped_sample_at(14_400, 48_000).unwrap().is_none());
    let mut mixer = AudioMixer::new(p.clone(), root.path()).unwrap();
    let mut output = vec![1.; 38_400];
    mixer.mix(0, &mut output).unwrap();
    assert!(output[..19_200].iter().all(|v| *v == 0.));
    for (index, actual) in output[19_200..28_800].chunks_exact(2).enumerate() {
        assert_eq!(actual, [value(4_799 - index, 0), value(4_799 - index, 1)]);
    }
    assert!(output[28_800..].iter().all(|v| *v == 0.));
    if let Content::Composition { clip } = &mut p.layers[0].content {
        clip.source_start_frame = 1;
    }
    let mut shifted = AudioMixer::new(p, root.path()).unwrap();
    shifted.mix(0, &mut output).unwrap();
    for (index, actual) in output[9_600..19_200].chunks_exact(2).enumerate() {
        assert_eq!(actual, [value(4_799 - index, 0), value(4_799 - index, 1)]);
    }
    assert!(output[..9_600]
        .iter()
        .chain(&output[19_200..])
        .all(|v| *v == 0.));
}

#[test]
fn mapped_subframe_trim_and_source_offset_survive_arbitrary_block_partitions() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(reverse(0.5));
    p.layers[0]
        .set_interval(
            1,
            motion_core::ClipRange {
                in_frame: 0.,
                out_frame: 0.5,
            },
        )
        .unwrap();
    if let Content::Audio { audio } = &mut p.layers[0].content {
        audio.source_offset_us = 25_000;
    }
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut output = vec![1.; 9_600];
    mixer.mix(0, &mut output).unwrap();
    for (index, actual) in output[..4_800].chunks_exact(2).enumerate() {
        assert_eq!(actual, [value(3_599 - index, 0), value(3_599 - index, 1)]);
    }
    assert!(output[4_800..].iter().all(|v| *v == 0.));
    for (start, count) in [(0, 1), (1, 255), (256, 1_999), (2_255, 145), (2_400, 2_400)] {
        let mut part = vec![1.; count * 2];
        mixer.mix(start as u64, &mut part).unwrap();
        assert_eq!(part, output[start * 2..(start + count) * 2]);
    }
}

#[test]
fn half_speed_sinc_interpolation_matches_a_known_tone_and_seeks_are_reproducible() {
    let (root, mut p) = fixture();
    let cache = motion_media::cache_path(root.path(), &p.audio_assets[0]).unwrap();
    let mut file = fs::File::create(cache).unwrap();
    for index in 0..4_800 {
        let v = 0.25 * (std::f32::consts::TAU * index as f32 / 48.).sin();
        file.write_all(&v.to_le_bytes()).unwrap();
        file.write_all(&(-v * 0.5).to_le_bytes()).unwrap();
    }
    drop(file);
    p.layers[0].time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(2, 1).unwrap(),
            TimeDirection::Forward,
            0.,
            0.,
        )
        .unwrap(),
        remap: None,
    });
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut output = vec![0.; 9_600];
    mixer.mix(0, &mut output).unwrap();
    for index in 128..4_800 {
        let expected = 0.25 * (std::f32::consts::TAU * index as f32 / 96.).sin();
        assert!(
            (output[index * 2] - expected).abs() < 0.001,
            "sample={index}"
        );
        assert_eq!(output[index * 2 + 1], -output[index * 2] * 0.5);
    }
    for start in [0, 127, 255, 257, 4_001] {
        let mut part = vec![0.; 798];
        mixer.mix(start as u64, &mut part).unwrap();
        assert_eq!(part, output[start * 2..start * 2 + 798]);
    }
}

#[test]
fn distant_source_windows_skip_poisoned_gaps_and_keep_pcm_scratch_bounded() {
    let (root, mut p) = fixture();
    let asset = &mut p.audio_assets[0];
    asset.sample_rate = 8_000;
    asset.channels = 1;
    asset.sample_frames = 4_800_000;
    asset.duration_us = 600_000_000;
    let cache = motion_media::cache_path(root.path(), asset).unwrap();
    let mut file = fs::File::create(cache).unwrap();
    file.set_len(asset.sample_frames * 4).unwrap();
    // This gap lies between all requested tap windows. Reading a single span
    // between extrema would both encounter the NaN and allocate megabytes.
    file.seek(SeekFrom::Start(2_000_500 * 4)).unwrap();
    file.write_all(&f32::NAN.to_le_bytes()).unwrap();
    file.seek(SeekFrom::Start(3_999_999 * 4)).unwrap();
    file.write_all(&0.25f32.to_le_bytes()).unwrap();
    drop(file);
    p.layers[0].time_mapping = Some(remap(&[(0., 0.), (0.5, 5_000.), (1., 0.)]));
    let mut mixer = AudioMixer::new(p, root.path()).unwrap();
    let mut output = vec![0.; 9_600];
    mixer.mix(0, &mut output).unwrap();
    assert_eq!(&output[4_800..4_802], &[0.25; 2]);
    assert!(output.iter().all(|v| v.is_finite()));
    let memory = mixer.memory_info();
    assert!(memory.known_capacity_bytes < 2_000_000, "{memory:?}");
}

#[test]
fn mapped_cache_read_errors_keep_asset_output_context_and_the_native_io_cause() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(reverse(1.));
    let mut mixer = AudioMixer::new(p.clone(), root.path()).unwrap();
    let cache = motion_media::cache_path(root.path(), &p.audio_assets[0]).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(cache)
        .unwrap()
        .set_len(0)
        .unwrap();
    let error = mixer.mix(0, &mut [0.; 20]).unwrap_err();
    let message = error.to_string();
    for field in [
        "asset=1",
        "first_sample=",
        "sample_frames=",
        "output_begin=0",
        "output_end=10",
    ] {
        assert!(message.contains(field), "{message}");
    }
    match error.primary() {
        motion_media::Error::Io(cause) => {
            assert_eq!(cause.kind(), std::io::ErrorKind::UnexpectedEof)
        }
        other => panic!("lost typed read cause: {other:?}"),
    }
}

#[test]
fn frozen_voice_does_not_read_pcm_but_a_moving_voice_reports_nonfinite_pcm() {
    let (root, mut p) = fixture();
    p.layers[0].time_mapping = Some(remap(&[(0., 0.5)]));
    let mut frozen = AudioMixer::new(p.clone(), root.path()).unwrap();
    let cache = motion_media::cache_path(root.path(), &p.audio_assets[0]).unwrap();
    let mut file = fs::OpenOptions::new().write(true).open(cache).unwrap();
    file.seek(SeekFrom::Start(4_799 * 8)).unwrap();
    file.write_all(&f32::NAN.to_le_bytes()).unwrap();
    let mut output = [1.; 20];
    frozen.mix(0, &mut output).unwrap();
    assert_eq!(output, [0.; 20]);
    p.layers[0].time_mapping = Some(reverse(1.));
    let mut moving = AudioMixer::new(p.clone(), root.path()).unwrap();
    let message = moving.mix(0, &mut output).unwrap_err().to_string();
    for field in [
        "asset=1",
        "sample=4799",
        "channel=0",
        "output_begin=0",
        "output_end=10",
    ] {
        assert!(message.contains(field), "{message}");
    }
    file.set_len(0).unwrap();
    frozen.mix(0, &mut output).unwrap();
    assert_eq!(output, [0.; 20]);
}
