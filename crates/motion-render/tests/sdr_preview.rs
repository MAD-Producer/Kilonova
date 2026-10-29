use motion_core::{EffectInstance, Layer, Project};
use motion_effects::{
    AlphaMode, EffectPackage, PassDefinition, Registry, WorkingSpace,
    image_contract::ChannelPrecision,
};
use motion_render::{Renderer, Scene, SdrPreviewFrameTargets, TextureResourceLedger};
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
fn renderer(precision: ChannelPrecision) -> Renderer {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        false,
        Default::default(),
    ))
    .unwrap();
    if let Some(change) = renderer.prepare_working_precision(precision).unwrap() {
        renderer.commit_working_precision(change).unwrap();
    }
    renderer
}
fn scene(renderer: &mut Renderer) -> Scene {
    let (mut project, registry) = fixture("return vec4(0.125,0.0625,0.03125,0.25);");
    project.background = [0.; 4];
    renderer.set_effect_registry(registry);
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}
fn ledger(targets: &SdrPreviewFrameTargets) -> TextureResourceLedger {
    let mut ledger = TextureResourceLedger::default();
    targets.collect_texture_resources(&mut ledger);
    ledger
}
fn render(
    renderer: &mut Renderer,
    targets: &mut SdrPreviewFrameTargets,
    scene: &Scene,
    width: u32,
    height: u32,
) -> u32 {
    let view = targets
        .prepare(renderer, width, height)
        .unwrap()
        .view
        .clone();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode_preview(scene, &view, width, height, &mut encoder, None)
        .unwrap();
    let draws = targets.encode_output(renderer, &mut encoder).unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            targets.collect_texture_resources(ledger)
        })
        .unwrap();
    renderer.check_health().unwrap();
    targets.commit_submitted().unwrap();
    draws
}
fn expect_pixels(bytes: &[u8]) {
    for pixel in bytes.chunks_exact(4) {
        assert!(
            pixel
                .iter()
                .zip([99_u8, 71, 49, 64])
                .all(|(a, b)| a.abs_diff(b) <= 1),
            "{pixel:?}"
        );
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_working_front_snapshot_preserves_bytes_across_four_precisions() {
    fn bytes(renderer: &Renderer, target: &motion_render::RenderTarget) -> Vec<u8> {
        let texture = target.view.texture();
        let row = target.width * texture.format().block_copy_size(None).unwrap();
        let stride = row.div_ceil(256) * 256;
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("working snapshot test readback"), size: u64::from(stride * target.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false,
        });
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout {
                offset: 0, bytes_per_row: Some(stride), rows_per_image: Some(target.height) } },
            wgpu::Extent3d { width: target.width, height: target.height, depth_or_array_layers: 1 });
        renderer.submit_tracked(encoder.finish(), |_| {});
        let (send, receive) = std::sync::mpsc::channel();
        buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| { let _ = send.send(result); });
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let mapped = buffer.slice(..).get_mapped_range();
        let mut result = Vec::with_capacity((row * target.height) as usize);
        for y in 0..target.height { let begin = (y * stride) as usize; result.extend_from_slice(&mapped[begin..begin + row as usize]); }
        drop(mapped);
        buffer.unmap();
        result
    }
    for precision in [ChannelPrecision::Unorm8, ChannelPrecision::Unorm16, ChannelPrecision::Float16, ChannelPrecision::Float32] {
        let mut renderer = renderer(precision);
        let scene = scene(&mut renderer);
        let mut targets = SdrPreviewFrameTargets::new(&renderer, SIZE, SIZE).unwrap();
        assert!(targets.working_front().is_none());
        render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
        expect_pixels(&read_output(&renderer, &targets));
        let working = targets.working_front().unwrap();
        let format = motion_render::image_formats::native_format(motion_render::image_formats::color_storage(precision));
        assert_eq!(working.view.texture().format(), format);
        let snapshot = renderer.snapshot_target(working).unwrap();
        assert_eq!(snapshot.view.texture().format(), format);
        let expected = bytes(&renderer, working);
        assert_eq!(bytes(&renderer, &snapshot), expected, "working snapshot precision={precision:?}");
        if precision != ChannelPrecision::Unorm8 {
            assert_ne!(targets.view.texture().format(), format);
        }
        targets.invalidate();
        assert!(targets.working_front().is_none());
        assert_eq!(bytes(&renderer, &snapshot), expected);
        renderer.check_health().unwrap();
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn retained_sdr_fronts_budget_native_and_output_storage_and_reuse_both_slots() {
    for (precision, pixel_bytes) in [
        (ChannelPrecision::Unorm8, 4),
        (ChannelPrecision::Unorm16, 12),
        (ChannelPrecision::Float16, 12),
        (ChannelPrecision::Float32, 20),
    ] {
        let mut renderer = renderer(precision);
        let scene = scene(&mut renderer);
        let mut targets = SdrPreviewFrameTargets::new(&renderer, SIZE, SIZE).unwrap();
        assert_eq!(
            targets.texture_bytes(),
            u64::from(SIZE * SIZE) * pixel_bytes
        );
        assert!(!targets.valid());
        assert_eq!(
            render(&mut renderer, &mut targets, &scene, SIZE, SIZE),
            u32::from(precision != ChannelPrecision::Unorm8)
        );
        expect_pixels(&read_output(&renderer, &targets));
        assert!(targets.valid());
        assert_eq!(
            targets.view.texture().format(),
            wgpu::TextureFormat::Rgba8UnormSrgb
        );
        assert_eq!(
            targets.texture_bytes(),
            u64::from(SIZE * SIZE) * pixel_bytes * 2
        );
        let retained = ledger(&targets);
        assert_eq!(retained.total_known_bytes(), targets.texture_bytes());
        assert_eq!(
            retained.resources().count(),
            if precision == ChannelPrecision::Unorm8 {
                2
            } else {
                4
            }
        );
        let ids: Vec<_> = retained.resources().map(|r| r.id.clone()).collect();
        let before = renderer.allocation_budget().info().unwrap();
        let reads = renderer.image_readback_info();
        for _ in 0..4 {
            render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
        }
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(
            ledger(&targets)
                .resources()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            before.admitted_bytes
        );
        assert_eq!(
            renderer
                .allocation_budget()
                .info()
                .unwrap()
                .live_allocations,
            before.live_allocations
        );
        assert_eq!(
            renderer.image_readback_info().submitted_calls,
            reads.submitted_calls
        );
    }
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn preview_refusals_preserve_displayed_pixels_and_mode_or_extent_changes_replace_the_working_candidate()
 {
    let mut renderer = renderer(ChannelPrecision::Float16);
    let scene = scene(&mut renderer);
    let mut targets = SdrPreviewFrameTargets::new(&renderer, SIZE, SIZE).unwrap();
    render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
    let old = read_output(&renderer, &targets);
    let before = ledger(&targets);
    let mut policy = renderer.memory_policy();
    let original = policy;
    policy.preview_target_budget_bytes = targets.texture_bytes() - 1;
    renderer.set_memory_policy(policy).unwrap();
    let error = targets.prepare(&renderer, SIZE, SIZE).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::PreviewTarget,
            required_bytes: targets.texture_bytes(),
            budget_bytes: targets.texture_bytes() - 1,
        })
    );
    assert_eq!(ledger(&targets), before);
    assert_eq!(read_output(&renderer, &targets), old);
    renderer.set_memory_policy(original).unwrap();
    let change = renderer
        .prepare_working_precision(ChannelPrecision::Unorm16)
        .unwrap()
        .unwrap();
    renderer.commit_working_precision(change).unwrap();
    targets.prepare(&renderer, SIZE, SIZE).unwrap();
    assert_eq!(
        targets.prepared().unwrap().view.texture().format(),
        wgpu::TextureFormat::Rgba16Unorm
    );
    assert_eq!(read_output(&renderer, &targets), old);
    render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
    expect_pixels(&read_output(&renderer, &targets));
    let displayed = targets.view.texture().clone();
    targets.prepare(&renderer, 23, 11).unwrap();
    assert_eq!(targets.view.texture(), &displayed);
    assert_eq!(
        (
            targets.prepared().unwrap().width,
            targets.prepared().unwrap().height
        ),
        (23, 11)
    );
    render(&mut renderer, &mut targets, &scene, 23, 11);
    assert_eq!((targets.width, targets.height), (23, 11));
    assert_ne!(targets.view.texture(), &displayed);
    assert_eq!(
        ledger(&targets).total_known_bytes(),
        targets.texture_bytes()
    );
    renderer.check_health().unwrap();
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn failed_output_uniform_admission_keeps_the_front_and_blocks_unconverted_publication() {
    let mut renderer = renderer(ChannelPrecision::Float16);
    let scene = scene(&mut renderer);
    let mut targets = SdrPreviewFrameTargets::new(&renderer, 1, 1).unwrap();
    render(&mut renderer, &mut targets, &scene, 1, 1);
    let old = read_output(&renderer, &targets);
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let budget = renderer.allocation_budget().clone();
    // Drop the obsolete idle candidate, matching prepare's normal replacement.
    targets.discard_candidate();
    let before = budget.info().unwrap();
    let working = u64::from(SIZE * SIZE) * 8;
    let output = u64::from(SIZE * SIZE) * 4;
    budget
        .set_limit(before.admitted_bytes + working + output + 31)
        .unwrap();
    let error = targets.prepare(&renderer, SIZE, SIZE).err().unwrap();
    assert_eq!(
        error.resource_rejection(),
        Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: before.admitted_bytes + working + output + 32,
            budget_bytes: before.admitted_bytes + working + output + 31,
        })
    );
    assert!(targets.prepared().is_some());
    assert!(targets.prepared_output().is_none());
    assert!(targets.commit_submitted().is_err());
    assert!(targets.valid());
    assert_eq!(
        budget.info().unwrap().admitted_bytes,
        before.admitted_bytes + working
    );
    budget.set_limit(before.budget_bytes).unwrap();
    assert_eq!(read_output(&renderer, &targets), old);
    render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
    expect_pixels(&read_output(&renderer, &targets));
    renderer.check_health().unwrap();
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn conversion_is_required_before_commit_and_foreign_preview_owners_or_encoders_refuse() {
    let mut renderer = renderer(ChannelPrecision::Float32);
    let scene = scene(&mut renderer);
    let mut targets = SdrPreviewFrameTargets::new(&renderer, SIZE, SIZE).unwrap();
    targets.prepare(&renderer, SIZE, SIZE).unwrap();
    assert!(
        targets
            .commit_submitted()
            .unwrap_err()
            .to_string()
            .contains("encode preview output")
    );
    assert!(!targets.valid());
    let foreign = super_foreign();
    let before = ledger(&targets);
    assert!(
        targets
            .prepare(&foreign, SIZE, SIZE)
            .err()
            .unwrap()
            .to_string()
            .contains("another renderer")
    );
    assert_eq!(ledger(&targets), before);
    let mut encoder = foreign
        .allocation_command_encoder(&Default::default())
        .unwrap();
    assert!(
        targets
            .encode_output(&renderer, &mut encoder)
            .unwrap_err()
            .to_string()
            .contains("another renderer")
    );
    drop(encoder);
    render(&mut renderer, &mut targets, &scene, SIZE, SIZE);
    expect_pixels(&read_output(&renderer, &targets));
    renderer.check_health().unwrap();
    foreign.check_health().unwrap();
}
fn super_foreign() -> Renderer {
    renderer(ChannelPrecision::Unorm8)
}
fn read_output(renderer: &Renderer, target: &motion_render::RenderTarget) -> Vec<u8> {
    let stride = (target.width * 4).div_ceil(256) * 256;
    let buffer = renderer
        .allocation_context()
        .buffer(&wgpu::BufferDescriptor {
            label: Some("independent output readback"),
            size: u64::from(stride) * u64::from(target.height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        target.view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(target.height),
            },
        },
        wgpu::Extent3d {
            width: target.width,
            height: target.height,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let mut bytes = Vec::new();
    {
        let mapped = buffer.slice(..).get_mapped_range().unwrap();
        for row in mapped.chunks_exact(stride as usize) {
            bytes.extend_from_slice(&row[..target.width as usize * 4]);
        }
    }
    buffer.unmap();
    if matches!(
        target.view.texture().format(),
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    bytes
}
