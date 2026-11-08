use super::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::{ColorImageContract, ColorImageTarget, ColorProcessorOutput, Renderer};
use prep::{PreparedProjectColors, SourceImageRequest};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Deserialize)]
struct OriginalReference {
    runtime: String,
    reference_sha256: String,
    absolute_tolerance: f32,
    relative_tolerance: f32,
    width: u32,
    height: u32,
    samples: Vec<[u16; 4]>,
    cases: Vec<OriginalCase>,
}
#[derive(Deserialize)]
struct OriginalCase {
    config: usize,
    stages: [usize; 2],
    intermediate: Vec<[f32; 4]>,
    expected: Vec<[f32; 4]>,
}
fn original_reference() -> OriginalReference {
    serde_json::from_str(include_str!("source_reference.json")).unwrap()
}
fn write_png(root: &Path, reference: &OriginalReference) {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let pixels: Vec<u16> = reference.samples.iter().flatten().copied().collect();
    image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(reference.width, reference.height, pixels)
        .unwrap()
        .save(root.join("assets/1.png"))
        .unwrap();
}
fn document(reference: &Reference, root: &Path, config: usize, alpha: AlphaMode) -> Project {
    let mut p = project(&reference.configs[config], alpha, root);
    p.assets[0].width = 3;
    p.assets[0].height = 4;
    settings(&mut p).sources.push(SourceColorAssignment {
        asset: 3,
        interpretation: SourceInterpretation::Data {
            meaning: ImageMeaning::LinearDepth {},
        },
    });
    p.validate().unwrap();
    p
}
fn prepare(reference: &Reference, p: &Project, root: &Path) -> PreparedProjectColors {
    prep::prepare_project_colors(p, root, |r| provider(reference, r), &Observation::new(8000))
        .unwrap()
        .unwrap()
}
fn request<'a>(
    p: &'a Project,
    root: &'a Path,
    asset: u64,
    cancel: &'a AtomicBool,
) -> SourceImageRequest<'a> {
    SourceImageRequest {
        project: p,
        root,
        asset,
        cancel,
    }
}
fn read(renderer: &Renderer, target: &ColorImageTarget) -> Vec<[f32; 4]> {
    renderer
        .read_color_image_target(target)
        .unwrap()
        .pixels
        .chunks_exact(16)
        .map(|v| {
            std::array::from_fn(|c| f32::from_le_bytes(v[c * 4..c * 4 + 4].try_into().unwrap()))
        })
        .collect()
}
fn compare(actual: &[[f32; 4]], expected: &[[f32; 4]], r: &OriginalReference) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        for c in 0..4 {
            assert!(
                a[c].is_finite()
                    && (a[c] - e[c]).abs()
                        <= r.absolute_tolerance + r.relative_tolerance * e[c].abs(),
                "original-file CPU reference pixel={index} channel={c} actual={} expected={}",
                a[c],
                e[c]
            );
        }
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_16bit_files_enter_project_input_and_ocio_endpoints_without_rgba8_or_readback() {
    let reference = reference();
    let original = original_reference();
    assert_eq!(original.runtime, "2.6.0");
    assert_eq!(
        original.reference_sha256,
        "bead7889440e39f8483904b088c948464223dbbc18f8cb0d8c5ec8b901b429a0"
    );
    assert_eq!(
        (original.absolute_tolerance, original.relative_tolerance),
        (0.0001, 0.0002)
    );
    assert_eq!(original.cases.len(), 14);
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let baseline = renderer.allocation_budget().info().unwrap().admitted_bytes;
    for case in &original.cases {
        let root = tempfile::tempdir().unwrap();
        let alpha = reference.cases[case.stages[0]].processor.input_alpha;
        let p = document(&reference, root.path(), case.config, alpha);
        let saved = p.clone();
        write_png(root.path(), &original);
        let prepared = prepare(&reference, &p, root.path());
        let observation = Observation::new(8001 + case.stages[1] as u64);
        let reads = renderer.image_readback_info();
        let installed = prepared
            .prepare_source_image(
                &mut renderer,
                request(&p, root.path(), 1, &AtomicBool::new(false)),
                &observation,
            )
            .unwrap()
            .unwrap();
        assert!(!observation.running());
        assert_eq!(installed.asset(), 1);
        assert_eq!(installed.source_precision(), ChannelPrecision::Unorm16);
        assert_eq!(
            installed.working().contract().space,
            prepared.default_input().output_space()
        );
        assert_eq!(installed.working().contract().alpha, alpha);
        assert_eq!(
            installed.working().contract().precision,
            ChannelPrecision::Float32
        );
        assert_eq!(
            renderer.image_readback_info(),
            reads,
            "source installation must not read pixels back"
        );
        assert_eq!(installed.working().image().readback_bytes(), 0);
        let processor = reference.cases[case.stages[1]]
            .processor
            .compile_image(prepared.config(), &reference.configs[case.config].selection)
            .unwrap();
        let endpoint = renderer
            .color_image_target(
                3,
                4,
                ColorImageContract {
                    range: motion_effects::color_processor::CodeValueRange::FULL,
                    space: processor.output_space(),
                    alpha: processor.output_alpha(),
                    precision: ChannelPrecision::Float32,
                },
            )
            .unwrap();
        let binding = ColorProcessorOutput::new(
            &renderer,
            installed.working(),
            &processor,
            endpoint.contract(),
        )
        .unwrap();
        for _ in 0..2 {
            let mut encoder = renderer
                .allocation_command_encoder(&Default::default())
                .unwrap();
            binding.encode(&renderer, &mut encoder, &endpoint).unwrap();
            renderer
                .submit_allocated(encoder.finish(), |ledger| {
                    ledger.merge(binding.texture_resource_ledger());
                    ledger.merge(endpoint.texture_resource_ledger());
                })
                .unwrap();
        }
        assert_eq!(
            renderer.image_readback_info(),
            reads,
            "endpoint reuse must not read back intermediate pixels"
        );
        let working = read(&renderer, installed.working());
        compare(&working, &case.intermediate, &original);
        assert_ne!(
            working[0], working[1],
            "adjacent 16-bit codes must survive decoding and input conversion"
        );
        compare(&read(&renderer, &endpoint), &case.expected, &original);
        assert_eq!(p, saved);
        assert!(!root.path().join(".cache").exists());
        drop((binding, endpoint, installed));
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(
            renderer.allocation_budget().info().unwrap().admitted_bytes,
            baseline
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn source_routes_refuse_stale_unknown_video_and_native_failures_and_recover_without_partial_images()
{
    let reference = reference();
    let original = original_reference();
    let root = tempfile::tempdir().unwrap();
    let p = document(&reference, root.path(), 0, AlphaMode::Straight);
    write_png(root.path(), &original);
    let prepared = prepare(&reference, &p, root.path());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let observation = Observation::new(8100);
    let cancel = AtomicBool::new(false);
    let before = renderer.allocation_budget().info().unwrap().admitted_bytes;
    // A data source has no file. Color import must bypass it without IO or allocation.
    assert!(prepared
        .prepare_source_image(
            &mut renderer,
            request(&p, root.path(), 3, &cancel),
            &observation
        )
        .unwrap()
        .is_none());
    for asset in [5, 99] {
        assert!(matches!(
            prepared.prepare_source_image(
                &mut renderer,
                request(&p, root.path(), asset, &cancel),
                &observation
            ),
            Err(prep::Error::SourceBinding { .. })
        ));
        assert!(!observation.running());
    }
    let mut stale = p.clone();
    settings(&mut stale).display.alpha = AlphaMode::Premultiplied;
    assert!(matches!(
        prepared.prepare_source_image(
            &mut renderer,
            request(&stale, root.path(), 1, &cancel),
            &observation
        ),
        Err(prep::Error::SourceBinding { .. })
    ));
    cancel.store(true, Ordering::Relaxed);
    assert!(
        matches!(prepared.prepare_source_image(&mut renderer,request(&p,root.path(),1,&cancel),&observation),Err(prep::Error::SourceImage { source,.. }) if matches!(*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Cancelled)))
    );
    cancel.store(false, Ordering::Relaxed);
    let mut policy = renderer.memory_policy();
    policy.decoded_image_budget_bytes = 191;
    renderer.set_memory_policy(policy).unwrap();
    let err = prepared
        .prepare_source_image(
            &mut renderer,
            request(&p, root.path(), 1, &cancel),
            &observation,
        )
        .err()
        .unwrap();
    let prep::Error::SourceImage {
        phase: "decode_original",
        source,
        ..
    } = err
    else {
        panic!("budget keeps decode context")
    };
    let motion_host::Error::ImageResource(err) = *source else {
        panic!("native image error")
    };
    assert!(err.resource_rejection().is_some());
    policy.decoded_image_budget_bytes = 1024 * 1024;
    renderer.set_memory_policy(policy).unwrap();
    let path = root.path().join("assets/1.png");
    let good = std::fs::read(&path).unwrap();
    let mut corrupt = good.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    let err = prepared
        .prepare_source_image(
            &mut renderer,
            request(&p, root.path(), 1, &cancel),
            &observation,
        )
        .err()
        .unwrap();
    assert!(
        matches!(err,prep::Error::SourceImage{source,..} if matches!(*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Png(_))))
    );
    std::fs::remove_file(&path).unwrap();
    let err = prepared
        .prepare_source_image(
            &mut renderer,
            request(&p, root.path(), 1, &cancel),
            &observation,
        )
        .err()
        .unwrap();
    assert!(
        matches!(err,prep::Error::SourceImage{source,..} if matches!(&*source,motion_host::Error::ImageResource(motion_render::image_resources::Error::Io(e)) if e.kind()==std::io::ErrorKind::NotFound))
    );
    assert!(!observation.running());
    assert_eq!(renderer.image_readback_info().requested_calls, 0);
    assert_eq!(
        renderer.allocation_budget().info().unwrap().admitted_bytes,
        before
    );
    std::fs::write(&path, good).unwrap();
    let recovered = prepared
        .prepare_source_image(
            &mut renderer,
            request(&p, root.path(), 1, &cancel),
            &observation,
        )
        .unwrap()
        .unwrap();
    compare(
        &read(&renderer, recovered.working()),
        &original.cases[0].intermediate,
        &original,
    );
}
