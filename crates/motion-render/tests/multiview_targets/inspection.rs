use super::*;
use motion_render::{
    multiview::ViewInspection, TextureResourceLedger, ViewChannel, ViewerSnapshot,
};

fn submit(
    renderer: &Renderer,
    presenter: &mut ViewFramePresenter,
    frames: &ViewFrameTargets,
    output: &RenderTarget,
    inspections: &[ViewInspection<'_>],
) {
    let placements = [0, 1, 2, 3].map(|index| ViewPlacement {
        viewport: [(index % 2) as f32 * 18., (index / 2) as f32 * 18., 16., 16.],
        image: None,
    });
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    presenter
        .encode_inspected(
            renderer,
            frames,
            &mut encoder,
            &output.view,
            output.view.texture().format(),
            [34, 34],
            &placements,
            inspections,
            [0., 0., 0., 1.],
        )
        .unwrap();
    renderer
        .submit_allocated(encoder.finish(), |ledger| {
            frames.collect_texture_resources(ledger);
            presenter.collect_texture_resources(ledger);
        })
        .unwrap();
}
fn cell(pixels: &[u8], index: usize) -> Vec<u8> {
    let x = index % 2 * 18;
    let y = index / 2 * 18;
    (y..y + 16)
        .flat_map(|row| pixels[(row * 34 + x) * 4..(row * 34 + x + 16) * 4].to_vec())
        .collect()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn channels_regions_and_comparisons_match_independent_working_image_oracles_in_four_precisions() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let mut renderer = new_renderer(precision);
        let mut frames = ViewFrameTargets::default();
        let sizes = [[16, 16]; 4];
        let colors = [
            [0.125, 0.25, 0.5, 0.75],
            [0.6, 0.125, 0.25, 0.5],
            [0.25, 0.5, 0.125, 1.],
            [0.5, 0.25, 0.125, 0.25],
        ];
        assert!(frames
            .update(&mut renderer, &sizes, |_, index, target, encoder| {
                clear(encoder, target, colors[index]);
                Ok(true)
            })
            .unwrap());
        let snapshots: Vec<_> = (0..4)
            .map(|i| ViewerSnapshot::capture(&renderer, frames.working_output(i).unwrap()).unwrap())
            .collect();
        assert!(frames
            .update(&mut renderer, &sizes, |_, index, target, encoder| {
                clear(encoder, target, colors[(index + 1) % 4]);
                Ok(true)
            })
            .unwrap());
        let inspections = [
            ViewInspection {
                channel: ViewChannel::Red,
                region: None,
                comparison: Some((snapshots[0].native(), 0.5)),
            },
            ViewInspection {
                channel: ViewChannel::Green,
                region: Some([0., 0., 0.5, 1.]),
                comparison: Some((snapshots[1].native(), 0.)),
            },
            ViewInspection {
                channel: ViewChannel::Blue,
                region: Some([0.25, 0.25, 0.5, 0.5]),
                comparison: None,
            },
            ViewInspection {
                channel: ViewChannel::Alpha,
                region: None,
                comparison: Some((snapshots[3].native(), 0.75)),
            },
        ];
        let output = renderer
            .color_output_target(34, 34, wgpu::TextureFormat::Rgba8UnormSrgb)
            .unwrap();
        let mut presenter = ViewFramePresenter::default();
        submit(&renderer, &mut presenter, &frames, &output, &inspections);
        let actual = bytes(&renderer, &output);
        let mut ledger = TextureResourceLedger::default();
        presenter.collect_texture_resources(&mut ledger);
        assert!(
            ledger.total_known_bytes() > 0,
            "selected comparison resources must be visible in the ledger"
        );
        for (index, inspection) in inspections.iter().enumerate() {
            let source = frames.working_output(index).unwrap();
            let mut oracle =
                Presenter::new_admitted(&renderer, &source.view, output.view.texture().format())
                    .unwrap();
            oracle
                .set_view_channel(&renderer.queue, inspection.channel)
                .unwrap();
            if let Some((target, _)) = inspection.comparison {
                oracle.set_comparison_source(&renderer.device, target);
            }
            oracle.set_inspection_geometry(
                inspection.region,
                inspection.comparison.map(|(_, split)| split),
            );
            let target = renderer
                .color_output_target(16, 16, wgpu::TextureFormat::Rgba8UnormSrgb)
                .unwrap();
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            oracle
                .encode_view(
                    &renderer.queue,
                    &mut encoder,
                    &target.view,
                    None,
                    [0., 0., 16., 16.],
                    None,
                    [0., 0., 0., 1.],
                )
                .unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    oracle.collect_inspection_resources(ledger)
                })
                .unwrap();
            assert_eq!(
                cell(&actual, index),
                bytes(&renderer, &target),
                "cell {index}, precision {precision:?}"
            );
        }
        assert_ne!(cell(&actual, 0), cell(&actual, 1));
        assert!(
            cell(&actual, 2)
                .chunks_exact(4)
                .any(|p| p == [0, 0, 0, 255]),
            "ROI must clear the surrounding pixels"
        );
        let defaults = [ViewInspection::default(); 4];
        submit(&renderer, &mut presenter, &frames, &output, &defaults);
        assert_ne!(
            actual,
            bytes(&renderer, &output),
            "resetting inspection must restore the live color views"
        );
        let mut cleared = TextureResourceLedger::default();
        presenter.collect_texture_resources(&mut cleared);
        assert_eq!(
            cleared.total_known_bytes(),
            0,
            "unselected comparisons must release presenter ownership"
        );
        submit(&renderer, &mut presenter, &frames, &output, &inspections);
        assert_eq!(actual, bytes(&renderer, &output));
        let placements = [0, 1, 2, 3].map(|i| ViewPlacement {
            viewport: [(i % 2) as f32 * 18., (i / 2) as f32 * 18., 16., 16.],
            image: None,
        });
        let mut invalid = inspections;
        invalid[3].region = Some([0., 0., 0., 1.]);
        let mut encoder = renderer
            .allocation_command_encoder(&Default::default())
            .unwrap();
        let error = presenter
            .encode_inspected(
                &renderer,
                &frames,
                &mut encoder,
                &output.view,
                output.view.texture().format(),
                [34, 34],
                &placements,
                &invalid,
                [0., 0., 0., 1.],
            )
            .unwrap_err();
        assert!(error.to_string().contains("invalid viewer inspection"));
        drop(encoder);
        assert_eq!(
            actual,
            bytes(&renderer, &output),
            "invalid later-cell geometry must not draw earlier cells"
        );
    }
}
