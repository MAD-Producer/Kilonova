use super::*;
use motion_core::{AudioAsset, AudioClip, Content, FrameScope, Layer, LayerTimeline, WorkArea};
use std::io::Write;

fn fixture(root: &Path) -> Project {
    let mut project = Project::new(64, 64, 59, 18).unwrap();
    project.background = [0., 0., 0., 1.];
    project.work_area = WorkArea {
        in_frame: 5,
        out_frame: 11,
    };
    let mut visible = Layer::solid(
        1,
        "Range sentinel",
        [64.; 2],
        [32., 32., 0.],
        [1., 0., 0., 1.],
    );
    visible.timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 11,
        offset_frame: 0,
    });
    project.layers.push(visible);
    let samples = WorkArea::full(project.frames)
        .sample_bounds(project.fps, 48_000)
        .unwrap();
    let audio = AudioAsset {
        id: 2,
        path: "assets/tone.wav".into(),
        mime: "audio/wav".into(),
        bytes: 1,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: samples.end,
        duration_us: samples.end * 1_000_000 / 48_000,
    };
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join(&audio.path), [0]).unwrap();
    let pcm_path = motion_media::cache_path(root, &audio).unwrap();
    fs::create_dir_all(pcm_path.parent().unwrap()).unwrap();
    let mut pcm = fs::File::create(pcm_path).unwrap();
    let selected = project
        .work_area
        .sample_bounds(project.fps, 48_000)
        .unwrap();
    for (index, value) in super::tests::tone(samples.end as usize, 0)
        .into_iter()
        .enumerate()
    {
        let value = if selected.contains(&(index as u64 / 2)) {
            value
        } else {
            0.
        };
        pcm.write_all(&value.to_le_bytes()).unwrap();
    }
    let mut sound = Layer::solid(2, "Audio sentinel", [0.; 2], [0.; 3], [0.; 4]);
    sound.content = Content::Audio {
        audio: AudioClip::new(2),
    };
    project.audio_assets.push(audio);
    project.layers.push(sound);
    project.validate().unwrap();
    project
}

fn freeze(project: Project, root: &Path) -> FrozenMovie {
    FrozenMovie::new(
        project,
        root,
        Registry::new_with_builtins().unwrap(),
        motion_render::resource_policy::MAX_SCRATCH_BUDGET,
        73,
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn selected_work_area_uses_absolute_source_frames_pcm_and_relative_output_timestamps() {
    let root = tempfile::tempdir().unwrap();
    let mut live = fixture(root.path());
    let snapshot = freeze(live.clone(), root.path());
    // Scope is selected after the frozen document was captured. Neither later
    // live range edits nor layer edits may leak into this export revision.
    live.work_area = WorkArea::full(live.frames);
    live.layers[0].visible = false;
    let snapshot = snapshot.with_scope(FrameScope::WorkArea);
    assert_eq!(snapshot.frames(), 6);
    let file = root.path().join("work-area.mp4");
    let control = ExportControl::default();
    let mut progress = Vec::new();
    let result = snapshot
        .export(&file, &control, |value| progress.push(value))
        .unwrap();
    assert_eq!(result.frames, 6);
    assert_eq!(result.fps, 59);
    assert_eq!(result.revision, 73);
    assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(progress.last(), Some(&1.));
    let probe = crate::av::probe(&file, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.asset.frame_count, 6);
    assert_eq!(
        probe.timestamps[0], 0,
        "output must begin at zero, not source frame five"
    );
    let center = |pixels: &[u8]| pixels[(32 * 64 + 32) * 4..(32 * 64 + 33) * 4].to_vec();
    let first = center(&probe.first_rgba);
    let end = *probe.timestamps.last().unwrap();
    let mut decoder = crate::av::Decoder::open(&file, probe.asset, probe.timestamps).unwrap();
    let last = center(&decoder.frame(end, &|| Ok(())).unwrap().rgba().unwrap());
    for color in [first, last] {
        assert!(
            color[0] > 220 && color[1] < 25 && color[2] < 25,
            "source-frame sentinel {color:?}"
        );
    }
    let bounds = WorkArea {
        in_frame: 5,
        out_frame: 11,
    }
    .sample_bounds(59, 48_000)
    .unwrap();
    let expected = (bounds.end - bounds.start) as usize;
    let (samples, rms) = super::tests::decoded_audio_rms(&file);
    assert!(
        (expected..=expected + 1024).contains(&samples),
        "selected PCM samples={samples} expected={expected}"
    );
    assert!(
        rms > 0.3 && rms < 0.4,
        "selected PCM must contain the original tone, RMS={rms}"
    );
}

#[test]
fn full_composition_is_default_and_reselecting_scope_uses_the_same_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture(root.path());
    let snapshot = freeze(project, root.path());
    assert_eq!(snapshot.frames(), 18);
    let snapshot = snapshot.with_scope(FrameScope::WorkArea);
    assert_eq!(snapshot.frames(), 6);
    let snapshot = snapshot.with_scope(FrameScope::Composition);
    assert_eq!(snapshot.frames(), 18);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cancelling_a_range_export_preserves_destination_and_cleans_only_its_temporary() {
    let root = tempfile::tempdir().unwrap();
    let project = fixture(root.path());
    let output = root.path().join("existing.mp4");
    fs::write(&output, b"preserve existing output").unwrap();
    let snapshot = freeze(project, root.path()).with_scope(FrameScope::WorkArea);
    let entries = fs::read_dir(root.path()).unwrap().count();
    let control = ExportControl::default();
    let cancel = control.clone();
    let error = snapshot
        .export(&output, &control, move |progress| {
            if progress > 0.1 {
                cancel.cancel();
            }
        })
        .err()
        .unwrap();
    assert_eq!(error, "movie export cancelled");
    assert_eq!(fs::read(&output).unwrap(), b"preserve existing output");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), entries);
}
