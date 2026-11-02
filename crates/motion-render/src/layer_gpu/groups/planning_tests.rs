use super::*;
use crate::{test_allocations, Renderer, Scene};
use motion_core::{Layer, Project};
use std::{collections::BTreeSet, hint::black_box, sync::Arc};

fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("vector planning test images"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

fn meshes(count: usize) -> Vec<VectorMesh> {
    (0..count)
        .map(|i| {
            let depth = i % 4;
            let commands = (0..depth)
                .map(|_| RasterCommand {
                    kind: 1,
                    start: 0,
                    end: 0,
                    opacity: 0.5,
                })
                .chain((0..depth).map(|_| RasterCommand {
                    kind: 2,
                    start: 0,
                    end: 0,
                    opacity: 0.5,
                }))
                .collect();
            VectorMesh {
                layer: i,
                width: 8 + (i % 4) as u32,
                height: 8 + (i % 3) as u32,
                fingerprint: i as u64,
                vertices: Arc::new(vec![]),
                commands: Arc::new(commands),
                root_opacity: 0.5,
                origin: [0.; 2],
            }
        })
        .collect()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn warmed_group_target_planning_allocates_nothing_and_releases_cpu_storage() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let image = layout(&renderer.device);
    let mut gpu = GroupGpu::new(
        &renderer.device,
        &image,
        renderer.allocation_budget(),
        motion_effects::image_contract::ChannelPrecision::Unorm8,
    );
    for count in [1, 8, 96] {
        let meshes = meshes(count);
        // Independent ordered-set oracle, including duplicate sizes and depth.
        let expected: BTreeSet<_> = (0..count)
            .flat_map(|i| {
                (0..=i % 4).map(move |depth| (8 + (i % 4) as u32, 8 + (i % 3) as u32, depth))
            })
            .collect();
        let bytes = expected
            .iter()
            .map(|(w, h, _)| u64::from(*w) * u64::from(*h) * 4 * 5)
            .sum::<u64>();
        assert_eq!(gpu.required_bytes(&meshes), bytes);
        assert_eq!(
            gpu.needed_scratch.iter().copied().collect::<BTreeSet<_>>(),
            expected
        );
        let old_allocations = test_allocations::count(|| {
            for _ in 0..120 {
                let old: HashSet<_> = meshes
                    .iter()
                    .flat_map(|mesh| {
                        (0..=mesh.scope_depth()).map(move |depth| (mesh.width, mesh.height, depth))
                    })
                    .collect();
                black_box(old);
            }
        });
        assert!(
            old_allocations >= 120,
            "observer must detect the previous allocating path"
        );
        let allocations = test_allocations::count(|| {
            for _ in 0..120 {
                assert_eq!(gpu.required_bytes(&meshes), bytes);
                gpu.retain(&meshes);
            }
        });
        assert_eq!(
            allocations, 0,
            "warm budget and retain collection must reuse CPU storage"
        );
        eprintln!("vector group planning layers={count} iterations=120 old_allocations={old_allocations} allocations={allocations} bytes={bytes}");
    }
    gpu.required_bytes(&meshes(96));
    assert!(gpu.needed_scratch.capacity() > 0);
    gpu.release();
    assert_eq!(gpu.needed_scratch.capacity(), 0);
    assert_eq!(gpu.staged_scratch.capacity(), 0);
    assert_eq!(gpu.required_bytes(&[]), 0);
    assert!(gpu.needed_scratch.is_empty());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn group_staging_deduplicates_targets_and_recovery_never_reuses_failed_membership() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let image = layout(&renderer.device);
    let sampler = renderer.device.create_sampler(&Default::default());
    let budget = renderer.allocation_budget().clone();
    let mut gpu = GroupGpu::new(
        &renderer.device,
        &image,
        &budget,
        motion_effects::image_contract::ChannelPrecision::Unorm8,
    );
    let meshes = meshes(96);
    let mut project = Project::new(32, 32, 30, 30).unwrap();
    project.layers = (0..96)
        .map(|i| Layer::solid(i + 1, "Vector planning", [8.; 2], [0.; 3], [1.; 4]))
        .collect();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let refs: Vec<_> = meshes.iter().collect();
    let before = budget.info().unwrap().admitted_bytes;
    budget.set_limit(before).unwrap();
    let failure = gpu
        .stage_targets(&renderer.device, &image, &sampler, &scene, &refs, &budget)
        .err()
        .unwrap();
    assert!(failure.resource_rejection().is_some(), "{failure}");
    assert!(
        failure.to_string().contains("group_msaa_allocate depth=0"),
        "{failure}"
    );
    assert!(gpu.targets.is_empty());
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    budget
        .set_limit(crate::DEFAULT_RENDER_ALLOCATION_BUDGET)
        .unwrap();
    let staged = gpu
        .stage_targets(&renderer.device, &image, &sampler, &scene, &refs, &budget)
        .unwrap();
    gpu.commit_targets(staged);
    assert_eq!(gpu.bytes(), gpu.required_bytes(&meshes));
    assert_eq!(gpu.targets.len(), gpu.needed_scratch.len());
    let warm = budget.info().unwrap();
    let allocations = test_allocations::count(|| {
        for _ in 0..120 {
            let staged = gpu
                .stage_targets(&renderer.device, &image, &sampler, &scene, &refs, &budget)
                .unwrap();
            assert!(staged.0.is_empty());
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, warm.admitted_bytes);
    let next = &meshes[24..32];
    gpu.retain(next);
    assert_eq!(gpu.bytes(), gpu.required_bytes(next));
    gpu.retain(&[]);
    assert_eq!(gpu.bytes(), 0);
    assert_eq!(budget.info().unwrap().admitted_bytes, before);
    gpu.release();
    assert_eq!(gpu.staged_scratch.capacity(), 0);
}
