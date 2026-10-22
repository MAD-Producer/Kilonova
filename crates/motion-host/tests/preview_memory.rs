use motion_core::{Layer, Project, VideoSample};
use motion_host::{
    preview_video::PreviewVideoCache,
    video_frame::{DecodedFrame, VideoPixels},
};
use motion_render::{ChromaLayout, Renderer, Scene, Yuv420Frame};
use std::sync::Arc;

fn yuv(layout: ChromaLayout, rotation: u32, standard: u32, range: u32) -> Yuv420Frame {
    let (cw, ch) = (4, 3);
    let mut uv = Vec::new();
    for _ in 0..ch {
        match layout {
            ChromaLayout::Uv => {
                for _ in 0..cw {
                    uv.extend_from_slice(&[74, 190]);
                }
            }
            ChromaLayout::Vu => {
                for _ in 0..cw {
                    uv.extend_from_slice(&[190, 74]);
                }
            }
            ChromaLayout::PlanarRows => {
                uv.extend(vec![74; cw]);
                uv.extend(vec![190; cw]);
            }
        }
    }
    Yuv420Frame {
        width: 7,
        height: 5,
        phase: [1, 1],
        rotation,
        standard,
        range,
        y: vec![90; 35],
        uv,
        chroma_layout: layout,
    }
}
fn decoded(pixels: VideoPixels, width: u32, height: u32) -> Arc<DecodedFrame> {
    Arc::new(DecodedFrame {
        pixels,
        width,
        height,
        pts: 123,
        end: 456,
        decode_us: 0,
        codec_us: 0,
        transfer_us: 0,
        pack_us: 0,
        source_transfer: "fixture",
        decoder_name: "fixture".into(),
    })
}
fn video_scene(w: u32, h: u32) -> Scene {
    let mut p = Project::new(w, h, 30, 60).unwrap();
    p.layers.push(Layer::solid(
        1,
        "video",
        [w as f32, h as f32],
        [w as f32 / 2., h as f32 / 2., 0.],
        [1.; 4],
    ));
    let mut s = Scene::new(&p).unwrap();
    s.sample(&p, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    s.layers[0].video = Some(VideoSample {
        asset: 1,
        source_time_us: 0,
    });
    s
}
#[test]
fn yuv_proxies_keep_rotation_matrix_range_and_absorb_all_native_chroma_layouts() {
    for layout in [ChromaLayout::Uv, ChromaLayout::Vu, ChromaLayout::PlanarRows] {
        for rotation in [0, 90, 180, 270] {
            for standard in [1, 2, 4] {
                for range in [1, 2] {
                    let frame = yuv(layout, rotation, standard, range);
                    let original = frame.to_rgba().unwrap();
                    let proxy = frame.preview_proxy(3).unwrap().unwrap();
                    assert!(proxy.bytes() < frame.bytes());
                    assert_eq!(proxy.phase, [0, 0]);
                    assert_eq!(
                        (proxy.rotation, proxy.standard, proxy.range),
                        (rotation, standard, range)
                    );
                    assert!(
                        proxy
                            .to_rgba()
                            .unwrap()
                            .chunks_exact(4)
                            .all(|p| p == &original[..4])
                    );
                    assert!(frame.preview_proxy(8).unwrap().is_none());
                }
            }
        }
    }
    let mut gradient = yuv(ChromaLayout::Uv, 0, 1, 2);
    gradient.y = (0..35).collect();
    assert_eq!(
        gradient.preview_proxy(3).unwrap().unwrap().y,
        vec![0, 2, 4, 7, 9, 11, 21, 23, 25]
    );
}
#[test]
fn proxy_cache_survives_cancelled_upload_and_reclaim_without_retaining_the_decoded_frame() {
    let frame = yuv(ChromaLayout::PlanarRows, 90, 1, 2);
    let original = frame.to_rgba().unwrap();
    let (w, h) = frame.display_size();
    let frame = decoded(VideoPixels::Yuv(frame), w, h);
    let mut cache = PreviewVideoCache::default();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let scene = video_scene(w, h);
    let target = renderer.capture_target(w, h).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let token = cache
        .upload(&mut renderer, 1, 1, &frame, Some(3), false, &mut encoder)
        .unwrap()
        .unwrap();
    assert!(renderer.reclaim_preview_working_set().is_err());
    drop(encoder);
    drop(token);
    renderer.reclaim_preview_working_set().unwrap();
    let bytes = cache.bytes();
    assert!(bytes < frame.bytes());
    assert_eq!(Arc::strong_count(&frame), 1);
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let token = cache
        .upload(&mut renderer, 1, 1, &frame, Some(3), false, &mut encoder)
        .unwrap()
        .unwrap();
    renderer
        .encode_preview(&scene, &target.view, w, h, &mut encoder, None)
        .unwrap();
    renderer.queue.submit(Some(encoder.finish()));
    token.mark_submitted();
    let actual = renderer.read_target(&target).unwrap();
    assert!(
        actual.chunks_exact(4).all(|p| p
            .iter()
            .zip(&original[..4])
            .all(|(a, b)| a.abs_diff(*b) <= 2)),
        "video proxy pixels differ: adapter={:?}, size={w}x{h}, expected={:?}, first_bad_pixels={:?}",
        renderer.adapter_info,
        &original[..4],
        actual
            .chunks_exact(4)
            .enumerate()
            .filter(|(_, p)| p
                .iter()
                .zip(&original[..4])
                .any(|(a, b)| a.abs_diff(*b) > 2))
            .take(8)
            .collect::<Vec<_>>()
    );
    let uploads = renderer.video_uploads;
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(
        cache
            .upload(&mut renderer, 1, 1, &frame, Some(3), false, &mut encoder)
            .unwrap()
            .is_none()
    );
    assert_eq!(uploads, renderer.video_uploads);
    assert_eq!(bytes, cache.bytes());
    drop(frame);
    cache.retain(&Scene::new(&Project::new(w, h, 30, 60).unwrap()).unwrap());
    assert_eq!(cache.bytes(), 0);
}
#[test]
fn rgba_preview_is_bounded_and_bad_pixels_are_not_a_memory_refusal() {
    let mut cache = PreviewVideoCache::default();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let frame = decoded(VideoPixels::Rgba([80, 100, 120, 255].repeat(35)), 7, 5);
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    cache
        .upload(&mut renderer, 1, 1, &frame, Some(3), false, &mut encoder)
        .unwrap();
    assert_eq!(cache.bytes(), 3 * 3 * 4);
    assert_eq!(Arc::strong_count(&frame), 1);
    let broken = decoded(VideoPixels::Rgba(vec![]), 7, 5);
    assert!(
        cache
            .upload(&mut renderer, 1, 1, &broken, Some(3), false, &mut encoder)
            .err()
            .unwrap()
            .resource_rejection()
            .is_none()
    );
    assert_eq!(cache.bytes(), 0);
}
