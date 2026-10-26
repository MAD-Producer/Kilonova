//! Independently authored numeric PNG and f32 byte-container fixtures.
use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{
    image_contract::ChannelPrecision, AlphaMode, BoundsExpr, EffectPackage, PassDefinition,
    WorkingSpace,
};
use std::sync::Arc;

pub const SIZE: [u32; 2] = [30, 24];
pub const PIXELS: [[u8; 4]; 6] = [
    [17, 43, 91, 255],
    [201, 81, 23, 255],
    [63, 151, 217, 255],
    [235, 19, 107, 255],
    [49, 187, 131, 255],
    [123, 71, 241, 255],
];
pub fn package(private: bool) -> Arc<EffectPackage> {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("shaders")).unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut def = manifest
        .effects
        .iter()
        .find(|d| d.id == "glow")
        .unwrap()
        .clone();
    manifest.id = if private {
        "test.numeric-byte-import"
    } else {
        "test.numeric-png-import"
    }
    .into();
    def.id = "pixels".into();
    def.editor = None;
    def.native_editor = None;
    def.execution = None;
    def.working_space = WorkingSpace::Linear;
    def.alpha_mode = AlphaMode::Premultiplied;
    def.padding = BoundsExpr::Constant { value: 0. };
    def.output_bounds = None;
    def.required_capabilities.clear();
    def.resources = vec!["assets/pixels.png".into()];
    def.shader_data = None;
    let source = if private {
        def.required_capabilities
            .push(motion_effects::shader_data::SHADER_DATA_CAPABILITY.into());
        def.shader_data = Some(motion_effects::shader_data::ShaderData {
            version: motion_effects::shader_data::SHADER_DATA_VERSION,
            uniforms: vec![],
            overflow: Some(motion_effects::shader_data::ShaderDataTexture {
                path: def.resources[0].clone(),
                scalars: 1,
            }),
        });
        image::save_buffer(
            root.path().join(&def.resources[0]),
            &0.5_f32.to_le_bytes().repeat(256),
            256,
            1,
            image::ColorType::Rgba8,
        )
        .unwrap();
        "fn main_fx(p:vec2<f32>)->vec4<f32>{let b=vec4<u32>(round(textureLoad(resource0,vec2(0,0),0)*255.0));let v=bitcast<f32>(b.x|(b.y<<8u)|(b.z<<16u)|(b.w<<24u));return vec4(v,v,v,1.0);}"
    } else {
        image::save_buffer(
            root.path().join(&def.resources[0]),
            bytemuck::cast_slice(&PIXELS),
            3,
            2,
            image::ColorType::Rgba8,
        )
        .unwrap();
        "fn main_fx(p:vec2<f32>)->vec4<f32>{return textureLoad(resource0,vec2(i32(p.x)%3,i32(p.y)%2),0);}"
    };
    std::fs::write(root.path().join("shaders/pixels.wgsl"), source).unwrap();
    def.passes = vec![PassDefinition {
        shader: "shaders/pixels.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.effects = vec![def];
    std::fs::write(
        root.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    Arc::new(
        EffectPackage::from_bytes(motion_effects::package_directory(root.path()).unwrap()).unwrap(),
    )
}
pub fn project(package: &EffectPackage, precision: ChannelPrecision) -> Project {
    let mut p = Project::new(SIZE[0], SIZE[1], 30, 60).unwrap();
    p.working_precision = precision;
    p.background = [0.; 4];
    let mut layer = Layer::solid(
        47,
        "resource-local numeric oracle",
        [SIZE[0] as f32, SIZE[1] as f32],
        [SIZE[0] as f32 / 2., SIZE[1] as f32 / 2., 0.],
        [1.; 4],
    );
    for id in [11, 12] {
        layer.effects.push(EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            &package.manifest.effects[0],
            layer.size,
        ));
    }
    p.layers.push(layer);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}
