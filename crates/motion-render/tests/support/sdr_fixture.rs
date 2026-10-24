//! Independently authored HDR/transparent source shared by native output probes.
use motion_core::{EffectInstance, Layer, Project, render_sampling::Shutter};
use motion_effects::{
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::ChannelPrecision,
};
use std::sync::Arc;

pub fn project(precision: ChannelPrecision, exposure: bool) -> (Project, Registry) {
    project_sized(precision, exposure, 18)
}
pub fn project_sized(
    precision: ChannelPrecision,
    exposure: bool,
    size: u32,
) -> (Project, Registry) {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "tint")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.passes = vec![PassDefinition {
        shader: "shaders/output.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.frozen-sdr".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("shaders/output.wgsl"),
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return vec4<f32>(2.0,-0.125,0.125,0.25);}",
    )
    .unwrap();
    std::fs::write(
        directory.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    let package = Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(directory.path()).unwrap())
            .unwrap(),
    );
    let mut project = Project::new(size, size, 30, 6).unwrap();
    project.working_precision = precision;
    project.background = [0.; 4];
    let mut layer = Layer::solid(
        1,
        "Independent SDR output probe",
        [size as f32 * 2.; 2],
        [size as f32 / 2., size as f32 / 2., 0.],
        [1.; 4],
    );
    layer.effects.push(EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &manifest.effects[0],
        layer.size,
    ));
    if exposure {
        layer.sampling.motion_blur = Some(Shutter {
            frames: 0.5,
            samples: 4,
        });
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (project, registry)
}
pub fn assert_png(pixels: &[u8]) {
    assert_eq!(pixels.len(), 18 * 18 * 4);
    // Clip unpremultiplied linear (8, -0.5, 0.5), encode sRGB exactly once,
    // retain alpha=0.25. The UNORM8 working control has one prior quantization.
    for pixel in pixels.chunks_exact(4) {
        for (actual, expected) in pixel.iter().zip([255u8, 0, 188, 64]) {
            assert!(actual.abs_diff(expected) <= 2, "pixel={pixel:?}");
        }
    }
}
