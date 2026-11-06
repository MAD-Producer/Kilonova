#![cfg(feature = "ffmpeg")]

use motion_core::{Content, Layer, LayerTimeline, Project, VideoClip};
use motion_desktop_media::{
    encode::Mp4Encoder,
    movie::{ExportControl, FrozenMovie},
};
use motion_host::Session;
use motion_render::{AllocationBudget, AllocationCategory};
use std::{fs, path::Path};

const EDGE: usize = 32;
const CLIPS: u32 = 60;
const COLORS: [[u8; 4]; 3] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]];
const ONE_VIDEO: u64 = (EDGE * EDGE * (4 + 1) + EDGE * EDGE / 2) as u64;

fn project(root: &Path) -> Project {
    fs::create_dir_all(root.join("assets")).unwrap();
    let source = root.join("assets/source.mp4");
    let mut project = Project::new(EDGE as u32, EDGE as u32, 30, CLIPS).unwrap();
    let mut encoder = Mp4Encoder::new(&source, EDGE as u32, EDGE as u32, project.fps).unwrap();
    for frame in 0..6 {
        encoder
            .video_frame(&COLORS[frame as usize % 3].repeat(EDGE * EDGE), frame)
            .unwrap();
    }
    let mut sample = 0u64;
    while sample < 9_600 {
        let count = encoder.audio_frame_size().min((9_600 - sample) as usize);
        encoder.audio_frame(&vec![0.; count * 2], sample).unwrap();
        sample += count as u64;
    }
    encoder.finish().unwrap();
    let mut probe =
        motion_desktop_media::av::probe(&source, None, Some(u32::MAX), &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), 6);
    probe.asset.id = 1;
    probe.asset.path = "assets/source.mp4".into();
    let index = motion_media::video_cache_path(root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    for frame in 0..CLIPS {
        let mut video = VideoClip::new(1);
        video.source_offset_us = probe.timestamps[frame as usize % 3];
        video.muted = true;
        let mut layer = Layer::solid(
            u64::from(frame) + 1,
            "Video clip",
            [EDGE as f32; 2],
            [EDGE as f32 / 2., EDGE as f32 / 2., 0.],
            [1.; 4],
        );
        layer.content = Content::Video { video };
        layer.timeline = Some(LayerTimeline {
            in_frame: frame as i32,
            out_frame: frame as i32 + 1,
            offset_frame: frame as i32,
        });
        project.layers.push(layer);
    }
    project.video_assets.push(probe.asset);
    project.validate().unwrap();
    project
}

fn assert_empty(budget: &AllocationBudget, context: &str) {
    let info = budget.info().unwrap();
    assert_eq!(info.admitted_bytes, 0, "{context}: {info:?}");
    assert_eq!(info.live_allocations, 0, "{context}: {info:?}");
    assert_eq!(info.encoder_scopes, 0, "{context}: {info:?}");
    assert!(
        info.categories
            .iter()
            .all(|category| category.admitted_bytes == 0),
        "{context}"
    );
}

fn assert_video_bound(budget: &AllocationBudget) {
    let info = budget.info().unwrap();
    let video = info
        .categories
        .iter()
        .find(|entry| entry.category == AllocationCategory::Video)
        .unwrap();
    assert!(
        video.admitted_bytes <= ONE_VIDEO,
        "a completed frame must not retain earlier clips: {info:?}"
    );
}

fn assert_pixels(path: &Path) {
    let probe = motion_desktop_media::av::probe(path, None, Some(u32::MAX), &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), CLIPS as usize);
    let timestamps = probe.timestamps.clone();
    let mut decoder =
        motion_desktop_media::av::Decoder::open(path, probe.asset, probe.timestamps).unwrap();
    for frame in [0usize, 1, 2, 15, 16, 17, 57, 58, 59] {
        let decoded = decoder.frame(timestamps[frame], &|| Ok(())).unwrap();
        let image = decoded.rgba().unwrap();
        let offset = ((EDGE / 2) * EDGE + EDGE / 2) * 4;
        let actual = &image[offset..offset + 4];
        let expected = COLORS[frame % 3];
        // Two native H.264/YUV420 round trips are lossy. The wrong clip differs
        // by about 255 on two channels, well outside this codec allowance.
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 16),
            "frame={frame} actual={actual:?} expected={expected:?}"
        );
        eprintln!(
            "native video pixels: output={} frame={frame} actual={actual:?} expected={expected:?}",
            path.file_name().unwrap().to_string_lossy()
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn sixty_clip_frozen_exports_release_shared_admissions_after_cancel_success_and_nested_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    let outputs = temporary.path().join("outputs");
    fs::create_dir(&outputs).unwrap();
    let mut expected_outputs = Vec::new();
    let source = project(&root);
    for nested in [false, true] {
        let mut engine = motion_core::Engine::new(source.clone()).unwrap();
        if nested {
            engine
                .apply(motion_core::Command::Composition {
                    action: motion_core::CompositionAction::Precompose {
                        objects: (1..=u64::from(CLIPS)).collect(),
                        name: "Nested videos".into(),
                        range: "composition".into(),
                    },
                })
                .unwrap();
        }
        motion_core::storage::save(&root, engine.project()).unwrap();
        let reopened = motion_core::storage::load(&root).unwrap();
        assert_eq!(&reopened, engine.project());
        let session =
            Session::new(reopened, root.clone(), motion_desktop_media::platform()).unwrap();
        let budget = session.allocation_budget().clone();
        assert_empty(&budget, "headless session before freeze");

        for cancel in [true, false] {
            let movie = FrozenMovie::from_session(&session).unwrap();
            assert_empty(&budget, "freeze owns no GPU resources");
            let output = outputs.join(format!("nested-{nested}-cancel-{cancel}.mp4"));
            if cancel {
                fs::write(&output, b"previous output").unwrap();
            }
            let control = ExportControl::default();
            let mut reports = Vec::new();
            let result = movie.export(&output, &control, |progress| {
                assert_video_bound(&budget);
                reports.push(progress);
                if cancel && progress > 0.5 {
                    assert!(control.cancel());
                }
            });
            assert!(
                reports.len() > 20,
                "must process actual video frames before cancelling"
            );
            if cancel {
                assert_eq!(result.err().as_deref(), Some("movie export cancelled"));
                assert_eq!(fs::read(&output).unwrap(), b"previous output");
            } else {
                let result = result.unwrap();
                assert_eq!(result.frames, CLIPS);
                assert_pixels(&output);
            }
            assert_empty(
                &budget,
                "export returned and its renderer has been destroyed",
            );
            let mut entries: Vec<_> = fs::read_dir(&outputs)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            entries.sort();
            expected_outputs.push(output.file_name().unwrap().to_owned());
            expected_outputs.sort();
            assert_eq!(
                entries, expected_outputs,
                "temporary movie must be removed on every terminal path"
            );
            eprintln!(
                "native video retirement: nested={nested} cancel={cancel} reports={} peak={}",
                reports.len(),
                budget.info().unwrap().peak_admitted_bytes
            );
        }
        drop(session);
        assert_empty(&budget, "session closed after export");
    }
}
