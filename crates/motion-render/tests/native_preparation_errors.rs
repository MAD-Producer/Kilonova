//! Real producer failures, retained native identity, and explicit GPU recovery.
use motion_core::{ExpressionEvaluator, Layer, Project};
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, WorkingSpace,
};
use motion_model::tracking::{lens::LensWarp, plate};
use motion_render::{ImageCaptureTarget, LensOutputPipeline, RenderError, Renderer, Scene};
use std::error::Error;

fn native<'a, T: Error + 'static>(error: &'a (dyn Error + 'static)) -> Option<&'a T> {
    if let Some(value) = error.downcast_ref::<T>() {
        Some(value)
    } else {
        error.source().and_then(native::<T>)
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_preparation_plane_failure_keeps_model_cause_and_previous_pixels_then_recovers() {
    let mut project = Project::new(32, 16, 30, 60).unwrap();
    let mut layer = Layer::solid(7, "Plane", [20., 10.], [16., 8., 0.], [1., 0., 0., 1.]);
    layer.three_d = true;
    project.layers.push(layer);
    let project_before = serde_json::to_vec(&project).unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(32, 16).unwrap();
    let baseline = renderer.capture(&scene, &target).unwrap().0;
    assert_eq!(baseline.len(), 32 * 16 * 4);
    assert!(baseline.chunks_exact(4).any(|p| p == [255, 0, 0, 255]));
    let saved_projection = scene.layers[0].view_projection;
    scene.layers[0].view_projection = glam::Mat4::ZERO;

    // Capture preflight already rejects this scene with a native Model error.
    // Exercise the independent public draw path containing the conversion.
    let failure = renderer.draw(&scene, &target.view, 32, 16).unwrap_err();
    let model = native::<motion_model::Error>(&failure).expect("original model cause");
    assert!(matches!(model, motion_model::Error::Invalid(message)
        if message == "layer 7 phase=exposure_projection: nonfinite or singular camera"));
    let RenderError::Diagnostic { cause, .. } = &failure else {
        panic!("plain compositor must retain its diagnostic boundary: {failure:?}");
    };
    let RenderError::Model(held) = cause.as_ref() else {
        panic!("original model object must be retained: {failure:?}");
    };
    assert!(std::ptr::eq(model, held));
    assert_eq!(failure.to_string(), format!("GPU validation: {model}"));
    assert!(failure.resource_rejection().is_none());
    assert_eq!(renderer.read_target(&target).unwrap(), baseline);
    assert_eq!(scene.layers[0].view_projection, glam::Mat4::ZERO);
    assert_eq!(serde_json::to_vec(&project).unwrap(), project_before);
    renderer.check_health().unwrap();

    // Recovery is a new caller action, not an implicit repair or retry.
    scene.layers[0].view_projection = saved_projection;
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, baseline);
    renderer.check_health().unwrap();
}

fn color_contract() -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        ChannelPrecision::Float16,
    )
}

fn clear(renderer: &Renderer, target: &ImageCaptureTarget, color: wgpu::Color) {
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("native preparation independent pixel baseline"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(color),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    }));
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(target.texture_resource_ledger());
        })
        .unwrap();
}

fn lens(crop: [f64; 4]) -> LensWarp {
    let extent = plate::Extent::new(16, 8).unwrap();
    LensWarp::new(
        plate::Calibration::new(extent, [1.; 2], [0.; 2], [0.; 3], [0.; 2]).unwrap(),
        plate::PlateTransform::new(extent, crop, plate::Rotation::None, extent).unwrap(),
        1.,
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_preparation_lens_failure_keeps_plate_cause_and_previous_pixels_then_recovers() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer
        .image_capture_target(16, 8, color_contract())
        .unwrap();
    let destination = renderer
        .image_capture_target(16, 8, color_contract())
        .unwrap();
    clear(
        &renderer,
        &source,
        wgpu::Color {
            r: 0.5,
            g: 0.25,
            b: 0.125,
            a: 1.,
        },
    );
    clear(&renderer, &destination, wgpu::Color::BLUE);
    let source_pixels = renderer.read_image_target(&source).unwrap().pixels;
    let baseline = renderer.read_image_target(&destination).unwrap().pixels;
    assert_eq!(baseline.len(), 16 * 8 * 8);
    assert_ne!(baseline, source_pixels);
    let pipeline = LensOutputPipeline::new(&renderer, wgpu::TextureFormat::Rgba16Float).unwrap();
    // Public constructors accept a finite positive subnormal crop. The actual
    // forward projection overflows in uniform preparation before any pass starts.
    let malformed = lens([0., 0., 1e-320, 1e-320]);
    let saved = malformed;
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    let budget_before = renderer.allocation_budget().info().unwrap();
    let failure = pipeline
        .encode_images(
            &renderer,
            &mut encoder,
            malformed,
            malformed.raster(1.).unwrap(),
            &source,
            &destination,
        )
        .unwrap_err();
    let cause = native::<plate::Error>(&failure).expect("original plate projection cause");
    assert_eq!(*cause, plate::Error::NonFiniteProjection);
    let RenderError::TrackingPlate(held) = &failure else {
        panic!("original plate object must be retained: {failure:?}");
    };
    assert!(std::ptr::eq(cause, held));
    assert_eq!(failure.to_string(), format!("GPU validation: {cause}"));
    assert!(failure.resource_rejection().is_none());
    assert_eq!(renderer.allocation_budget().info().unwrap(), budget_before);
    assert_eq!(malformed, saved);
    // Submitting the failed call's encoder must not clear or partially draw.
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    assert_eq!(
        renderer.read_image_target(&destination).unwrap().pixels,
        baseline
    );
    assert_eq!(
        renderer.read_image_target(&source).unwrap().pixels,
        source_pixels
    );
    renderer.check_health().unwrap();

    let healthy = lens([0., 0., 16., 8.]);
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    pipeline
        .encode_images(
            &renderer,
            &mut encoder,
            healthy,
            healthy.raster(1.).unwrap(),
            &source,
            &destination,
        )
        .unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            ledger.merge(source.texture_resource_ledger());
            ledger.merge(destination.texture_resource_ledger());
        })
        .unwrap();
    assert_eq!(
        renderer.read_image_target(&destination).unwrap().pixels,
        source_pixels
    );
    renderer.check_health().unwrap();
}
