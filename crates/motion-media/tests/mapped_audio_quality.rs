use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    AudioAsset, AudioClip, Content, Layer, Project,
};
use motion_media::AudioMixer;
use std::{fs, io::Write};
#[path = "mapped_audio_quality/reference.rs"]
mod reference;

fn fixture(rate: u32, stretch: [u32; 2], reverse: bool, high: f64) -> (tempfile::TempDir, Project) {
    let root = tempfile::tempdir().unwrap();
    let mut project = Project::new(32, 24, 30, 30).unwrap();
    let asset = AudioAsset {
        id: 1,
        path: "assets/quality.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: rate,
        channels: 2,
        sample_frames: u64::from(rate) * 2,
        duration_us: 2_000_000,
    };
    let path = motion_media::cache_path(root.path(), &asset).unwrap();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    for i in 0..asset.sample_frames {
        for frequency in [250., high] {
            let value = (0.25
                * (std::f64::consts::TAU * frequency * i as f64 / f64::from(rate)).sin())
                as f32;
            file.write_all(&value.to_le_bytes()).unwrap();
        }
    }
    file.flush().unwrap();
    let mut layer = Layer::solid(7, "Mapped audio quality", [0.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(stretch[0], stretch[1]).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 60. } else { 0. },
        )
        .unwrap(),
        remap: None,
    });
    project.layers.push(layer);
    project.audio_assets.push(asset);
    project.validate().unwrap();
    (root, project)
}

#[test]
fn speedups_filter_folded_frequencies_and_preserve_the_pitch_shifted_passband() {
    for (rate, stretch, high) in [
        (48_000, [1, 2], 18_000.),
        (44_100, [1, 4], 8_000.),
        (96_000, [1, 8], 7_000.),
        (48_000, [2, 3], 20_000.),
    ] {
        for reverse in [false, true] {
            let (root, project) = fixture(rate, stretch, reverse, high);
            let voice = project.audio_voices().unwrap().remove(0);
            let mut mixer = AudioMixer::new(project, root.path()).unwrap();
            let mut output = vec![0.; 8192];
            mixer.mix(512, &mut output).unwrap();
            let rms = (output
                .chunks_exact(2)
                .map(|s| f64::from(s[1]).powi(2))
                .sum::<f64>()
                / 4096.)
                .sqrt();
            assert!(
                rms < 0.015,
                "rate={rate} stretch={stretch:?} reverse={reverse} alias RMS={rms}"
            );
            let mut largest_error = 0f64;
            for (i, sample) in output.chunks_exact(2).enumerate() {
                let position = voice
                    .mapped_sample_at(512 + i as u64, rate)
                    .unwrap()
                    .unwrap()
                    .position;
                let expected =
                    0.25 * (std::f64::consts::TAU * 250. * position / f64::from(rate)).sin();
                largest_error = largest_error.max((f64::from(sample[0]) - expected).abs());
            }
            assert!(
                largest_error < 0.001,
                "rate={rate} stretch={stretch:?} reverse={reverse} passband error={largest_error}"
            );
            println!("rate={rate} stretch={stretch:?} reverse={reverse} alias_rms={rms} passband_error={largest_error}");
        }
    }
}

#[test]
fn slowing_high_rate_media_preserves_frequencies_now_inside_output_nyquist() {
    for reverse in [false, true] {
        let (root, project) = fixture(96_000, [2, 1], reverse, 32_000.);
        let voice = project.audio_voices().unwrap().remove(0);
        let mut mixer = AudioMixer::new(project, root.path()).unwrap();
        let mut output = vec![0.; 8192];
        mixer.mix(512, &mut output).unwrap();
        for (i, frame) in output.chunks_exact(2).enumerate() {
            let mapped = voice
                .mapped_sample_at(512 + i as u64, 96_000)
                .unwrap()
                .unwrap();
            assert!((mapped.native_rate - if reverse { -1. } else { 1. }).abs() < 1e-12);
            let quantized_position = (mapped.position * 1024.).round() / 1024.;
            assert_eq!(quantized_position.fract(), 0.);
            let expected = (0.25
                * (std::f64::consts::TAU * 32_000. * quantized_position / 96_000.).sin())
                as f32;
            assert_eq!(frame[1], expected, "reverse={reverse} sample={i}");
        }
    }
}

#[test]
fn changing_speed_and_turns_are_independent_of_block_partitions_and_match_reference() {
    use motion_core::layer_time::{TimeRemap, TimeRemapKey};
    let (root, mut project) = fixture(48_000, [1, 1], false, 18_000.);
    project.layers[0].time_mapping.as_mut().unwrap().remap = Some(
        TimeRemap::new(
            [(0., 0.), (15., 45.), (30., 0.)]
                .into_iter()
                .map(|(frame, source_frame)| TimeRemapKey {
                    frame,
                    source_frame,
                    easing: motion_core::Easing {
                        ease: motion_core::Ease::InOut,
                        curve: None,
                    },
                })
                .collect(),
        )
        .unwrap(),
    );
    let before = project.clone();
    let voice = project.audio_voices().unwrap().remove(0);
    let mut mixer = AudioMixer::new(project.clone(), root.path()).unwrap();
    let mut output = vec![0.; 19_200];
    mixer.mix(20_000, &mut output).unwrap();
    for i in (0..9600).step_by(17) {
        let sample = 20_000 + i as u64;
        let mapped = voice.mapped_sample_at(sample, 48_000).unwrap().unwrap();
        let half = if sample < 24_000 {
            sample as f64 / 24_000.
        } else {
            (sample - 24_000) as f64 / 24_000.
        };
        let expected_rate = 18. * half * (1. - half) * if sample < 24_000 { 1. } else { -1. };
        assert!((mapped.native_rate - expected_rate).abs() < 1e-12);
        for (channel, hz) in [250., 18_000.].into_iter().enumerate() {
            let expected = reference::sample(mapped.position, expected_rate, 96_000, |n| {
                (0.25 * (std::f64::consts::TAU * hz * n as f64 / 48_000.).sin()) as f32
            });
            assert!(
                (output[i * 2 + channel] - expected).abs() < 1e-7,
                "sample={sample} channel={channel} expected={expected} actual={}",
                output[i * 2 + channel]
            );
        }
    }
    let mut split = AudioMixer::new(project.clone(), root.path()).unwrap();
    let mut start = 0;
    for count in [1, 127, 256, 511, 4093, 4612] {
        let mut part = vec![0.; count * 2];
        split.mix(20_000 + start as u64, &mut part).unwrap();
        assert_eq!(part, output[start * 2..(start + count) * 2]);
        start += count;
    }
    assert_eq!(start, 9600);
    for start in [8000, 255, 1200, 4097] {
        let mut part = vec![0.; 510];
        split.mix(20_000 + start as u64, &mut part).unwrap();
        assert_eq!(part, output[start * 2..start * 2 + 510]);
    }
    assert!(split.memory_info().known_capacity_bytes < 2_000_000);
    assert_eq!(project, before);
}

#[test]
fn composed_rate_overflow_retains_mapping_context_and_refuses_before_pcm_reads() {
    use motion_core::layer_time::{TimeRemap, TimeRemapKey};
    let (root, mut project) = fixture(192_000, [1, 1], false, 18_000.);
    project.layers[0].time_mapping.as_mut().unwrap().remap = Some(
        TimeRemap::new(
            [(0., 0.), (1e-308, 1.)]
                .into_iter()
                .map(|(frame, source_frame)| TimeRemapKey {
                    frame,
                    source_frame,
                    easing: Default::default(),
                })
                .collect(),
        )
        .unwrap(),
    );
    project.validate().unwrap();
    let voice = project.audio_voices().unwrap().remove(0);
    let error = voice.mapped_sample_at(0, 192_000).unwrap_err();
    assert!(matches!(
        error,
        motion_core::Error::LayerTime {
            phase: "pcm_rate",
            ..
        }
    ));
    let mut mixer = AudioMixer::new(project.clone(), root.path()).unwrap();
    // If mapping erroneously continued into file reads, this would cause EOF.
    let path = motion_media::cache_path(root.path(), &project.audio_assets[0]).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(0)
        .unwrap();
    let error = mixer.mix(0, &mut [0.; 20]).unwrap_err();
    let message = error.to_string();
    for context in [
        "asset=1",
        "output_sample=0",
        "phase=map",
        "native PCM derivative is non-finite",
    ] {
        assert!(message.contains(context), "{message}");
    }
    assert!(error
        .primary()
        .to_string()
        .contains("native PCM derivative is non-finite"));
}

#[test]
fn low_pass_step_response_matches_reference_without_clamping_the_overshoot() {
    let (root, project) = fixture(48_000, [1, 2], false, 18_000.);
    let asset = &project.audio_assets[0];
    let path = motion_media::cache_path(root.path(), asset).unwrap();
    let mut file = std::io::BufWriter::new(fs::File::create(path).unwrap());
    let source = |n: u64| if n < 24_000 { 1f32 / 64. } else { 1f32 / 32. };
    for n in 0..asset.sample_frames {
        file.write_all(&source(n).to_le_bytes().repeat(2)).unwrap();
    }
    file.flush().unwrap();
    let frames = asset.sample_frames;
    let mut mixer = AudioMixer::new(project, root.path()).unwrap();
    let mut output = vec![0.; 1024];
    mixer.mix(11_744, &mut output).unwrap();
    let mut peak = 0f32;
    for (i, frame) in output.chunks_exact(2).enumerate() {
        let expected = reference::sample(2. * (11_744 + i) as f64, 2., frames, source);
        for actual in frame {
            assert!((*actual - expected).abs() < 1e-7);
            peak = peak.max(*actual);
        }
    }
    // Correct low-pass ringing is not an amplifier/gain error. A raw-sample
    // shortcut or a clamp to the source plateau must both fail this control.
    assert!(peak > 1. / 32. + 0.001, "step response peak={peak}");
    println!("independent step-response peak={peak}");
}
