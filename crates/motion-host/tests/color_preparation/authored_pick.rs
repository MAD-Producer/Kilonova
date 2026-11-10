//! Actual single-texel Float32 reads and inverse authored endpoint qualification.
use super::*;

#[test]
#[ignore = "requires explicit native OCIO prefix and an actual GPU"]
fn working_pixel_picks_preserve_authored_space_range_alpha_and_contracts() {
    let reference = reference();
    let mut renderer = renderer();
    match std::env::var("WGPU_BACKEND").as_deref() {
        Ok("vulkan") => assert_eq!(renderer.adapter.get_info().backend, wgpu::Backend::Vulkan),
        Ok("dx12") => assert_eq!(renderer.adapter.get_info().backend, wgpu::Backend::Dx12),
        _ => {}
    }
    let mut checked = 0;
    let mut grid_checked = false;
    for (stage, case) in reference
        .cases
        .iter()
        .enumerate()
        .filter(|(_, case)| case.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        for limited in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let mut project = document(
                &reference,
                case.config,
                case.processor.input_alpha,
                root.path(),
            );
            let (black, white) = if limited {
                (64. / 1023., 940. / 1023.)
            } else {
                (0., 1.)
            };
            if limited {
                settings(&mut project).authored_input.signal.range = SignalRange::Limited {
                    black: f64::from(black),
                    white: f64::from(white),
                };
                // Authored controls have their own endpoint; it must never be
                // substituted with the default asset input assignment.
                let settings = settings(&mut project);
                settings.selection.input = settings.selection.working.clone();
                settings.default_input = settings.working.clone();
            }
            let before = serde_json::to_value(&project).unwrap();
            let prepared = prep::prepare_native_authored_project(
                &project,
                root.path(),
                &runtime(),
                &Observation::new(8700 + stage as u64),
            )
            .unwrap()
            .unwrap();
            std::fs::remove_file(
                root.path()
                    .join(&project.color_management.resources().unwrap()[0].path),
            )
            .unwrap();
            let working: Vec<_> = case
                .expected
                .iter()
                .copied()
                .map(|rgba| premult(rgba, case.processor.output_alpha))
                .collect();
            let target = renderer
                .upload_color_image_f32(
                    working.len() as u32,
                    1,
                    prepared.authored.contract(),
                    &working,
                )
                .unwrap();
            for (index, expected) in case.input.iter().copied().enumerate() {
                let reads = renderer.image_readback_info();
                let (contract, actual) = renderer
                    .sample_color_image_pixel(&target, [index as u32, 0])
                    .unwrap();
                assert_eq!(contract, prepared.authored.contract());
                assert_eq!(
                    actual, working[index],
                    "raw Float32 read must not transfer or quantize"
                );
                let after = renderer.image_readback_info();
                assert_eq!(after.completed_calls, reads.completed_calls + 1);
                assert_eq!(
                    after.submitted_payload_bytes,
                    reads.submitted_payload_bytes + 16
                );
                assert_eq!(after.mapped_buffer_bytes, reads.mapped_buffer_bytes + 256);
                let actual = prepared
                    .authored
                    .working_color_to_authored(prepared.colors.config(), contract, actual)
                    .unwrap();
                let mut expected = expected;
                let alpha = expected[3];
                for value in &mut expected[..3] {
                    let straight = if case.processor.input_alpha == AlphaMode::Premultiplied {
                        if alpha > 0. {
                            *value / alpha
                        } else {
                            0.
                        }
                    } else if alpha > 0. {
                        *value
                    } else {
                        0.
                    };
                    *value = straight * (white - black) + black;
                    if case.processor.input_alpha == AlphaMode::Premultiplied {
                        *value *= alpha;
                    }
                }
                assert!(
                    close(actual, expected),
                    "stage={stage} limited={limited} sample={index}: {actual:?} != {expected:?}"
                );
                checked += 1;
            }
            let contract = prepared.authored.contract();
            for invalid in [
                [f32::NAN, 0., 0., 1.],
                [0., 0., 0., -0.1],
                [0., 0., 0., 1.1],
            ] {
                assert!(prepared
                    .authored
                    .working_color_to_authored(prepared.colors.config(), contract, invalid)
                    .is_err());
            }
            let overflow = prepared
                .authored
                .working_color_to_authored(
                    prepared.colors.config(),
                    contract,
                    [f32::MAX, 0., 0., f32::MIN_POSITIVE],
                )
                .unwrap_err();
            assert!(
                matches!(
                    &overflow,
                    AuthoredSceneError::PickNative {
                        phase: "color_pick_transform",
                        source,
                    } if matches!(source.as_ref(), motion_effects::native_color::Error::Admission("nonfinite authored RGB input"))
                ),
                "original native admission cause must survive: {overflow:?}"
            );
            for bad in [
                ColorImageContract {
                    space: prepared.colors.output().output_space(),
                    ..contract
                },
                ColorImageContract {
                    alpha: AlphaMode::Straight,
                    ..contract
                },
                ColorImageContract {
                    precision: ChannelPrecision::Float16,
                    ..contract
                },
                ColorImageContract {
                    range: CodeValueRange::new(0.1, 0.9).unwrap(),
                    ..contract
                },
            ] {
                assert!(prepared
                    .authored
                    .working_color_to_authored(prepared.colors.config(), bad, [0.; 4])
                    .is_err());
            }
            let reads = renderer.image_readback_info();
            assert!(renderer
                .sample_color_image_pixel(&target, [working.len() as u32, 0])
                .is_err());
            assert_eq!(renderer.image_readback_info(), reads);
            if !grid_checked {
                let pixels = [
                    [-1., 2., 0.25, 0.5],
                    [3., -2., 0., 1.],
                    [0.5, 0.25, 0.75, 0.],
                    [0.01, 0.02, 0.03, 0.1],
                ];
                let grid = renderer
                    .upload_color_image_f32(2, 2, contract, &pixels)
                    .unwrap();
                for y in 0..2 {
                    for x in 0..2 {
                        assert_eq!(
                            renderer.sample_color_image_pixel(&grid, [x, y]).unwrap(),
                            (contract, pixels[(y * 2 + x) as usize])
                        );
                    }
                }
                let reads = renderer.image_readback_info();
                assert!(renderer.sample_color_image_pixel(&grid, [0, 2]).is_err());
                assert_eq!(renderer.image_readback_info(), reads);
                let other = super::renderer();
                assert!(other.sample_color_image_pixel(&grid, [0, 0]).is_err());
                assert_eq!(other.image_readback_info().requested_calls, 0);
                let half = renderer
                    .color_image_target(
                        1,
                        1,
                        ColorImageContract {
                            precision: ChannelPrecision::Float16,
                            ..contract
                        },
                    )
                    .unwrap();
                let reads = renderer.image_readback_info();
                assert!(renderer.sample_color_image_pixel(&half, [0, 0]).is_err());
                assert_eq!(renderer.image_readback_info(), reads);
                grid_checked = true;
            }
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
        }
    }
    assert!(checked >= 60);
    assert!(grid_checked);
    println!("managed color pick: {checked} full/limited authored round trips through real Float32 texels");
}
