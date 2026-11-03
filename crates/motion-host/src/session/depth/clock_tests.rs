use super::*;
use motion_core::{FrameRate, LayerTimeline, RateError, VideoClip};

fn fixture() -> (Project, Layer, VideoAsset) {
    let project = Project::new_with_frame_rate(32, 32,
        FrameRate::new(30000, 1001).unwrap(), 72).unwrap();
    let mut layer = Layer::solid(7, "video", [32.; 2], [0.; 3], [1.; 4]);
    let mut video = VideoClip::new(1);
    video.source_offset_us = 250_000;
    layer.content = Content::Video { video };
    layer.timeline = Some(LayerTimeline { in_frame: 0, out_frame: 48, offset_frame: 0 });
    let asset = VideoAsset {
        id: 1, path: "assets/source.mp4".into(), bytes: 1, mime: "video/avc".into(),
        track: 0, width: 32, height: 32, rotation: 0, display_width: 32, display_height: 32,
        video_start_us: 250_000, video_end_us: 1_000_000, duration_us: 1_000_000,
        frame_count: 18, variable_frame_rate: true, nominal_frame_rate: 24.,
        color_standard: 1, color_range: 2, audio_asset: None,
    };
    (project, layer, asset)
}

#[test]
fn exact_fractional_depth_clock_distinguishes_visible_samples_and_normal_misses() {
    let (project, mut layer, asset) = fixture();
    assert_eq!(video_target(&project, &layer, &asset, 0.25).unwrap(), Some(258_342));
    assert_eq!(video_target(&project, &layer, &asset, 1.5).unwrap(), Some(300_050));
    for frame in [-1., 48., 72., f64::NAN] {
        assert_eq!(video_target(&project, &layer, &asset, frame).unwrap(), None);
    }
    let Content::Video { video } = &mut layer.content else { unreachable!() };
    video.source_offset_us = 0;
    assert_eq!(video_target(&project, &layer, &asset, 0.).unwrap(), None);
}

#[test]
fn depth_clock_overflow_keeps_typed_primary_cause_and_source_context() {
    let (project, mut layer, asset) = fixture();
    let Content::Video { video } = &mut layer.content else { unreachable!() };
    // Controlled invalid origin exercises the fallible public-clock boundary.
    video.source_offset_us = u64::MAX;
    let error = video_target(&project, &layer, &asset, 0.25).unwrap_err();
    let crate::Error::Context { context, cause } = error else { panic!("clock lost context") };
    assert!(context.contains("object=7") && context.contains("asset=1")
        && context.contains("frame=0.25") && context.contains("phase=source_time"));
    assert!(matches!(*cause, crate::Error::Model(motion_core::Error::FrameRate(RateError::NumericRange))));
    // Inactive layers remain normal misses without evaluating an irrelevant clock.
    assert_eq!(video_target(&project, &layer, &asset, 48.).unwrap(), None);
}
