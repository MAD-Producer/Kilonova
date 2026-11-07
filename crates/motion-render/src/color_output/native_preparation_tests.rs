use super::*;
use motion_effects::image_contract::ImageFormatError;
use std::error::Error;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_preparation_contract_failure_keeps_native_cause_before_allocation_then_recovers() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let source = renderer.render_target(16, 8).unwrap();
    let descriptor = ColorOutputDescriptor {
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        alpha: AlphaMode::Premultiplied,
        transform: OutputTransform::SDR_CLIP,
    };
    let healthy = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        ChannelPrecision::Unorm8,
    );
    let mut raw = serde_json::to_value(healthy).unwrap();
    raw["version"] = 999.into();
    let wrong_version: ImageContract = serde_json::from_value(raw.clone()).unwrap();
    let wrong_precision =
        ImageContract::new(ImageMeaning::LinearDepth {}, ChannelPrecision::Unorm8);
    for contract in [wrong_version, wrong_precision] {
        let expected = contract.preferred_storage().unwrap_err();
        let before = renderer.allocation_budget().info().unwrap();
        // Public targets already validate contracts. Call the real private
        // converter boundary, without forging a target or mocking its result.
        let failure =
            match ColorOutput::new(&renderer, source.texture(), contract, descriptor, false) {
                Err(failure) => failure,
                Ok(_) => panic!("invalid input contract must fail"),
            };
        let RenderError::Diagnostic { cause, .. } = &failure else {
            panic!("original contract object must be retained: {failure:?}");
        };
        let RenderError::ImageContract(held) = cause.as_ref() else {
            panic!("native contract variant required: {failure:?}");
        };
        let native = failure
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<ImageFormatError>()
            .expect("native contract source");
        assert!(std::ptr::eq(native, held));
        assert_eq!(native, &expected);
        assert_eq!(
            failure.to_string(),
            format!("GPU validation: color output input: {expected}")
        );
        assert!(failure.resource_rejection().is_none());
        assert_eq!(renderer.allocation_budget().info().unwrap(), before);
        assert_eq!(contract.preferred_storage().unwrap_err(), expected);
        renderer.check_health().unwrap();
    }
    assert_eq!(serde_json::to_value(wrong_version).unwrap(), raw);
    let conversion =
        ColorOutput::new(&renderer, source.texture(), healthy, descriptor, false).unwrap();
    assert_eq!(conversion.descriptor, descriptor);
    renderer.check_health().unwrap();
}
