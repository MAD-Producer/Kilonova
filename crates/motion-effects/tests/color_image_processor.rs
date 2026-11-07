use motion_effects::{color_config::*, color_processor::*};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Deserialize)]
struct Reference {
    configs: Vec<Config>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Config {
    manifest: ColorConfigManifest,
    config: String,
    selection: ColorSelection,
}
#[derive(Deserialize)]
struct Case {
    config: usize,
    processor: ColorProcessorSource,
}
#[test]
fn all_explicit_ocio_programs_prepare_direct_image_shaders() {
    let reference: Reference =
        serde_json::from_str(include_str!("color_processor/reference.json")).unwrap();
    let configs: Vec<_> = reference
        .configs
        .into_iter()
        .map(|c| {
            (
                VerifiedColorConfig::new(
                    c.manifest,
                    Arc::from(c.config.as_bytes()),
                    BTreeMap::new(),
                )
                .unwrap(),
                c.selection,
            )
        })
        .collect();
    let mut keys = BTreeSet::new();
    for c in reference.cases {
        let (config, selection) = &configs[c.config];
        let compute = c.processor.compile(config, selection).unwrap();
        let image = c.processor.compile_image(config, selection).unwrap();
        assert_eq!(image.input_alpha(), c.processor.input_alpha);
        assert_eq!(image.output_alpha(), c.processor.output_alpha);
        assert_eq!(image.phase(), c.processor.phase);
        assert_eq!(image.tables(), compute.tables());
        assert_ne!(image.key(), compute.key());
        assert!(keys.insert(image.key()));
        let module = naga::front::wgsl::parse_str(image.wgsl()).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
        assert_eq!(module.entry_points.len(), 2);
        assert!(module
            .entry_points
            .iter()
            .any(|e| e.stage == naga::ShaderStage::Fragment));
        assert!(module
            .entry_points
            .iter()
            .any(|e| e.stage == naga::ShaderStage::Vertex && e.name == "motion_image_vertex"));
        assert!(image.wgsl().contains("textureLoad"));
        assert!(!image.wgsl().contains("textureSample"));
        assert!(!image.wgsl().contains("@compute"));
    }
    assert_eq!(keys.len(), 20);
}
