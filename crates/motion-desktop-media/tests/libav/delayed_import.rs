//! Actual libav probing, transactional import, picture decoding and PCM clocks.
use super::*;
use motion_core::{AudioClip, Content, Project, Transform};
use motion_host::{Platform, Session};
use motion_media::{AudioMixer, VideoImportOptions};
use std::{
    fs::File,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn real_delayed_video_import_shows_first_picture_and_preserves_audio_head_and_tail() {
    for (name, with_audio) in [
        ("video-delayed.mp4", false),
        ("video-delayed-with-audio-tail.mp4", true),
    ] {
        let root = tempfile::tempdir().unwrap();
        let backend = motion_desktop_media::platform();
        let mut session = Session::new(
            Project::new(256, 144, 60, 300).unwrap(),
            root.path().into(),
            backend.clone(),
        )
        .unwrap();
        let path = fixture(name);
        let original = probe(&path, None, None, &|| Ok(())).unwrap();
        assert_eq!(original.asset.video_start_us, 250_000, "{name}");
        assert_eq!(original.audio_track.is_some(), with_audio, "{name}");
        let input = path.clone();
        let worker = backend.clone();
        session
            .video_jobs
            .start(
                move || {
                    let file = File::open(input)?;
                    let bytes = file.metadata()?.len();
                    Ok((Box::new(file), Some(bytes)))
                },
                VideoImportOptions {
                    request_id: "delayed".into(),
                    at_frame: 0,
                    name: name.into(),
                    track: None,
                    audio_track: None,
                    with_audio,
                },
                false,
                Arc::new(move |path, track, audio, check| {
                    worker.probe_video(path, track, audio, check)
                }),
            )
            .unwrap();
        let began = Instant::now();
        loop {
            let status = session.video_jobs.status("delayed").unwrap();
            if status.state == "ready" {
                break;
            }
            assert_eq!(
                status.state, "running",
                "{name}: phase={} error={:?}",
                status.phase, status.error
            );
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "{name}: phase={} error={:?}",
                status.phase,
                status.error
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let committed = session
            .video_jobs
            .commit("delayed", &mut session.engine)
            .unwrap();
        assert_eq!(
            committed.state, "succeeded",
            "{name}: phase={} error={:?}",
            committed.phase, committed.error
        );
        session.video_jobs.release("delayed").unwrap();
        let project = session.engine.project().clone();
        let layer = &project.layers[0];
        let Content::Video { video } = &layer.content else {
            panic!("expected imported video")
        };
        let asset = &project.video_assets[0];
        assert_eq!(video.source_offset_us, if with_audio { 0 } else { 250_000 });
        assert_eq!(video.hold_first_frame, with_audio);
        assert_eq!(video.picture_time_us(0., project.fps, asset), 250_000);
        assert_eq!(
            video.source_time_us(0., project.fps),
            if with_audio { 0 } else { 250_000 }
        );
        let mut decoder = backend
            .open_decoder(
                &root.path().join(&asset.path),
                asset.clone(),
                original.timestamps,
            )
            .unwrap();
        // The old source-zero picture request really is outside this source.
        assert!(decoder.frame(0, &|| Ok(())).is_err(), "{name}");
        let first = decoder
            .frame(
                video.picture_time_us(0., project.fps, asset) as u64,
                &|| Ok(()),
            )
            .unwrap();
        assert_eq!(first.pts, asset.video_start_us);
        assert!(first.bytes() > 0);
        let later = video.picture_time_us(30., project.fps, asset) as u64;
        assert_eq!(later, if with_audio { 500_000 } else { 750_000 });
        let decoded = decoder.frame(later, &|| Ok(())).unwrap();
        assert!(decoded.pts <= later && later < decoded.end);

        if with_audio {
            let audio_asset = project
                .audio_assets
                .iter()
                .find(|a| Some(a.id) == asset.audio_asset)
                .unwrap();
            assert!(audio_asset.duration_us > asset.video_end_us + 500_000);
            assert_eq!(project.layer_audio(layer).unwrap().source_offset_us, 0);
            let mut audio_only = project.clone();
            audio_only.layers[0].content = Content::Audio {
                audio: AudioClip::new(audio_asset.id),
            };
            audio_only.layers[0].size = [0.; 2];
            audio_only.layers[0].transform = Transform::new([0.; 3]);
            let mut actual = AudioMixer::new(project.clone(), root.path()).unwrap();
            let mut reference = AudioMixer::new(audio_only, root.path()).unwrap();
            // The independently generated pulse fixture is silent until 0.10s.
            // 0.125s exercises audible audio before the 0.25s picture start;
            // 2s exercises audible audio after the 1.583333s picture end.
            for sample in [0, 6_000, 12_000, 96_000] {
                let mut actual_pcm = vec![0.; 4096];
                let mut reference_pcm = vec![0.; 4096];
                actual.mix(sample, &mut actual_pcm).unwrap();
                reference.mix(sample, &mut reference_pcm).unwrap();
                assert_eq!(
                    actual_pcm, reference_pcm,
                    "audio clock shifted at sample {sample}"
                );
                if sample == 0 {
                    assert!(actual_pcm.iter().all(|value| value.abs() < 0.001));
                } else {
                    assert!(
                        actual_pcm.iter().any(|value| value.abs() > 0.001),
                        "audio head/tail lost at sample {sample}"
                    );
                }
            }
        }
        motion_core::storage::save(root.path(), &project).unwrap();
        assert_eq!(motion_core::storage::load(root.path()).unwrap(), project);
        assert!(session.engine.undo().unwrap());
        assert!(session.engine.project().layers.is_empty());
        assert!(session.engine.redo().unwrap());
        assert_eq!(session.engine.project(), &project);
    }
}
