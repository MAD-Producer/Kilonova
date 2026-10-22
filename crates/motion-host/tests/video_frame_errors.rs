use motion_host::video_frame::{DecodedFrame, PreparedVideoFrame, VideoPixels};
use motion_render::{AllocationBudget, ChromaLayout, RenderError, Renderer, Yuv420Frame};
use std::{any::Any, sync::Arc};

fn frame(pixels: VideoPixels) -> Arc<DecodedFrame> {
    Arc::new(DecodedFrame {
        pixels,
        pts: 100,
        end: 200,
        width: 2,
        height: 2,
        decode_us: 0,
        codec_us: 0,
        transfer_us: 0,
        pack_us: 0,
        source_transfer: "authored-fixture",
        decoder_name: "fixture".into(),
    })
}

// Any lets this regression execute against the former text-returning API too.
fn render_cause(error: &dyn Any) -> &RenderError {
    let Some(motion_host::Error::Render(cause)) = error.downcast_ref::<motion_host::Error>() else {
        panic!("native host/render error identity was lost");
    };
    cause
}

#[test]
fn decoded_upload_and_encoder_keep_real_allocation_refusals() {
    let budget = AllocationBudget::new(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes).unwrap();
    let pixels = frame(VideoPixels::Rgba([20, 40, 60, 255].repeat(4)));
    let upload = pixels.upload(&mut renderer, 71, 83).unwrap_err();
    assert!(render_cause(&upload).resource_rejection().is_some());
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    let encoded = match pixels.encode_upload(&mut renderer, 71, 83, &mut encoder) {
        Err(error) => error,
        Ok(_) => panic!("zero remaining capacity must refuse video upload"),
    };
    assert!(render_cause(&encoded).resource_rejection().is_some());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(renderer.video_uploads, 0);
    drop(encoder);
    budget
        .set_limit(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET)
        .unwrap();
    pixels.upload(&mut renderer, 71, 83).unwrap();
    assert_eq!(renderer.video_uploads, 1);
    assert_eq!(pixels.rgba().unwrap().as_ref(), [20, 40, 60, 255].repeat(4));
}

#[test]
fn prepared_upload_preserves_the_required_neighbor_refusal() {
    let budget = AllocationBudget::new(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET).unwrap();
    let mut renderer =
        pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
    let before = budget.info().unwrap();
    budget.set_limit(before.admitted_bytes).unwrap();
    let left = frame(VideoPixels::Rgba([0, 0, 0, 255].repeat(4)));
    let mut right = frame(VideoPixels::Rgba(vec![255; 16]));
    Arc::get_mut(&mut right).unwrap().pts = 200;
    let prepared = PreparedVideoFrame {
        left,
        next: Some((right, 0.5)),
    };
    let error = prepared.upload(&mut renderer, 71, 83).unwrap_err();
    assert!(render_cause(&error).resource_rejection().is_some());
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget
        .set_limit(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET)
        .unwrap();
    prepared.upload(&mut renderer, 71, 83).unwrap();
    assert_eq!(
        &prepared.decoded().unwrap().rgba().unwrap()[..4],
        &[188, 188, 188, 255]
    );
}

#[test]
fn cpu_conversion_keeps_native_invalid_planes_and_healthy_borrows() {
    let invalid = frame(VideoPixels::Yuv(Yuv420Frame {
        width: 2,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0; 2],
        y: vec![16; 4],
        uv: vec![],
        chroma_layout: ChromaLayout::Uv,
    }));
    let error = invalid.rgba().unwrap_err();
    assert!(
        matches!(render_cause(&error), RenderError::Invalid(message) if message == "YUV frame plane length mismatch")
    );
    let rgba = frame(VideoPixels::Rgba([11, 22, 33, 255].repeat(4)));
    assert!(matches!(
        rgba.rgba().unwrap(),
        std::borrow::Cow::Borrowed(_)
    ));
    assert_eq!(rgba.rgba().unwrap().as_ref(), [11, 22, 33, 255].repeat(4));
}
