//! Resized targets reuse cold pipelines/tables and isolate queued cell uniforms.
use super::*;
use motion_effects::native_color::NativeColorRuntime;
use motion_render::{ManagedDisplayPresenter, ManagedDisplayTarget};

#[test]
#[ignore = "requires the pinned native OCIO runtime and an actual GPU"]
fn rebound_display_keeps_native_pixels_and_independent_cells_without_new_tables() {
    let reference = reference();
    let configs = configs(&reference);
    let runtime = NativeColorRuntime::load(&std::path::PathBuf::from(
        std::env::var_os("MOTION_TEST_OCIO_RUNTIME").expect("explicit native runtime"),
    ))
    .unwrap();
    let case = reference
        .cases
        .iter()
        .find(|case| {
            case.processor.phase == ColorProcessorPhase::WorkingToDisplay
                && case.processor.input_alpha == AlphaMode::Premultiplied
                && !case.processor.tables.is_empty()
        })
        .expect("original display reference with numeric tables");
    let (config, selection) = &configs[case.config];
    let native = runtime
        .prepare(
            config,
            selection,
            case.processor.phase,
            case.processor.input_alpha,
            case.processor.output_alpha,
        )
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let foreign = pollster::block_on(Renderer::headless()).unwrap();
    let mut checked = 0;
    let mut wrong_geometry = 0;
    for opaque in [false, true] {
        let compile = |opaque| {
            if opaque {
                native
                    .source()
                    .compile_display_over_black(config, selection)
            } else {
                native.source().compile_image(config, selection)
            }
        };
        let processor = compile(opaque).unwrap();
        assert!(!processor.tables().is_empty());
        let input_contract = ColorImageContract {
            space: processor.input_space(),
            range: processor.input_range(),
            alpha: processor.input_alpha(),
            precision: ChannelPrecision::Float32,
        };
        let source = renderer
            .upload_color_image_f32(3, 4, input_contract, &case.input)
            .unwrap();
        let prototype = ManagedDisplayTarget::new(&renderer, &source, &processor).unwrap();
        let left_source = renderer
            .upload_color_image_f32(1, 1, input_contract, &[case.input[3]])
            .unwrap();
        let right_source = renderer
            .upload_color_image_f32(1, 1, input_contract, &[case.input[4]])
            .unwrap();
        let before = renderer.allocation_budget().info().unwrap();
        let left = prototype.rebind(&renderer, &left_source).unwrap();
        let right = prototype.rebind(&renderer, &right_source).unwrap();
        let after = renderer.allocation_budget().info().unwrap();
        assert_eq!(
            after.admitted_bytes - before.admitted_bytes,
            2 * 16,
            "two 1x1 display targets only: numeric tables must not be reallocated"
        );
        assert_eq!(after.live_allocations - before.live_allocations, 2);
        assert_eq!(left.dimensions(), [1, 1]);
        assert_eq!(right.dimensions(), [1, 1]);
        let budget = renderer.allocation_budget().clone();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes + 15).unwrap();
        assert!(
            prototype
                .rebind(&renderer, &left_source)
                .err()
                .unwrap()
                .resource_rejection()
                .is_some()
        );
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        assert_eq!(
            budget.info().unwrap().live_allocations,
            before.live_allocations
        );
        budget.set_limit(before.admitted_bytes + 16).unwrap();
        let exact = prototype.rebind(&renderer, &left_source).unwrap();
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            before.admitted_bytes + 16
        );
        drop(exact);
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        budget.set_limit(before.budget_bytes).unwrap();

        for mutation in 0..4 {
            let mut wrong = input_contract;
            match mutation {
                0 => wrong.space = processor.output_space(),
                1 => wrong.range = CodeValueRange::new(0.1, 0.9).unwrap(),
                2 => wrong.alpha = AlphaMode::Straight,
                3 => wrong.precision = ChannelPrecision::Float16,
                _ => unreachable!(),
            }
            let wrong = renderer.color_image_target(1, 1, wrong).unwrap();
            let before = renderer.allocation_budget().info().unwrap();
            assert!(
                prototype.rebind(&renderer, &wrong).is_err(),
                "mutation={mutation}"
            );
            let after = renderer.allocation_budget().info().unwrap();
            assert_eq!(before.admitted_bytes, after.admitted_bytes);
            assert_eq!(before.live_allocations, after.live_allocations);
        }
        let foreign_source = foreign.color_image_target(1, 1, input_contract).unwrap();
        assert!(prototype.rebind(&renderer, &foreign_source).is_err());
        assert!(prototype.rebind(&foreign, &foreign_source).is_err());
        let other =
            ManagedDisplayTarget::new(&renderer, &left_source, &compile(!opaque).unwrap()).unwrap();
        let expected = |index: usize| -> [f32; 4] {
            if !opaque {
                return case.expected[index];
            }
            let p = case.input[index];
            let rgb = native
                .apply_rgb(if p[3] > 0. {
                    [p[0], p[1], p[2]]
                } else {
                    [0.; 3]
                })
                .unwrap();
            [rgb[0], rgb[1], rgb[2], 1.]
        };
        let a = expected(3);
        let b = expected(4);
        let oracle: Vec<_> = (0..4).flat_map(|_| [a, a, a, b, b, b]).collect();
        // Rebound owners must retain their own source leases and the shared
        // tables after the original source and cold target have retired.
        drop(prototype);
        drop(source);
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ] {
            let prepared = ManagedDisplayPresenter::new(&renderer, &left, format).unwrap();
            let left_presenter = prepared.rebind(&renderer, &left).unwrap();
            let right_presenter = prepared.rebind(&renderer, &right).unwrap();
            if !opaque && format == wgpu::TextureFormat::Rgba8Unorm {
                let before = budget.info().unwrap();
                budget.set_limit(before.admitted_bytes + 63).unwrap();
                assert!(
                    prepared
                        .rebind(&renderer, &left)
                        .err()
                        .unwrap()
                        .resource_rejection()
                        .is_some()
                );
                assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
                assert_eq!(
                    budget.info().unwrap().live_allocations,
                    before.live_allocations
                );
                budget.set_limit(before.admitted_bytes + 64).unwrap();
                let exact = prepared.rebind(&renderer, &left).unwrap();
                assert_eq!(
                    budget.info().unwrap().admitted_bytes,
                    before.admitted_bytes + 64
                );
                drop(exact);
                assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
                budget.set_limit(before.budget_bytes).unwrap();
            }
            let before = renderer.allocation_budget().info().unwrap();
            assert!(
                prepared.rebind(&renderer, &other).is_err(),
                "other matte mode must not reuse the prepared display binding"
            );
            assert!(prepared.rebind(&foreign, &left).is_err());
            let after = renderer.allocation_budget().info().unwrap();
            assert_eq!(before.admitted_bytes, after.admitted_bytes);
            assert_eq!(before.live_allocations, after.live_allocations);
            drop(prepared);
            let output = managed_display::sized_surface(&renderer, format, 6, 4);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            left.encode(&renderer, &mut encoder).unwrap();
            right.encode(&renderer, &mut encoder).unwrap();
            left_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &output,
                    [0., 0., 3., 4.],
                    None,
                    [0.; 4],
                    false,
                )
                .unwrap();
            right_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &output,
                    [3., 0., 3., 4.],
                    None,
                    [0.; 4],
                    true,
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    left.collect_texture_resources(ledger);
                    right.collect_texture_resources(ledger);
                    left_presenter.collect_texture_resources(ledger);
                    right_presenter.collect_texture_resources(ledger);
                })
                .unwrap();
            let original = managed_display::pixels(&renderer, &output);
            assert_eq!(
                managed_display::errors(&original, &oracle, format),
                0,
                "opaque={opaque} {format:?}"
            );
            checked += oracle.len();
            // Actual wrong GPU route: one writable binding for two cell origins.
            // Both expected halves use the same source, isolating uniform reuse.
            let wrong = managed_display::sized_surface(&renderer, format, 6, 4);
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            left_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &wrong,
                    [0., 0., 3., 4.],
                    None,
                    [0.; 4],
                    false,
                )
                .unwrap();
            left_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &wrong,
                    [3., 0., 3., 4.],
                    None,
                    [0.; 4],
                    true,
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    left_presenter.collect_texture_resources(ledger)
                })
                .unwrap();
            let pixels = managed_display::pixels(&renderer, &wrong);
            let errors = managed_display::errors(&pixels, &vec![a; 24], format);
            assert!(
                errors > 0,
                "actual shared-geometry uniform route must fail opaque={opaque} {format:?}"
            );
            wrong_geometry += errors;
            // Explicit recovery with the independent bindings restores all bytes.
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            left_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &output,
                    [0., 0., 3., 4.],
                    None,
                    [0.; 4],
                    false,
                )
                .unwrap();
            right_presenter
                .encode_view_load(
                    &renderer,
                    &mut encoder,
                    &output,
                    [3., 0., 3., 4.],
                    None,
                    [0.; 4],
                    true,
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    left_presenter.collect_texture_resources(ledger);
                    right_presenter.collect_texture_resources(ledger);
                })
                .unwrap();
            assert_eq!(
                managed_display::pixels(&renderer, &output),
                original,
                "recovery restores the original qualified bytes"
            );
        }
    }
    renderer.check_health().unwrap();
    assert_eq!(checked, 192);
    println!(
        "rebound native SDR pixels={checked}; actual shared-uniform wrong components={wrong_geometry}; table reservations=shared"
    );
}
