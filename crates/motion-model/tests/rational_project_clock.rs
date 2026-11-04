use motion_model::{
    AudioAsset, AudioClip, Camera, Composition, CompositionAction, CompositionClip,
    CompositionSettings, Content, Ease, FrameRate, Layer, LayerTimeline, Project,
    VideoClip, WorkArea, CURRENT_PROJECT_VERSION,
};
use motion_model::layer_time::SourceTimeSample;
use serde_json::json;

fn rate(numerator: u32) -> FrameRate {
    FrameRate::new(numerator, 1001).unwrap()
}

#[test]
fn project_wire_preserves_exact_rates_and_rejects_integer_or_previous_schema() {
    let project = Project::new_with_frame_rate(32, 24, rate(30000), 300).unwrap();
    let wire = serde_json::to_value(&project).unwrap();
    assert_eq!(wire["fps"], json!({"numerator":30000,"denominator":1001}));
    assert_eq!(wire["version"], CURRENT_PROJECT_VERSION);
    let reopened: Project = serde_json::from_value(wire.clone()).unwrap();
    reopened.validate().unwrap();
    assert_eq!(reopened, project);
    assert_eq!(project.frame_pts_us(299).unwrap(), 9_976_633);
    assert!(project.frame_pts_us(300).is_err());
    let mut legacy = wire.clone();
    legacy["fps"] = json!(30);
    assert!(serde_json::from_value::<Project>(legacy).is_err());
    let mut previous = wire;
    previous["version"] = json!(CURRENT_PROJECT_VERSION - 1);
    assert!(serde_json::from_value::<Project>(previous).unwrap().validate().is_err());
    assert!(Project::new_with_frame_rate(32, 24, FrameRate::new(1, 2).unwrap(), 300).is_err());
    assert!(Project::new_with_frame_rate(32, 24, FrameRate::new(240001, 1000).unwrap(), 300).is_err());
}

#[test]
fn integer_project_output_timestamps_keep_the_original_rounding_at_every_frame() {
    for fps in [1, 24, 25, 30, 48, 60, 120, 240] {
        let project = Project::new(16, 16, fps, 36000).unwrap();
        for frame in 0..36000 {
            let previous = ((u64::from(frame) * 1_000_000 + u64::from(fps) / 2) / u64::from(fps)) as i64;
            assert_eq!(project.frame_pts_us(frame).unwrap(), previous, "fps={fps} frame={frame}");
        }
    }
}

#[test]
fn nested_fractional_frames_quantize_once_at_the_media_leaf() {
    let mut first = CompositionClip::new("first".into());
    first.source_start_frame = -7;
    let mut second = CompositionClip::new("second".into());
    second.source_start_frame = 2;
    let source = SourceTimeSample::new(0.25, 1.).unwrap()
        .through_composition(&first, rate(24000), rate(60000)).unwrap()
        .through_composition(&second, rate(60000), rate(30000)).unwrap();
    assert_eq!(source.frame, -1.1875);
    assert_eq!(source.rate, 1.25);
    let mut video = VideoClip::new(7);
    video.source_offset_us = 1_000_000;
    assert_eq!(source.video_time_us(&video, rate(30000)).unwrap(), 960_377);
    let pcm = source.pcm_position(rate(30000), 48000, 1_000_000).unwrap().unwrap();
    assert!((pcm - 46_098.1).abs() < 1e-9, "{pcm}");
    assert!(video.source_time_us(f64::MAX, rate(30000)).is_err());
    video.source_offset_us = u64::MAX;
    assert!(video.source_time_us(0., rate(30000)).is_err());
}

#[test]
fn nested_audio_origins_and_work_area_use_exact_absolute_sample_edges() {
    let mut project = Project::new_with_frame_rate(32, 24, rate(30000), 300).unwrap();
    project.audio_assets.push(AudioAsset {
        id: 83, path: "assets/audio.wav".into(), mime: "audio/wav".into(), bytes: 1,
        track: 0, sample_rate: 48000, channels: 2, sample_frames: 1_440_000,
        duration_us: 30_000_000,
    });
    let mut audio = Layer::solid(7, "Audio", [0.; 2], [0.; 3], [1.; 4]);
    audio.content = Content::Audio { audio: AudioClip::new(83) };
    audio.timeline = Some(LayerTimeline { in_frame: 4, out_frame: 80, offset_frame: -2 });
    project.compositions.push(Composition {
        id: "comp-child".into(), name: "Child".into(), width: 32, height: 24,
        fps: rate(24000), frames: 240, work_area: WorkArea::full(240), guides: vec![],
        background: [0.; 4], camera: Camera::new(32, 24), layers: vec![audio], expressions: vec![],
    });
    let mut reference = Layer::solid(17, "Reference", [32., 24.], [16., 12., 0.], [1.; 4]);
    let mut clip = CompositionClip::new("comp-child".into());
    clip.source_start_frame = 3;
    reference.content = Content::Composition { clip };
    reference.timeline = Some(LayerTimeline { in_frame: 10, out_frame: 200, offset_frame: 7 });
    project.layers.push(reference);
    project.validate().unwrap();
    let voices = project.audio_voices().unwrap();
    assert_eq!(voices.len(), 1);
    // Root origin 7*8008/5 - child start 3*2002 = 26026/5.
    // The leaf offset adds -2*2002 before the sole final floor.
    assert_eq!((voices[0].begin_sample, voices[0].end_sample, voices[0].offset_sample),
        (16_016, 165_365, 1_201));
    assert_eq!(WorkArea { in_frame: 1, out_frame: 5 }.sample_bounds(rate(30000), 48000).unwrap(),
        1601..8008);
}

#[test]
fn fps_changes_require_an_explicit_policy_and_keep_frames_or_seconds() {
    let mut project = Project::new_with_frame_rate(32, 24, rate(24000), 240).unwrap();
    let mut layer = Layer::solid(1, "Card", [8.; 2], [0.; 3], [1.; 4]);
    layer.transform.position.upsert(24, [24., 0., 0.], Ease::Linear).unwrap();
    project.layers.push(layer);
    let before = project.clone();
    let settings = |timing: &str| CompositionSettings {
        name: "Retimed".into(), width: 32, height: 24, fps: rate(30000), frames: 300,
        timing: timing.into(), shorten: "reject".into(),
    };
    let mut unspecified = serde_json::to_value(settings("preserve_frames")).unwrap();
    unspecified.as_object_mut().unwrap().remove("timing");
    assert!(serde_json::from_value::<CompositionSettings>(unspecified).is_err());
    project.edit_composition(CompositionAction::Settings { settings: settings("preserve_frames") }).unwrap();
    assert_eq!(project.layers[0].transform.position.keys[0].frame, 24);
    assert_eq!(project.work_area, WorkArea::full(300));
    project = before;
    project.edit_composition(CompositionAction::Settings { settings: settings("preserve_seconds") }).unwrap();
    assert_eq!(project.layers[0].transform.position.keys[0].frame, 30);
    assert_eq!(project.layers[0].transform.position.keys.len(), 1);
    assert_eq!(project.fps, rate(30000));
}
