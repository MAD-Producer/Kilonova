use super::*;
use motion_effects::{color_config::*, color_processor::ColorProcessorSource, WorkingSpace};
use std::collections::BTreeMap;
fn processor() -> CompiledColorImageProcessor {
    let r: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../motion-effects/tests/color_processor/reference.json"
    )))
    .unwrap();
    let case = r["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| !c["processor"]["tables"].as_array().unwrap().is_empty())
        .unwrap();
    let c = &r["configs"][case["config"].as_u64().unwrap() as usize];
    let config = VerifiedColorConfig::new(
        serde_json::from_value(c["manifest"].clone()).unwrap(),
        Arc::from(c["config"].as_str().unwrap().as_bytes()),
        BTreeMap::new(),
    )
    .unwrap();
    let source: ColorProcessorSource = serde_json::from_value(case["processor"].clone()).unwrap();
    source
        .compile_image(
            &config,
            &serde_json::from_value(c["selection"].clone()).unwrap(),
        )
        .unwrap()
}
fn contract(alpha: AlphaMode) -> ImageContract {
    ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha,
        },
        ChannelPrecision::Float32,
    )
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failed_image_shader_retains_native_cause_and_releases_table_admission() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let p = processor();
    let source = renderer
        .image_capture_target(3, 4, contract(p.input_alpha()))
        .unwrap();
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let error = match RawColorProcessorOutput::create(
        &renderer,
        &source,
        &p,
        contract(p.output_alpha()),
        "THIS_SECRET_SHADER_TOKEN is not WGSL",
    ) {
        Err(e) => e,
        Ok(_) => panic!("invalid shader must fail"),
    };
    let RenderError::ColorProcessorPipeline(native) = &error else {
        panic!("{error:?}")
    };
    assert!(matches!(native.native(), wgpu::Error::Validation { .. }));
    assert_eq!(native.key(), p.key());
    assert_eq!(native.phase(), p.phase());
    assert!(std::ptr::eq(
        native
            .source()
            .unwrap()
            .downcast_ref::<wgpu::Error>()
            .unwrap(),
        native.native()
    ));
    assert!(native
        .native()
        .to_string()
        .contains("THIS_SECRET_SHADER_TOKEN"));
    assert!(!error.to_string().contains("THIS_SECRET_SHADER_TOKEN"));
    assert!(native.admission_error().is_none());
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    renderer.check_health().unwrap();
    let healthy =
        RawColorProcessorOutput::new(&renderer, &source, &p, contract(p.output_alpha())).unwrap();
    assert_eq!(healthy.key(), p.key());
    drop(healthy);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn refused_image_tables_keep_typed_budget_reason_and_allow_recovery() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let p = processor();
    let source = renderer
        .image_capture_target(3, 4, contract(p.input_alpha()))
        .unwrap();
    let budget = renderer.allocation_budget();
    let before = budget.info().unwrap();
    assert!(!p.tables().is_empty());
    let first_bytes = p.tables()[0].len() as u64 * 16;
    budget
        .set_limit(before.admitted_bytes + first_bytes - 1)
        .unwrap();
    let error =
        match RawColorProcessorOutput::new(&renderer, &source, &p, contract(p.output_alpha())) {
            Err(e) => e,
            Ok(_) => panic!("table must exceed budget"),
        };
    assert!(error.resource_rejection().is_some(), "{error:?}");
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget.set_limit(before.budget_bytes).unwrap();
    let healthy =
        RawColorProcessorOutput::new(&renderer, &source, &p, contract(p.output_alpha())).unwrap();
    drop(healthy);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
}
