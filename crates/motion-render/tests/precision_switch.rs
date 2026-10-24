use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
};
use motion_render::{Renderer, Scene, image_formats};
use std::sync::Arc;
const SIZE: u32 = 17;
fn fixture(body: &str) -> (Project, Registry) {
    let directory = tempfile::tempdir().unwrap();
    let mut manifest = motion_effects::builtin::manifest();
    let mut definition = manifest
        .effects
        .iter()
        .find(|d| d.id == "curves")
        .unwrap()
        .clone();
    definition.working_space = WorkingSpace::Linear;
    definition.alpha_mode = AlphaMode::Premultiplied;
    definition.resources.clear();
    definition.passes = vec![PassDefinition {
        shader: "shaders/curve.wgsl".into(),
        entry: "main_fx".into(),
    }];
    manifest.id = "com.motionstudio.tests.curve-precision".into();
    manifest.effects = vec![definition];
    std::fs::create_dir(directory.path().join("shaders")).unwrap();
    std::fs::write(
        directory.path().join("shaders/curve.wgsl"),
        format!("fn main_fx(p: vec2<f32>) -> vec4<f32> {{ {body} }}"),
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
    let mut project = Project::new(SIZE, SIZE, 30, 60).unwrap();
    let mut layer = Layer::solid(
        1,
        "curve probe",
        [SIZE as f32 * 2.; 2],
        [SIZE as f32 / 2., SIZE as f32 / 2., 0.],
        [1.; 4],
    );
    layer.effects.push(EffectInstance::new(
        1,
        &manifest.id,
        &manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        layer.size,
    ));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    let mut registry = Registry::new_with_builtins().unwrap();
    registry.insert(package).unwrap();
    (project, registry)
}
fn renderer(budget: motion_render::AllocationBudget) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        budget,
    ))
    .unwrap()
}
fn pixels(renderer: &mut Renderer, project: &Project) -> Vec<u8> {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let target = renderer
        .image_capture_target(
            SIZE,
            SIZE,
            ImageContract::new(
                ImageMeaning::Color {
                    space: WorkingSpace::Linear,
                    alpha: AlphaMode::Premultiplied,
                },
                renderer.working_precision(),
            ),
        )
        .unwrap();
    renderer.draw(&scene, target.view(), SIZE, SIZE).unwrap();
    renderer.read_image_target(&target).unwrap().pixels
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn same_device_precision_changes_match_fresh_native_images_and_preserve_custom_effects() {
    let (mut project, registry) = fixture("return curve_lookup(vec4(0.5001,0.25,0.75,1.0));");
    let mut vector = Layer::solid(2, "Working-format vector", [8.; 2], [8.5, 8.5, 0.], [1.; 4]);
    vector.content = motion_core::Content::Vector {
        vector: motion_core::vector::VectorContent::shape(motion_core::vector::ShapeKind::Ellipse),
    };
    vector.transform.opacity.value = 50.001;
    project.layers.push(vector);
    let mut live = renderer(Default::default());
    live.set_effect_registry(registry.clone());
    let device = live.device.clone();
    let queue = live.queue.clone();
    for precision in [
        ChannelPrecision::Float16,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float32,
        ChannelPrecision::Unorm8,
        ChannelPrecision::Float32,
    ] {
        let change = live.prepare_working_precision(precision).unwrap().unwrap();
        live.commit_working_precision(change).unwrap();
        assert_eq!(live.device, device);
        assert_eq!(live.queue, queue);
        assert_eq!(live.working_precision(), precision);
        assert_eq!(
            live.target_format,
            image_formats::native_format(image_formats::color_storage(precision))
        );
        project.working_precision = precision;
        let actual = pixels(&mut live, &project);
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let mut fresh =
            pollster::block_on(Renderer::new(&instance, None, live.target_format)).unwrap();
        fresh.set_effect_registry(registry.clone());
        assert_eq!(
            actual,
            pixels(&mut fresh, &project),
            "precision={precision:?}"
        );
        let before = live.allocation_budget().info().unwrap();
        assert!(live.prepare_working_precision(precision).unwrap().is_none());
        assert_eq!(
            live.allocation_budget().info().unwrap().admitted_bytes,
            before.admitted_bytes
        );
        assert_eq!(
            live.allocation_budget().info().unwrap().live_allocations,
            before.live_allocations
        );
        assert_eq!(actual, pixels(&mut live, &project));
        live.check_health().unwrap();
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn precision_preparation_admits_peak_capacity_and_refusal_preserves_old_pixels() {
    let (project, registry) = fixture("return curve_lookup(vec4(0.5,0.25,0.75,1.0));");
    let mut renderer = renderer(Default::default());
    renderer.set_effect_registry(registry);
    let expected = pixels(&mut renderer, &project);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let budget = renderer.allocation_budget().clone();
    let before = budget.info().unwrap();
    let prepared = renderer
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    let peak = budget.info().unwrap().admitted_bytes;
    assert!(peak > before.admitted_bytes);
    drop(prepared);
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before.admitted_bytes,
        "preparation must not queue uploads"
    );
    budget.set_limit(peak - 1).unwrap();
    let error = renderer
        .prepare_working_precision(ChannelPrecision::Float16)
        .err()
        .unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: peak,
            budget_bytes: peak - 1,
        })
    );
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    assert_eq!(renderer.working_precision(), ChannelPrecision::Unorm8);
    // Raise only for the verification readback; the rejected mode never committed.
    budget.set_limit(before.budget_bytes).unwrap();
    assert_eq!(expected, pixels(&mut renderer, &project));
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let prepared = renderer
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    budget.set_limit(peak - 1).unwrap();
    let error = renderer.commit_working_precision(prepared).unwrap_err();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: peak,
            budget_bytes: peak - 1,
        })
    );
    assert_eq!(renderer.working_precision(), ChannelPrecision::Unorm8);
    assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
    budget.set_limit(before.budget_bytes).unwrap();
    let prepared = renderer
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    renderer.commit_working_precision(prepared).unwrap();
    assert_eq!(renderer.working_precision(), ChannelPrecision::Float16);
    let actual = pixels(&mut renderer, &project);
    assert_eq!(actual.len(), SIZE as usize * SIZE as usize * 8);
    renderer.check_health().unwrap();
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn narrow_devices_foreign_owners_and_outdated_preparations_refuse_without_mutation() {
    let narrow = pollster::block_on(Renderer::headless()).unwrap();
    assert!(
        !narrow
            .device
            .features()
            .contains(wgpu::Features::FLOAT32_FILTERABLE)
    );
    let error = narrow
        .prepare_working_precision(ChannelPrecision::Float32)
        .err()
        .unwrap();
    assert!(error.to_string().contains("enabled-device"));
    assert_eq!(narrow.working_precision(), ChannelPrecision::Unorm8);
    narrow.check_health().unwrap();
    let budget = motion_render::AllocationBudget::new(128 << 20).unwrap();
    let mut first = renderer(budget.clone());
    let mut second = renderer(budget);
    let prepared = first
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    assert!(
        second
            .commit_working_precision(prepared)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    assert_eq!(second.working_precision(), ChannelPrecision::Unorm8);
    let stale = first
        .prepare_working_precision(ChannelPrecision::Float16)
        .unwrap()
        .unwrap();
    let current = first
        .prepare_working_precision(ChannelPrecision::Unorm16)
        .unwrap()
        .unwrap();
    first.commit_working_precision(current).unwrap();
    assert!(
        first
            .commit_working_precision(stale)
            .unwrap_err()
            .to_string()
            .contains("another mode")
    );
    assert_eq!(first.working_precision(), ChannelPrecision::Unorm16);
    first.check_health().unwrap();
    second.check_health().unwrap();
}
