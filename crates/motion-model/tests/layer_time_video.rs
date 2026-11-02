use motion_model::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Content, Layer, LayerTimeline, VideoAsset, VideoClip,
};

fn fixture() -> (Layer, VideoAsset) {
    let mut layer = Layer::solid(7, "video", [32.; 2], [0.; 3], [1.; 4]);
    layer.content = Content::Video {
        video: VideoClip::new(1),
    };
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 24,
        offset_frame: 0,
    });
    let asset = VideoAsset {
        id: 1,
        path: "assets/source.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 32,
        height: 32,
        rotation: 0,
        display_width: 32,
        display_height: 32,
        video_start_us: 250_000,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 18,
        variable_frame_rate: false,
        nominal_frame_rate: 24.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    };
    (layer, asset)
}

#[test]
fn unmapped_video_retains_exact_timestamp_rounding_and_leading_picture_policy() {
    let (mut layer, asset) = fixture();
    for hold in [false, true] {
        let Content::Video { video } = &mut layer.content else {
            unreachable!()
        };
        video.hold_first_frame = hold;
        for offset in [-12, 0, 19] {
            layer.timeline.as_mut().unwrap().offset_frame = offset;
            let before = layer.clone();
            for fps in [1, 24, 30, 60, 240] {
                for frame in [-1., 0., 0.25, 3.125, 6., 23.999, 24., 60.] {
                    let local = frame - f64::from(offset);
                    let Content::Video { video } = &layer.content else {
                        unreachable!()
                    };
                    assert_eq!(
                        layer.video_source_time_us(frame, fps).unwrap(),
                        video.source_time_us(local, fps)
                    );
                    assert_eq!(
                        layer.video_picture_time_us(frame, fps, &asset).unwrap(),
                        video.picture_time_us(local, fps, &asset)
                    );
                }
            }
            assert_eq!(layer, before);
        }
    }
}

#[test]
fn reverse_video_uses_the_last_decoder_quantum_then_applies_only_the_existing_head_policy() {
    let (mut layer, asset) = fixture();
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 24.).unwrap(),
        remap: None,
    });
    let before = layer.clone();
    assert_eq!(
        layer.video_picture_time_us(0., 24, &asset).unwrap(),
        999_999
    );
    assert_eq!(layer.video_source_time_us(23., 24).unwrap(), 41_666);
    assert_eq!(layer.video_source_time_us(24., 24).unwrap(), -1);
    assert_eq!(layer, before);
    let Content::Video { video } = &mut layer.content else {
        unreachable!()
    };
    video.hold_first_frame = true;
    assert_eq!(
        layer.video_picture_time_us(23., 24, &asset).unwrap(),
        250_000
    );
    assert_eq!(layer.video_source_time_us(23., 24).unwrap(), 41_666);
    assert_eq!(layer.video_picture_time_us(24., 24, &asset).unwrap(), -1);
}

#[test]
fn video_clock_failures_keep_object_phase_and_original_cause_without_changing_the_layer() {
    let (layer, mut asset) = fixture();
    let before = layer.clone();
    for error in [
        layer.video_source_time_us(0., 0).unwrap_err(),
        layer.video_source_time_us(f64::NAN, 24).unwrap_err(),
        {
            asset.id = 99;
            layer.video_picture_time_us(0., 24, &asset).unwrap_err()
        },
        Layer::solid(7, "not video", [32.; 2], [0.; 3], [1.; 4])
            .video_source_time_us(0., 24)
            .unwrap_err(),
    ] {
        let motion_model::Error::LayerTimeObject {
            object,
            phase,
            source,
        } = &error
        else {
            panic!("lost typed context: {error:?}")
        };
        assert_eq!(*object, 7);
        assert!(["video_source", "source_input", "video_picture"].contains(phase));
        assert!(!source.to_string().is_empty());
        assert!(std::error::Error::source(&error).is_some());
    }
    assert_eq!(layer, before);
}
