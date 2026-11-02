#![cfg(feature = "ffmpeg")]
//! Real frozen-movie/AAC output against independently authored flat PCM.
use motion_core::{
    layer_time::{
        LayerClock, LayerTimeMapping, TimeDirection, TimeRemap, TimeRemapKey, TimeStretch,
    },
    AudioAsset, AudioClip, Composition, CompositionClip, Content, Layer, Project,
};
use motion_desktop_media::movie::{ExportControl, FrozenMovie};
use std::{fs, io::Write, path::Path};

fn tone(index: usize) -> f32 {
    let envelope = 0.1 + 0.3 * index as f32 / 9_600.;
    envelope * (std::f32::consts::TAU * index as f32 / 48.).sin()
}
fn reverse(pivot: f64) -> LayerTimeMapping {
    LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., pivot).unwrap(),
        remap: None,
    }
}
fn project(root: &Path, pcm: &[f32]) -> Project {
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("assets/authored.wav"), [0]).unwrap();
    let mut p = Project::new(32, 32, 10, 2).unwrap();
    let asset = AudioAsset {
        id: 1,
        path: "assets/authored.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 9_600,
        duration_us: 200_000,
    };
    let cache = motion_media::cache_path(root, &asset).unwrap();
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    let mut file = fs::File::create(cache).unwrap();
    assert_eq!(pcm.len(), 19_200);
    for sample in pcm {
        file.write_all(&sample.to_le_bytes()).unwrap();
    }
    p.audio_assets.push(asset);
    let mut sound = Layer::solid(7, "Authored tone", [0.; 2], [0.; 3], [1.; 4]);
    sound.content = Content::Audio {
        audio: AudioClip::new(1),
    };
    p.layers.push(sound);
    p.layers.push(Layer::solid(
        8,
        "Visible control",
        [16.; 2],
        [16., 16., 0.],
        [0.2, 0.4, 0.8, 1.],
    ));
    p
}
fn decode(path: &Path) -> Vec<[f32; 2]> {
    use ffmpeg_next::{codec, format, frame, media};
    let mut input = format::input(path).unwrap();
    let stream = input.streams().best(media::Type::Audio).unwrap();
    assert_eq!(stream.parameters().id(), codec::Id::AAC);
    let track = stream.index();
    let mut decoder = codec::context::Context::from_parameters(stream.parameters())
        .unwrap()
        .decoder()
        .audio()
        .unwrap();
    let mut output = vec![];
    let mut read = |decoder: &mut codec::decoder::Audio| {
        let mut decoded = frame::Audio::empty();
        loop {
            match decoder.receive_frame(&mut decoded) {
                Ok(()) => {}
                Err(ffmpeg_next::Error::Eof) => break,
                Err(ffmpeg_next::Error::Other { errno })
                    if errno == ffmpeg_next::util::error::EAGAIN =>
                {
                    break
                }
                Err(error) => panic!("decode exported AAC: {error}"),
            }
            assert_eq!(decoded.rate(), 48_000);
            assert_eq!(decoded.channels(), 2);
            assert_eq!(decoded.plane::<f32>(0).len(), decoded.plane::<f32>(1).len());
            output.extend(
                decoded
                    .plane::<f32>(0)
                    .iter()
                    .zip(decoded.plane::<f32>(1))
                    .map(|(left, right)| [*left, *right]),
            );
        }
    };
    for (stream, packet) in input.packets() {
        if stream.index() == track {
            decoder.send_packet(&packet).unwrap();
            read(&mut decoder);
        }
    }
    decoder.send_eof().unwrap();
    read(&mut decoder);
    assert!(
        output.len() >= 9_600,
        "missing exported samples: {}",
        output.len()
    );
    assert!(output.iter().flatten().all(|v| v.is_finite()));
    output
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn frozen_movie_remapped_aac_equals_flat_authored_pcm_after_live_edits() {
    assert!(motion_desktop_media::encode::available());
    let pcm: Vec<f32> = (0..9_600)
        .flat_map(|index| [tone(index), -tone(index) * 0.5])
        .collect();
    for mode in ["reverse", "nested", "remap", "freeze"] {
        let dir = tempfile::tempdir().unwrap();
        let mapped_root = dir.path().join("mapped");
        let flat_root = dir.path().join("flat");
        let mut mapped = project(&mapped_root, &pcm);
        mapped.layers[0].time_mapping = Some(reverse(2.));
        let reference: Vec<f32> = (0..9_600)
            .flat_map(|index| {
                let source = match mode {
                    "reverse" => Some(9_599 - index),
                    "nested" => Some(index),
                    "remap" if index < 2_400 => Some(2 * index),
                    "remap" if index < 4_800 => Some(9_599 - 2 * index),
                    "remap" if index >= 7_200 => Some(2 * index - 14_400),
                    _ => None,
                };
                source.map_or([0.; 2], |index| [tone(index), -tone(index) * 0.5])
            })
            .collect();
        if mode == "nested" {
            let child = Composition {
                id: "comp-child".into(),
                name: "Reversed child".into(),
                width: 32,
                height: 32,
                fps: 10,
                frames: 2,
                work_area: mapped.work_area,
                guides: vec![],
                background: mapped.background,
                camera: mapped.camera.clone(),
                layers: std::mem::take(&mut mapped.layers),
                expressions: vec![],
            };
            mapped.compositions.push(child);
            mapped.fps = 5;
            mapped.frames = 1;
            mapped.work_area = motion_core::WorkArea::full(1);
            let mut reference =
                Layer::solid(17, "Reversed reference", [32.; 2], [16., 16., 0.], [1.; 4]);
            reference.content = Content::Composition {
                clip: CompositionClip::new("comp-child".into()),
            };
            reference.time_mapping = Some(reverse(1.));
            mapped.layers.push(reference);
        } else if mode != "reverse" {
            let points = if mode == "freeze" {
                vec![(0., 0.5)]
            } else {
                vec![(0., 0.), (0.5, 1.), (1., 0.), (1.5, 0.), (2., 1.)]
            };
            mapped.layers[0].time_mapping = Some(LayerTimeMapping {
                clock: LayerClock::default(),
                remap: Some(
                    TimeRemap::new(
                        points
                            .into_iter()
                            .map(|(frame, source_frame)| TimeRemapKey {
                                frame,
                                source_frame,
                                easing: Default::default(),
                            })
                            .collect(),
                    )
                    .unwrap(),
                ),
            });
        }
        let flat = project(&flat_root, &reference);
        let authored = serde_json::to_vec_pretty(&mapped).unwrap();
        let registry = motion_effects::Registry::new_with_builtins().unwrap();
        let budget = motion_render::resource_policy::MAX_SCRATCH_BUDGET;
        let snapshot =
            FrozenMovie::new(mapped.clone(), &mapped_root, registry.clone(), budget, 7).unwrap();
        let control = FrozenMovie::new(flat, &flat_root, registry, budget, 8).unwrap();
        // The exported plan and PCM must survive live project/cache changes.
        mapped.layers.clear();
        let cache = motion_media::cache_path(&mapped_root, &mapped.audio_assets[0]).unwrap();
        fs::write(cache, vec![0; pcm.len() * 4]).unwrap();
        fs::write(
            mapped_root.join("assets/authored.wav"),
            b"changed live source",
        )
        .unwrap();
        let mapped_file = dir.path().join("mapped.mp4");
        let flat_file = dir.path().join("flat.mp4");
        let actual = snapshot
            .export(&mapped_file, &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(actual.revision, 7);
        assert!(actual.bytes > 0);
        control
            .export(&flat_file, &ExportControl::default(), |_| {})
            .unwrap();
        let actual = decode(&mapped_file);
        let expected = decode(&flat_file);
        assert_eq!(actual, expected, "mapped/flat decoded AAC mismatch: {mode}");
        if mode == "freeze" {
            assert!(actual.iter().flatten().all(|v| v.abs() < 1e-6));
        } else {
            assert!(actual.iter().flatten().any(|v| v.abs() > 0.1));
        }
        println!(
            "mode={mode} decoded_aac_frames={} exact_flat_pcm_control=true frozen_revision=7",
            actual.len()
        );
        if let Some(root) = std::env::var_os("MOTION_TIME_AUDIO_EVIDENCE") {
            let destination = std::path::PathBuf::from(root).join(mode);
            fs::create_dir_all(&destination).unwrap();
            fs::copy(&mapped_file, destination.join("mapped.mp4")).unwrap();
            fs::copy(&flat_file, destination.join("flat.mp4")).unwrap();
            fs::write(destination.join("authored-project.json"), &authored).unwrap();
            fs::write(
                destination.join("flat-reference.f32"),
                reference
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            fs::write(
                destination.join("decoded-aac.f32"),
                actual
                    .iter()
                    .flatten()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        }
    }
}
