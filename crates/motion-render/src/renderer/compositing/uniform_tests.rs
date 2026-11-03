use super::*;
use motion_core::compositing::{MatteMode, TrackMatte};

// Independent copy of the old per-source payload, also used by the test-only
// legacy queue-write path. It does not call the new packing implementation.
pub(super) fn reference_uniform(
    frame: &crate::effect_plan::EffectFramePlan,
    index: usize,
    size: [u32; 2],
) -> Result<DrawUniform> {
    let d = &frame.draws[index];
    let c = frame.compositing[index];
    let flags = u32::from(c.mode != 0) * 2 + u32::from(c.mode == 2 || c.mode == 4) * 4;
    Ok(DrawUniform {
        mvp: std::array::from_fn(|col| std::array::from_fn(|row| d.words[col * 4 + row])),
        color: [1.; 4],
        extent_opacity: [0., 0., d.words[22], flags as f32],
        uv_scale: [1., 1., size[0] as f32, size[1] as f32],
        screen_uv: crate::plane_coordinates::screen_map(
            frame.uv_maps[index],
            [0., 0., size[0] as f32, size[1] as f32],
            d.layer,
        )?,
    })
}

fn project(count: u64) -> Project {
    let mut project = Project::new(32, 24, 30, 120).unwrap();
    project.background = [0.1, 0.2, 0.3, 0.4];
    for id in 1..=count {
        let mut layer = motion_core::Layer::solid(
            id,
            "Matte uniform",
            [16., 12.],
            [8. + id as f32 / 3., 12., 0.],
            [id as f32 / count as f32, 0.4, 0.8, 0.6],
        );
        if (id - 1) % 8 != 0 {
            layer.track_matte = Some(TrackMatte {
                source: id - 1,
                hide_source: false,
                mode: if id % 2 == 0 {
                    MatteMode::Alpha
                } else {
                    MatteMode::Luma
                },
            });
        }
        project.layers.push(layer);
    }
    project.validate().unwrap();
    project
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn many_matte_sources_share_one_uniform_upload_without_changing_pixels() {
    let project = project(32);
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(32, 24).unwrap();
    renderer.matte_planning.uniform_uploads.set(0);
    renderer.capture(&scene, &target).unwrap();
    assert_eq!(renderer.matte_planning.order.len(), 28);
    assert_eq!(
        renderer.matte_planning.uniform_uploads.get(),
        1,
        "all projected matte uniforms must share one queue upload"
    );
    renderer.check_health().unwrap();
}

#[test]
fn packed_matte_uniforms_preserve_sparse_offsets_modes_and_bytes_without_warm_allocations() {
    use crate::effect_plan::{EffectFramePlan, LayerComposite, PlannedDraw, DRAW_WORDS};
    let frame = EffectFramePlan {
        draws: (0..MAX_LAYERS)
            .map(|i| PlannedDraw {
                layer: u64::MAX - i as u64,
                words: std::array::from_fn::<_, DRAW_WORDS, _>(|j| {
                    (i * DRAW_WORDS + j) as f32 / 100.
                }),
                pass_start: 0,
                pass_end: 0,
            })
            .collect(),
        compositing: (0..MAX_LAYERS)
            .map(|i| LayerComposite {
                blend: 0,
                space: 0,
                visible: true,
                matte: -1,
                mode: (i % 4 + 1) as u32,
            })
            .collect(),
        uv_maps: (0..MAX_LAYERS)
            .map(|i| {
                if i % 3 == 0 {
                    glam::DMat3::ZERO
                } else {
                    glam::DMat3::from_cols(
                        glam::DVec3::new(0.8, 0.1, 0.001),
                        glam::DVec3::new(0.2, 0.9, 0.),
                        glam::DVec3::new(i as f64, -2., 1.),
                    )
                }
            })
            .collect(),
        ..Default::default()
    };
    let stride = (DRAW_SIZE as usize).div_ceil(256) * 256;
    let mut planning = MattePlanning::default();
    for count in [1, 8, MAX_LAYERS] {
        planning.order = (0..count).rev().map(|i| (i, i % 2 == 0)).collect();
        // Both extraction modes of one source share its unchanged uniform slot.
        planning.order.push((count - 1, false));
        planning.uniform_bytes(&frame, [47, 31], stride).unwrap();
        let allocations = crate::test_allocations::count(|| {
            for _ in 0..120 {
                let (offset, bytes) = planning.uniform_bytes(&frame, [47, 31], stride).unwrap();
                assert_eq!(offset, 0);
                assert_eq!(bytes.len(), (count - 1) * stride + DRAW_SIZE as usize);
                for i in 0..count {
                    let expected = reference_uniform(&frame, i, [47, 31]).unwrap();
                    assert_eq!(
                        &bytes[i * stride..i * stride + DRAW_SIZE as usize],
                        bytemuck::bytes_of(&expected)
                    );
                }
            }
        });
        assert_eq!(allocations, 0);
    }
    for order in [
        vec![(127, false)],
        vec![(127, true), (3, false), (127, false)],
        vec![],
    ] {
        planning.order = order.clone();
        let first = order.iter().map(|&(i, _)| i).min().unwrap_or(0);
        let last = order.iter().map(|&(i, _)| i).max().unwrap_or(0);
        let (offset, bytes) = planning.uniform_bytes(&frame, [19, 23], stride).unwrap();
        assert_eq!(offset as usize, first * stride);
        assert_eq!(
            bytes.len(),
            if order.is_empty() {
                0
            } else {
                (last - first) * stride + DRAW_SIZE as usize
            }
        );
        for (i, _) in order {
            let expected = reference_uniform(&frame, i, [19, 23]).unwrap();
            let start = i * stride - offset as usize;
            assert_eq!(
                &bytes[start..start + DRAW_SIZE as usize],
                bytemuck::bytes_of(&expected)
            );
        }
    }
    assert!(planning.uniform_upload.capacity() <= MAX_LAYERS * stride);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn batched_matte_upload_matches_legacy_pixels_in_four_precisions_after_scene_changes() {
    use motion_effects::{
        image_contract::{ChannelPrecision, ImageContract, ImageMeaning},
        AlphaMode, WorkingSpace,
    };
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = pollster::block_on(Renderer::headless_with_working_precision(
            precision,
            crate::AllocationBudget::default(),
        ))
        .unwrap();
        let target = renderer
            .image_capture_target(
                47,
                31,
                ImageContract::new(
                    ImageMeaning::Color {
                        space: WorkingSpace::Linear,
                        alpha: AlphaMode::Premultiplied,
                    },
                    precision,
                ),
            )
            .unwrap();
        for count in [2, 8, 32, 128, 3, 0, 16] {
            let mut project = project(count);
            for layer in &mut project.layers {
                layer.transform.position.value[0] = 8. + (layer.id % 17) as f32 / 2.;
                if let Some(matte) = &mut layer.track_matte {
                    matte.mode = match layer.id % 4 {
                        0 => MatteMode::Alpha,
                        1 => MatteMode::AlphaInverted,
                        2 => MatteMode::Luma,
                        _ => MatteMode::LumaInverted,
                    };
                }
            }
            if count == 3 {
                project.layers[2].track_matte = None;
                project.layers[2].visible = false;
                project.layers[0].track_matte = Some(TrackMatte {
                    source: 3,
                    hide_source: true,
                    mode: MatteMode::Alpha,
                });
                project.layers[1].track_matte = Some(TrackMatte {
                    source: 3,
                    hide_source: true,
                    mode: MatteMode::LumaInverted,
                });
            }
            project.validate().unwrap();
            let mut previous = None;
            for revision in 0..3 {
                for layer in &mut project.layers {
                    layer.transform.position.value[0] += 0.75;
                }
                let mut scene = Scene::new(&project).unwrap();
                scene
                    .sample(
                        &project,
                        revision as f64,
                        None,
                        &motion_core::ExpressionEvaluator,
                    )
                    .unwrap();
                for preview in [false, true] {
                    renderer.matte_planning.legacy_uniform_upload = true;
                    renderer.matte_planning.uniform_uploads.set(0);
                    let expected_stats = renderer
                        .draw_internal(
                            &scene,
                            target.view(),
                            47,
                            31,
                            preview,
                            crate::gpu_commands::CommandStage::Composition,
                        )
                        .unwrap();
                    let legacy_uploads = renderer.matte_planning.uniform_uploads.get();
                    let expected = renderer.read_image_target(&target).unwrap().pixels;
                    let ledger = renderer.texture_resource_ledger();
                    renderer.matte_planning.legacy_uniform_upload = false;
                    renderer.matte_planning.uniform_uploads.set(0);
                    let stats = renderer
                        .draw_internal(
                            &scene,
                            target.view(),
                            47,
                            31,
                            preview,
                            crate::gpu_commands::CommandStage::Composition,
                        )
                        .unwrap();
                    let actual = renderer.read_image_target(&target).unwrap().pixels;
                    assert_eq!(
                        actual, expected,
                        "{precision:?} count={count} revision={revision} preview={preview}"
                    );
                    assert_eq!(stats.draw_calls, expected_stats.draw_calls);
                    assert_eq!(stats.texture_bytes, expected_stats.texture_bytes);
                    assert_eq!(renderer.texture_resource_ledger(), ledger);
                    assert_eq!(
                        renderer.matte_planning.uniform_uploads.get(),
                        usize::from(count > 0)
                    );
                    assert_eq!(
                        legacy_uploads,
                        if count == 3 {
                            2
                        } else {
                            (count / 8 * 7 + count % 8).saturating_sub(u64::from(count % 8 > 0))
                                as usize
                        }
                    );
                    if !preview && count != 0 {
                        if let Some(previous) = &previous {
                            assert_ne!(&actual, previous);
                        }
                        previous = Some(actual);
                    }
                    println!("precision={precision:?} layers={count} revision={revision} preview={preview} legacy_uploads={legacy_uploads} batched_uploads={}", renderer.matte_planning.uniform_uploads.get());
                    renderer.check_health().unwrap();
                }
            }
        }
        renderer.clear_assets();
        assert_eq!(renderer.matte_planning.uniform_upload.capacity(), 0);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalid_matte_projection_refuses_before_upload_and_recovers_without_changing_output() {
    let project = project(8);
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(32, 24).unwrap();
    let expected = renderer.capture(&scene, &target).unwrap().0;
    let ledger = renderer.texture_resource_ledger();
    let index = renderer.matte_planning.order[2].0;
    let layer = renderer.effect_gpu.builder.frame.draws[index].layer;
    let map = renderer.effect_gpu.builder.frame.uv_maps[index];
    renderer.effect_gpu.builder.frame.uv_maps[index] =
        glam::DMat3::from_diagonal(glam::DVec3::splat(f64::NAN));
    let error = renderer
        .prepare_track_matte_uniforms([32, 24])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!(
            "object={layer} phase=plane_coordinates: nonfinite inverse projection"
        )),
        "{error}"
    );
    assert_eq!(renderer.matte_planning.uniform_uploads.get(), 0);
    assert_eq!(renderer.texture_resource_ledger(), ledger);
    assert_eq!(renderer.read_target(&target).unwrap(), expected);
    renderer.effect_gpu.builder.frame.uv_maps[index] = map;
    renderer.prepare_track_matte_uniforms([32, 24]).unwrap();
    assert_eq!(renderer.matte_planning.uniform_uploads.get(), 1);
    assert_eq!(renderer.capture(&scene, &target).unwrap().0, expected);
    renderer.check_health().unwrap();
}
