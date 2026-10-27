//! Actual saved-project coverage for geometry shared across model layer sources.
use super::*;

struct Paint(String);
impl AssetResolver for Paint {
    fn read(&mut self, path: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "paint.mtl");
        Ok(self.0.as_bytes().to_vec())
    }
}

fn crossing_project(root: &std::path::Path, samples: u32, blended: bool) -> Project {
    let mut project = Project::new(96, 64, 30, 61).unwrap();
    project.background = [0.; 4];
    std::fs::create_dir_all(root.join("assets/models")).unwrap();
    for (id, material) in [
        (7, "newmtl Paint\nKd 1 0 0\nillum 0\n"),
        (8, "newmtl Paint\nKd 0 0 1\nillum 0\n"),
    ] {
        let sign = if id == 7 { 1. } else { -1. };
        let (left, right) = if blended {
            (sign * 0.2, sign * 0.2)
        } else {
            (-sign * 0.6, sign * 0.6)
        };
        let source = format!("mtllib paint.mtl\nv -0.8 -0.6 {left}\nv 0.8 -0.6 {right}\nv 0.8 0.6 {right}\nv -0.8 0.6 {left}\nusemtl Paint\nf 1 2 3 4\n");
        let mut paint = material.to_string();
        if blended {
            paint.push_str("d 0.5\n");
        }
        let imported = motion_scene::obj::import_obj(
            ImportContext {
                request: 8710,
                source: id,
            },
            source.as_bytes(),
            &mut Paint(paint),
            &Default::default(),
            &ImportLimits::default(),
        )
        .unwrap();
        let frozen = imported
            .freeze(
                source.as_bytes(),
                motion_core::model_assets::MAX_MODEL_ASSET_BYTES as usize,
            )
            .unwrap();
        let path = format!("assets/models/plane-{id}.mscn");
        std::fs::write(root.join(&path), &frozen).unwrap();
        project.model_assets.push(ModelAsset {
            id,
            path,
            bytes: frozen.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&frozen)),
        });
        let mut layer = Layer::solid(
            id - 6,
            "Shared depth surface",
            [40.; 2],
            [48., 32., 0.],
            [1.; 4],
        );
        layer.three_d = true;
        layer.content = Content::Model {
            model: ModelClip {
                asset: id,
                scene: 0,
                pixels_per_metre: 20.,
                pivot: [0.; 3],
                samples,
                animation: None,
            },
        };
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}

fn capture(renderer: &mut Renderer, project: &Project) -> Vec<u8> {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&scene);
    let target = renderer.capture_target(96, 64).unwrap();
    renderer.capture(&scene, &target).unwrap().0
}
fn pixel(bytes: &[u8], x: usize) -> &[u8] {
    &bytes[(32 * 96 + x) * 4..][..4]
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_project_crossing_models_use_per_fragment_depth_in_either_layer_order() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let authored = crossing_project(root.path(), samples, false);
        motion_core::storage::save(root.path(), &authored).unwrap();
        let mut restored = motion_core::storage::load(root.path()).unwrap();
        assert_eq!(restored, authored);
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.configure_assets(&restored, root.path()).unwrap();
        for asset in &restored.model_assets {
            std::fs::remove_file(root.path().join(&asset.path)).unwrap();
        }
        let first = capture(&mut renderer, &restored);
        assert_eq!(
            pixel(&first, 40),
            &[0, 0, 255, 255],
            "left ray must hit nearer blue model, samples={samples}"
        );
        assert_eq!(
            pixel(&first, 56),
            &[255, 0, 0, 255],
            "right ray must hit nearer red model, samples={samples}"
        );
        restored.layers.reverse();
        let reversed = capture(&mut renderer, &restored);
        assert_eq!(
            first, reversed,
            "opaque geometry cannot depend on authored layer order"
        );
        restored.layers.reverse();
        assert_eq!(
            restored, authored,
            "shared rendering must preserve saved authoring"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_project_blended_models_sort_globally_across_sources() {
    for samples in [1, 4] {
        let root = tempfile::tempdir().unwrap();
        let mut project = crossing_project(root.path(), samples, true);
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.configure_assets(&project, root.path()).unwrap();
        let first = capture(&mut renderer, &project);
        let center = pixel(&first, 48);
        assert!(
            center[0] > center[2] && center[2] > 0 && center[3] > 0 && center[3] < 255,
            "nearer red must blend above blue in linear premultiplied color, got {center:?}"
        );
        project.layers.reverse();
        let reversed = capture(&mut renderer, &project);
        assert_eq!(
            first, reversed,
            "global transparent order cannot be the source/layer order"
        );
    }
}

fn sampled(project: &Project, frame: f64) -> Scene {
    let mut scene = Scene::new(project).unwrap();
    scene
        .sample(project, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
    scene
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_project_pending_frames_keep_their_own_poses_when_submitted_in_reverse() {
    let root = tempfile::tempdir().unwrap();
    let mut project = crossing_project(root.path(), 4, false);
    project.layers[0]
        .transform
        .position
        .set_animated(0, true)
        .unwrap();
    project.layers[0]
        .transform
        .position
        .set_at(60, [62., 32., 0.])
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let first_scene = sampled(&project, 0.);
    let last_scene = sampled(&project, 60.);
    let first = renderer.capture_target(96, 64).unwrap();
    let last = renderer.capture_target(96, 64).unwrap();
    let expected_first = renderer.capture(&first_scene, &first).unwrap().0;
    let expected_last = renderer.capture(&last_scene, &last).unwrap().0;
    assert_ne!(
        expected_first, expected_last,
        "authored poses must actually differ"
    );
    let mut first_encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&first_scene, &first.view, 96, 64, &mut first_encoder, None)
        .unwrap();
    let mut last_encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last_scene, &last.view, 96, 64, &mut last_encoder, None)
        .unwrap();
    renderer
        .submit_allocated(last_encoder.finish(), |_| {})
        .unwrap();
    renderer
        .submit_allocated(first_encoder.finish(), |_| {})
        .unwrap();
    assert_eq!(renderer.read_target(&first).unwrap(), expected_first);
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    let mut abandoned = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    renderer
        .encode(&last_scene, &last.view, 96, 64, &mut abandoned, None)
        .unwrap();
    drop(abandoned);
    // Uniform queue writes keep their leases until a real submission completes.
    // Drain those writes while proving the abandoned raster was never executed.
    assert_eq!(renderer.read_target(&last).unwrap(), expected_last);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before,
        "abandoned project commands must release immutable native frame reservations"
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_project_raster_budget_refuses_one_byte_short_and_deletion_releases_depth() {
    let root = tempfile::tempdir().unwrap();
    let mut project = crossing_project(root.path(), 4, false);
    let scene = sampled(&project, 0.);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let target = renderer.capture_target(96, 64).unwrap();
    let (before, stats) = renderer.capture(&scene, &target).unwrap();
    assert_eq!(
        stats.draw_calls, 3,
        "two native meshes share one composition raster"
    );
    let ledger = renderer.texture_resource_ledger();
    let depth_ids: Vec<_> = ledger
        .resources()
        .filter(|r| r.references.iter().any(|v| v.owner == "scene/depth"))
        .map(|r| r.id.clone())
        .collect();
    assert_eq!(
        depth_ids.len(),
        1,
        "the project must own one common native depth target"
    );
    let resident: u64 = ledger
        .resources()
        .filter(|r| {
            r.references.iter().any(|v| {
                v.owner.starts_with("scene/image/")
                    || v.owner.starts_with("images/Model(")
                    || v.owner == "images/Static(0)"
            })
        })
        .map(|r| {
            r.logical_bytes
                .expect("all admitted fixture formats have an exact capacity")
        })
        .sum();
    let required = resident + (192 * 128 - 96 * 64) * 4;
    let original_policy = renderer.memory_policy();
    let mut policy = original_policy;
    policy.source_texture_budget_bytes = required - 1;
    renderer.set_memory_policy(policy).unwrap();
    let larger = renderer.capture_target(192, 128).unwrap();
    let mut rejected = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    let failure = renderer
        .encode(&scene, &larger.view, 192, 128, &mut rejected, None)
        .unwrap_err();
    assert!(matches!(failure, motion_render::RenderError::Resource {
        rejection: motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::SourceTextures, required_bytes, budget_bytes,
        }, ..
    } if required_bytes == required && budget_bytes == required - 1));
    drop(rejected);
    assert_eq!(
        renderer.read_target(&target).unwrap(),
        before,
        "refusal must preserve the completed image"
    );
    renderer.set_memory_policy(original_policy).unwrap();
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, before);
    project.layers.clear();
    project.model_assets.clear();
    renderer.configure_assets(&project, root.path()).unwrap();
    let empty = sampled(&project, 0.);
    renderer.retain_video_instances(&empty);
    assert!(renderer
        .capture(&empty, &target)
        .unwrap()
        .0
        .iter()
        .all(|v| *v == 0));
    let after = renderer.texture_resource_ledger();
    assert!(
        after.resources().all(|r| !depth_ids.contains(&r.id)),
        "deleted model composition must retire its shared depth"
    );
    assert_eq!(renderer.texture_memory_breakdown().model_sources, 0);
}

fn two_native_pixels(renderer: &Renderer, target: &motion_render::RenderTarget) -> Vec<u8> {
    let bytes = target
        .view
        .texture()
        .format()
        .block_copy_size(None)
        .unwrap() as usize;
    let buffer = renderer
        .allocation_context()
        .buffer(&wgpu::BufferDescriptor {
            label: Some("independent project precision rays"),
            size: 512,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
        .unwrap();
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    for (row, x) in [40, 56].into_iter().enumerate() {
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: target.view.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d { x, y: 32, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: row as u64 * 256,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
    }
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    let result = [&mapped[..bytes], &mapped[256..256 + bytes]].concat();
    drop(mapped);
    buffer.unmap();
    result
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn shared_project_depth_preserves_four_working_precisions_and_both_sample_counts() {
    use motion_effects::image_contract::ChannelPrecision::*;
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        for samples in [1, 4] {
            let root = tempfile::tempdir().unwrap();
            let project = crossing_project(root.path(), samples, false);
            let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
                precision,
                motion_render::AllocationBudget::new(128 << 20).unwrap(),
            ))
            .unwrap();
            renderer.configure_assets(&project, root.path()).unwrap();
            let scene = sampled(&project, 0.);
            let target = renderer.render_target(96, 64).unwrap();
            let stats = renderer.draw(&scene, &target.view, 96, 64).unwrap();
            assert_eq!(stats.draw_calls, 3);
            let ideal: Vec<u8> = [0, 0, 1, 1, 1, 0, 0, 1]
                .into_iter()
                .flat_map(|v| match precision {
                    Unorm8 => vec![if v == 1 { 255 } else { 0 }],
                    Unorm16 => (if v == 1 { u16::MAX } else { 0 }).to_le_bytes().to_vec(),
                    Float16 => (if v == 1 { 0x3c00u16 } else { 0 }).to_le_bytes().to_vec(),
                    Float32 => (if v == 1 { 1f32 } else { 0f32 }).to_le_bytes().to_vec(),
                })
                .collect();
            let (format, pixel_bytes) = match precision {
                Unorm8 => (wgpu::TextureFormat::Rgba8UnormSrgb, 4),
                Unorm16 => (wgpu::TextureFormat::Rgba16Unorm, 8),
                Float16 => (wgpu::TextureFormat::Rgba16Float, 8),
                Float32 => (wgpu::TextureFormat::Rgba32Float, 16),
            };
            assert_eq!(target.view.texture().format(), format);
            // The unchanged isolated model path is the color oracle. Hardware
            // Float16 conversion can round an interpolated near-one value down
            // by one representable unit; shared depth must preserve those exact
            // native bytes rather than introduce another conversion or downgrade.
            let mut expected = Vec::with_capacity(pixel_bytes * 2);
            for (id, range) in [(2, 0..pixel_bytes), (1, pixel_bytes..pixel_bytes * 2)] {
                let mut isolated = project.clone();
                isolated.layers.retain(|layer| layer.id == id);
                let isolated_scene = sampled(&isolated, 0.);
                let isolated_target = renderer.render_target(96, 64).unwrap();
                renderer
                    .draw(&isolated_scene, &isolated_target.view, 96, 64)
                    .unwrap();
                let pixels = two_native_pixels(&renderer, &isolated_target);
                expected.extend_from_slice(&pixels[range]);
            }
            assert_eq!(expected.len(), pixel_bytes * 2);
            assert!(
                expected.iter().any(|v| *v != 0),
                "the independent sources must cover actual pixels"
            );
            if let Some(dir) = std::env::var_os("MOTION_PROJECT_MODEL_PRECISION_REPORT") {
                let path =
                    std::path::PathBuf::from(dir).join(format!("{precision:?}-{samples}.json"));
                std::fs::write(path, serde_json::to_vec_pretty(&serde_json::json!({
                    "precision":format!("{precision:?}"),"samples":samples,"ideal":ideal,"isolated":expected,
                    "reference":"unchanged one-model renderer path; blue left ray then red right ray",
                })).unwrap()).unwrap();
            }
            assert_eq!(
                two_native_pixels(&renderer, &target),
                expected,
                "precision={precision:?} samples={samples}"
            );
        }
    }
}
