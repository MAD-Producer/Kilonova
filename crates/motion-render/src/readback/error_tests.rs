use super::*;
use crate::{ReadbackError, RenderError, Renderer};
use std::error::Error;

fn mapping(error: &RenderError, context: &str) {
    let RenderError::NativeReadback(ReadbackError::Mapping {
        context: actual,
        cause,
    }) = error
    else {
        panic!("readback native mapping cause lost: {error:?}");
    };
    assert_eq!(*actual, context);
    assert_eq!(
        error.to_string(),
        format!("GPU readback failed: {context}{cause}")
    );
    let native = error
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<wgpu::BufferAsyncError>()
        .unwrap();
    assert!(std::ptr::eq(native, cause));
    assert!(error.resource_rejection().is_none());
}

fn balanced_failure(renderer: &Renderer, before: crate::ImageReadbackInfo) {
    let failed = renderer.image_readback_info();
    assert_eq!(failed.failed_calls, before.failed_calls + 1);
    assert_eq!(failed.completed_calls, before.completed_calls);
    assert_eq!(failed.in_flight_calls, 0);
    renderer.check_health().unwrap();
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn png_readback_keeps_real_cancelled_mapping_cause_and_recovers_all_pixels() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let project = motion_model::Project::new(16, 8, 30, 60).unwrap();
    let scene = crate::Scene::new(&project).unwrap();
    let target = renderer.capture_target(16, 8).unwrap();
    let expected = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(expected.len(), 16 * 8 * 4);
    let before = renderer.image_readback_info();
    cancel_next_map();
    let error = renderer.read_target(&target).unwrap_err();
    mapping(&error, "");
    balanced_failure(&renderer, before);
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    assert_eq!(
        renderer.image_readback_info().completed_calls,
        before.completed_calls + 1
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_image_readback_keeps_real_cancelled_mapping_cause_and_recovers_all_bytes() {
    use motion_effects::image_contract::{ChannelPrecision, ImageContract, ImageMeaning};
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: motion_effects::WorkingSpace::Linear,
            alpha: motion_effects::AlphaMode::Premultiplied,
        },
        ChannelPrecision::Float16,
    );
    let target = renderer.image_capture_target(16, 8, contract).unwrap();
    let expected = renderer.read_image_target(&target).unwrap();
    assert_eq!(expected.pixels, vec![0; 16 * 8 * 8]);
    let before = renderer.image_readback_info();
    cancel_next_map();
    let error = renderer.read_image_target(&target).unwrap_err();
    mapping(&error, "image capture mapping: ");
    balanced_failure(&renderer, before);
    let recovered = renderer.read_image_target(&target).unwrap();
    assert_eq!(recovered.contract, expected.contract);
    assert_eq!(recovered.storage, expected.storage);
    assert_eq!(recovered.pixels, expected.pixels);
    assert_eq!(
        renderer.image_readback_info().completed_calls,
        before.completed_calls + 1
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn preview_pick_keeps_real_cancelled_mapping_cause_and_explicitly_recovers() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.render_target(16, 8).unwrap();
    let expected = renderer.sample_preview_pixel(&target, [7, 3]).unwrap();
    assert_eq!(expected, [0.; 4]);
    let before = renderer.image_readback_info();
    cancel_next_map();
    let error = renderer.sample_preview_pixel(&target, [7, 3]).unwrap_err();
    mapping(&error, "color pick mapping: ");
    balanced_failure(&renderer, before);
    assert_eq!(
        renderer.sample_preview_pixel(&target, [7, 3]).unwrap(),
        expected
    );
    assert_eq!(
        renderer.image_readback_info().completed_calls,
        before.completed_calls + 1
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn callback_disconnect_and_unmapped_view_retain_actual_native_sources() {
    let (send, receive) = std::sync::mpsc::channel::<()>();
    drop(send);
    let cause = receive.recv().unwrap_err();
    let expected = format!("GPU readback failed: image capture callback: {cause}");
    let error = RenderError::from(ReadbackError::Callback {
        context: "image capture callback: ",
        cause,
    });
    assert_eq!(error.to_string(), expected);
    assert!(
        error
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<std::sync::mpsc::RecvError>()
    );
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("native unmapped readback error"),
        size: 256,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let cause = buffer
        .slice(..)
        .get_mapped_range()
        .err()
        .expect("unmapped view must fail");
    let expected = format!("GPU readback failed: color pick mapped range: {cause}");
    let error = RenderError::from(ReadbackError::Range {
        context: "color pick mapped range: ",
        cause,
    });
    assert_eq!(error.to_string(), expected);
    let RenderError::NativeReadback(ReadbackError::Range { cause, .. }) = &error else {
        panic!("range cause lost");
    };
    let native = error
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<wgpu::MapRangeError>()
        .unwrap();
    assert!(std::ptr::eq(native, cause));
    renderer.check_health().unwrap();
}
