use super::*;
use motion_effects::{color_lut::LutPurpose, WorkingSpace};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_lut_shader_failure_retains_native_cause_and_releases_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Straight,
        },
        ChannelPrecision::Float32,
    );
    let source = renderer.image_capture_target(2, 2, contract).unwrap();
    let lut = ColorLut::from_cube(
        "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n",
        LutInterpolation::Linear,
        LutPurpose::Look,
    )
    .unwrap();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let failure = match LutOutput::create(&renderer, &source, &lut, contract, "this is not WGSL") {
        Err(error) => error,
        Ok(_) => panic!("invalid shader must not produce a LUT binding"),
    };
    let RenderError::LutPipeline(native) = &failure else {
        panic!("{failure:?}")
    };
    assert!(matches!(native.native(), wgpu::Error::Validation { .. }));
    assert!(std::ptr::eq(
        native
            .source()
            .unwrap()
            .downcast_ref::<wgpu::Error>()
            .unwrap(),
        native.native()
    ));
    assert!(native.admission_error().is_none());
    assert!(failure.to_string().contains("color LUT phase=create"));
    assert!(failure.to_string().contains("WGSL"));
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    renderer.check_health().unwrap();
    let healthy = LutOutput::new(&renderer, &source, &lut, contract).unwrap();
    assert_eq!(healthy.key(), lut.key());
    drop(healthy);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
}
