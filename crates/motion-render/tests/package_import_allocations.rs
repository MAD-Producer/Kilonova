//! Verify actual owned texture identities independently of aggregate accounting.
use motion_effects::{
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
    AlphaMode, Registry, WorkingSpace,
};
use motion_render::{Renderer, Scene, TextureCategory};
#[path = "support/import_fixture.rs"]
mod fixture;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_package_imports_keep_one_actual_texture_identity_across_numeric_updates() {
    let mut rows = Vec::new();
    for private in [false, true] {
        let package = fixture::package(private);
        let mut project = fixture::project(&package, ChannelPrecision::Float16);
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut renderer = pollster::block_on(Renderer::new(
            &instance,
            None,
            wgpu::TextureFormat::Rgba16Float,
        ))
        .unwrap();
        let mut registry = Registry::new_with_builtins().unwrap();
        registry.insert(package.clone()).unwrap();
        renderer.set_effect_registry(registry);
        let [w, h] = fixture::SIZE;
        let contract = ImageContract::new(
            ImageMeaning::Color {
                space: WorkingSpace::Linear,
                alpha: AlphaMode::Premultiplied,
            },
            ChannelPrecision::Float16,
        );
        let target = renderer.image_capture_target(w, h, contract).unwrap();
        let sample = |p: &motion_core::Project| {
            let mut s = Scene::new(p).unwrap();
            s.sample(p, 0., None, &motion_core::ExpressionEvaluator)
                .unwrap();
            s
        };
        renderer
            .draw(&sample(&project), target.view(), w, h)
            .unwrap();
        let first = renderer.read_image_target(&target).unwrap().pixels;
        let resources = |r: &Renderer| {
            r.texture_resource_ledger()
                .resources()
                .filter(|image| {
                    image
                        .references
                        .iter()
                        .any(|owner| owner.category == TextureCategory::PluginResources)
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let before = resources(&renderer);
        assert_eq!(
            before.len(),
            1,
            "two effects retain one actual package texture"
        );
        assert_eq!(
            [before[0].width, before[0].height],
            if private { [256, 1] } else { [3, 2] }
        );
        assert_eq!(before[0].format, "Rgba8Unorm");
        assert_eq!(
            before[0].logical_bytes,
            Some(if private { 1024 } else { 24 })
        );
        let builds = renderer.effect_pipeline_builds();
        project.layers[0].effects[0]
            .params
            .get_mut("edge")
            .unwrap()
            .track
            .value[0] = 3.;
        renderer
            .draw(&sample(&project), target.view(), w, h)
            .unwrap();
        assert_eq!(renderer.read_image_target(&target).unwrap().pixels, first);
        let after = resources(&renderer);
        assert_eq!(
            after, before,
            "numeric changes preserve the actual backing allocation identity"
        );
        assert_eq!(renderer.effect_pipeline_builds(), builds);
        rows.push(serde_json::json!({"private":private,"resource":before[0],"numeric_identity_preserved":true,"pixels_preserved":true,"native_pipeline_builds":builds,"adapter":format!("{:?}",renderer.adapter_info)}));
    }
    if let Some(dir) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::PathBuf::from(dir).join("native-import-allocation-identity.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
