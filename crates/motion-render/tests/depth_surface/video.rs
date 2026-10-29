//! Native video color revisions are immutable while command buffers are pending.
use super::*;
use motion_render::{image_resources::Resolution, DepthImage};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn owned_video_geometry_preserves_reverse_submission_quality_and_decoder_transitions() {
    let root = tempfile::tempdir().unwrap();
    let p = video_project(root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let mut red = sampled(&p);
    let resource = field(1);
    red.attach_calibrated_depth_image(
        1,
        &resource.descriptor().source,
        &resource,
        input(),
        DepthImage::new([2, 1], vec![255, 0, 0, 255, 255, 0, 0, 255]).unwrap(),
    )
    .unwrap();
    assert_eq!(
        red.video_layers().len(),
        1,
        "source inventory must still include calibrated video"
    );
    assert!(red.video_decode_layers().is_empty());
    let mut green = sampled(&p);
    green
        .attach_calibrated_depth_image(
            1,
            &resource.descriptor().source,
            &resource,
            input(),
            DepthImage::new([2, 1], vec![0, 255, 0, 255, 0, 255, 0, 255]).unwrap(),
        )
        .unwrap();
    let a = renderer.capture_target(96, 64).unwrap();
    let b = renderer.capture_target(96, 64).unwrap();
    renderer
        .prepare_scene_assets(&red, Resolution::Full, false)
        .unwrap();
    let expected_red = renderer.capture(&red, &a).unwrap().0;
    check_oracle(&red, &expected_red);
    renderer
        .prepare_scene_assets(&green, Resolution::Full, false)
        .unwrap();
    let expected_green = renderer.capture(&green, &b).unwrap().0;
    assert_ne!(
        expected_red, expected_green,
        "distinct owned source colors must remain distinct"
    );
    for mode in [Resolution::Full, Resolution::Preview(1), Resolution::Full] {
        renderer.prepare_scene_assets(&red, mode, false).unwrap();
        let uploads = renderer.image_upload_bytes;
        renderer.prepare_scene_assets(&red, mode, false).unwrap();
        assert_eq!(
            renderer.image_upload_bytes, uploads,
            "unchanged color/quality must not reupload"
        );
        assert_eq!(renderer.capture(&red, &a).unwrap().0, expected_red);
    }
    let mut first = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&red, &a.view, 96, 64, &mut first, None)
        .unwrap();
    renderer
        .prepare_scene_assets(&green, Resolution::Full, false)
        .unwrap();
    let mut last = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&green, &b.view, 96, 64, &mut last, None)
        .unwrap();
    renderer.submit_allocated(last.finish(), |_| {}).unwrap();
    renderer.submit_allocated(first.finish(), |_| {}).unwrap();
    assert_eq!(renderer.read_target(&a).unwrap(), expected_red);
    assert_eq!(renderer.read_target(&b).unwrap(), expected_green);
    retain_fixture("video-reverse-first.png", &expected_red);
    retain_fixture("video-reverse-last.png", &expected_green);

    // Return to an ordinary decoder at the same PTS, then back to calibrated
    // colors. An old codec stamp must not suppress a required ordinary upload.
    let ordinary_pixels = [0, 255, 0, 255, 0, 255, 0, 255];
    renderer
        .upload_video_frame(1, 7, 0, 2, 1, &ordinary_pixels)
        .unwrap();
    renderer
        .prepare_scene_assets(&red, Resolution::Full, false)
        .unwrap();
    assert_eq!(renderer.capture(&red, &a).unwrap().0, expected_red);
    renderer
        .upload_video_frame(1, 7, 0, 2, 1, &ordinary_pixels)
        .unwrap();
    let mut ordinary_project = p.clone();
    ordinary_project.layers[0].depth_calibration = None;
    ordinary_project.layers.truncate(1);
    let ordinary = sampled(&ordinary_project);
    assert_eq!(ordinary.video_decode_layers().len(), 1);
    let pixels = renderer.capture(&ordinary, &b).unwrap().0;
    assert_eq!(&pixels[(32 * 96 + 48) * 4..][..4], &[0, 255, 0, 255]);
    renderer.clear_assets();
    renderer.configure_assets(&p, root.path()).unwrap();
    renderer
        .prepare_scene_assets(&red, Resolution::Full, false)
        .unwrap();
    assert_eq!(
        renderer.capture(&red, &a).unwrap().0,
        expected_red,
        "eviction must recover without a video file or decoder"
    );
}

// Shared owned-video inventory; no decoder or external binary fixture.
pub(super) fn video_project(root: &std::path::Path) -> Project {
    let mut p = project(root);
    p.assets.clear();
    p.layers[0].content = Content::Video {
        video: motion_core::VideoClip::new(7),
    };
    p.video_assets.push(motion_core::VideoAsset {
        id: 7,
        path: "assets/absent.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 2,
        height: 1,
        rotation: 0,
        display_width: 2,
        display_height: 1,
        video_start_us: 0,
        video_end_us: 2_000_000,
        duration_us: 2_000_000,
        frame_count: 60,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    p.validate().unwrap();
    p
}
