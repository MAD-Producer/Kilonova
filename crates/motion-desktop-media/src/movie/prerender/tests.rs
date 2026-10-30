use super::*;
use motion_core::{Asset, Content, Ease, Layer};
use motion_effects::image_contract::{ChannelPrecision, ImageStorage};

fn frozen(project: Project, root: &Path, revision: u64) -> FrozenMovie {
    FrozenMovie::new(
        project,
        root,
        Registry::new_with_builtins().unwrap(),
        96 << 20,
        revision,
    )
    .unwrap()
}
fn image_project(root: &Path, color: [u8; 4]) -> Project {
    fs::create_dir_all(root.join("assets")).unwrap();
    image::save_buffer(
        root.join("assets/image.png"),
        &color,
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut p = Project::new(32, 24, 30, 6).unwrap();
    p.background = [0., 0., 0., 1.];
    p.assets.push(Asset {
        id: 17,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    let mut layer = Layer::solid(1, "Owned image", [8.; 2], [6., 12., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 17 };
    for (frame, x) in [(0, 6.), (5, 26.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 12., 0.], Ease::Linear)
            .unwrap();
    }
    p.layers.push(layer);
    p
}

#[test]
fn owner_capture_defers_missing_source_io_until_published_background_freeze() {
    let root = tempfile::tempdir().unwrap();
    let project = image_project(root.path(), [255, 0, 0, 255]);
    let session = Session::new(project, root.path().into(), crate::platform()).unwrap();
    fs::remove_file(root.path().join("assets/image.png")).unwrap();
    let control = ExportControl::default();
    let plan = PrerenderPlan::capture(&session, 7, 9, 82, 1 << 20, &control).unwrap();
    let observed = std::thread::spawn(move || {
        let mut phases = Vec::new();
        let cause = plan
            .prepare(&control, |phase| phases.push(phase))
            .err()
            .unwrap();
        (phases, cause)
    })
    .join()
    .unwrap();
    assert_eq!(observed.0, [Phase::Freeze]);
    assert!(observed.1.contains("assets/image.png"), "{}", observed.1);
    // Missing bytes never become a successful black frame or a content key.
    image_project(root.path(), [255, 0, 0, 255]);
    let input = PrerenderPlan::capture(&session, 7, 9, 83, 1 << 20, &ExportControl::default())
        .unwrap()
        .prepare(&ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!(input.context().document_generation, 7);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn independent_native_device_uses_the_exact_foreground_adapter_handle() {
    let root = tempfile::tempdir().unwrap();
    let project = image_project(root.path(), [255, 0, 0, 255]);
    let session = Session::new(project, root.path().into(), crate::platform()).unwrap();
    let foreground = pollster::block_on(Renderer::headless_with_working_precision(
        ChannelPrecision::Unorm8,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let adapter = foreground.adapter.clone();
    let device = foreground.device.clone();
    let queue = foreground.queue.clone();
    let mut plan =
        PrerenderPlan::capture(&session, 7, 9, 82, 1 << 20, &ExportControl::default()).unwrap();
    // A headless Session has no Graphics; supply its actual foreground adapter
    // here. Window-backed capture takes this same handle from Graphics.
    plan.adapter = Some(adapter.clone());
    drop(session);
    let image = std::thread::spawn(move || {
        let prepared = plan.prepare(&ExportControl::default(), |_| {}).unwrap();
        let mut runtime = NativePrerender::open(prepared, &ExportControl::default()).unwrap();
        assert_eq!(runtime.renderer.adapter, adapter);
        assert_ne!(runtime.renderer.device, device);
        assert_ne!(runtime.renderer.queue, queue);
        runtime
            .render(0., &ExportControl::default(), |_| {})
            .unwrap()
    })
    .join()
    .unwrap();
    let at = |x: usize| image.image().pixels[(12 * 32 + x) * 4];
    assert!(at(6) > 240 && at(26) < 10);
}
#[test]
fn keys_hash_actual_owned_sources_and_ignore_live_paths_revision_and_export_scope() {
    fn transferable<T: Send>() {}
    transferable::<PrerenderPlan>();
    transferable::<PreparedPrerender>();
    transferable::<CompletedFrame>();
    let root = tempfile::tempdir().unwrap();
    let p = image_project(root.path(), [255, 0, 0, 255]);
    let red = frozen(p.clone(), root.path(), 1);
    let mut red_again =
        frozen(p.clone(), root.path(), 99).with_scope(motion_core::FrameScope::WorkArea);
    red_again.range = motion_core::WorkArea {
        in_frame: 1,
        out_frame: 3,
    };
    assert_ne!(red.range, red_again.range);
    let before = key::context(&red, 10, 20, &ExportControl::default()).unwrap();
    assert_eq!(
        before,
        key::context(&red_again, 10, 20, &ExportControl::default()).unwrap()
    );
    image::save_buffer(
        root.path().join("assets/image.png"),
        &[0, 0, 255, 255],
        1,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let blue = frozen(p, root.path(), 1);
    assert_ne!(
        before.content,
        key::context(&blue, 10, 20, &ExportControl::default())
            .unwrap()
            .content
    );
    fs::remove_file(root.path().join("assets/image.png")).unwrap();
    assert_eq!(
        before,
        key::context(&red, 10, 20, &ExportControl::default()).unwrap()
    );
    let prepared = PreparedPrerender::new(red, 10, 20, 1 << 20, &ExportControl::default()).unwrap();
    assert_eq!(
        prepared.frame_key(-0.).unwrap(),
        prepared.frame_key(0.).unwrap()
    );
    assert_ne!(
        prepared.frame_key(0.25).unwrap(),
        prepared.frame_key(0.5).unwrap()
    );
    for frame in [f64::NAN, f64::INFINITY, -1., 6.] {
        assert!(prepared.frame_key(frame).is_err());
    }
    assert_ne!(
        before,
        key::context(&blue, 11, 20, &ExportControl::default()).unwrap()
    );
}
#[test]
fn key_and_cpu_frame_admission_cancel_before_gpu_or_publication() {
    let root = tempfile::tempdir().unwrap();
    let p = Project::new(32, 24, 30, 6).unwrap();
    let input = frozen(p.clone(), root.path(), 1);
    let cancelled = ExportControl::default();
    assert!(cancelled.cancel());
    assert!(key::context(&input, 1, 1, &cancelled)
        .unwrap_err()
        .contains("cancelled"));
    let error = PreparedPrerender::new(input, 1, 1, 32 * 24 * 4 - 1, &ExportControl::default())
        .err()
        .unwrap();
    assert!(error.contains("CPU bytes"), "{error}");
    let input = PreparedPrerender::new(
        frozen(p, root.path(), 1),
        1,
        1,
        32 * 24 * 4,
        &ExportControl::default(),
    )
    .unwrap();
    assert!(NativePrerender::open(input, &cancelled)
        .err()
        .unwrap()
        .contains("cancelled"));
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_background_worker_renders_owned_animated_sources_after_session_close() {
    let root = tempfile::tempdir().unwrap();
    let p = image_project(root.path(), [255, 0, 0, 255]);
    let session = Session::new(p, root.path().into(), crate::platform()).unwrap();
    let control = ExportControl::default();
    let input = PrerenderPlan::capture(&session, 7, 9, 82, 1 << 20, &control)
        .unwrap()
        .prepare(&control, |_| {})
        .unwrap();
    let context = input.context();
    let revision = session.engine.revision();
    let frame = session.frame;
    let snapshot = session.engine.snapshot();
    fs::remove_file(root.path().join("assets/image.png")).unwrap();
    let worker = std::thread::spawn(move || {
        let mut runtime = NativePrerender::open(input, &ExportControl::default()).unwrap();
        assert_eq!(runtime.context(), context);
        let first = runtime
            .render(0., &ExportControl::default(), |_| {})
            .unwrap();
        let last = runtime
            .render(5., &ExportControl::default(), |_| {})
            .unwrap();
        assert_eq!(runtime.readback_info().completed_calls, 2);
        (first, last)
    });
    // The renderer lives on the independent worker, while the owner can close.
    assert_eq!(session.engine.snapshot(), snapshot);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.frame, frame);
    drop(session);
    let (first, last) = worker.join().unwrap();
    let at = |image: &CapturedImage, x: usize| image.pixels[(12 * 32 + x) * 4];
    assert_eq!(first.image().storage, ImageStorage::Rgba8UnormSrgb);
    assert!(at(first.image(), 6) > 240 && at(first.image(), 26) < 10);
    assert!(at(last.image(), 6) < 10 && at(last.image(), 26) > 240);
    assert_ne!(first.key(), last.key());
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cancellation_at_actual_draw_and_readback_phases_never_publishes_and_worker_recovers() {
    let root = tempfile::tempdir().unwrap();
    let p = image_project(root.path(), [255, 0, 0, 255]);
    let input = PreparedPrerender::new(
        frozen(p, root.path(), 1),
        1,
        1,
        1 << 20,
        &ExportControl::default(),
    )
    .unwrap();
    let mut runtime = NativePrerender::open(input, &ExportControl::default()).unwrap();
    for stop in [Phase::Draw, Phase::Readback] {
        let control = ExportControl::default();
        let mut reached = false;
        let error = runtime
            .render(0.25, &control, |phase| {
                if phase == stop {
                    reached = true;
                    assert!(control.cancel());
                }
            })
            .err()
            .unwrap();
        assert!(reached && error.contains("cancelled"), "{error}");
        assert_eq!(runtime.readback_info().requested_calls, 0);
    }
    let complete = runtime
        .render(0.25, &ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!(complete.image().pixels.len(), 32 * 24 * 4);
    assert_eq!(runtime.readback_info().completed_calls, 1);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn background_renderer_obeys_the_same_live_allocation_budget_and_recovers() {
    let root = tempfile::tempdir().unwrap();
    let p = image_project(root.path(), [255, 0, 0, 255]);
    let input = PreparedPrerender::new(
        frozen(p, root.path(), 1),
        1,
        1,
        1 << 20,
        &ExportControl::default(),
    )
    .unwrap();
    let budget = input.frozen.allocation_budget.clone();
    let limit = budget.info().unwrap().budget_bytes;
    let mut runtime = NativePrerender::open(input, &ExportControl::default()).unwrap();
    assert!(budget.info().unwrap().admitted_bytes > 0);
    budget.set_limit(1).unwrap();
    let error = runtime
        .render(0., &ExportControl::default(), |_| {})
        .err()
        .unwrap();
    assert!(error.contains("budget"), "{error}");
    assert_eq!(runtime.readback_info().completed_calls, 0);
    budget.set_limit(limit).unwrap();
    let output = runtime
        .render(0., &ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!(output.image().pixels.len(), 32 * 24 * 4);
    assert_eq!(runtime.readback_info().completed_calls, 1);
    runtime
        .renderer
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .unwrap();
    drop(runtime);
    // Dropping the independent device waits for its submitted work and releases
    // its retained allocation leases; the CPU completion holds no GPU handle.
    assert_eq!(budget.info().unwrap().admitted_bytes, 0);
}

#[path = "../../../../motion-render/tests/support/sdr_fixture.rs"]
mod fixture;
#[test]
#[ignore = "explicit four-precision native prerender acceptance requires optional GPU features"]
fn all_four_precisions_keep_native_storage_and_existing_exposure_policies() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for exposure in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let (p, registry) = fixture::project(precision, exposure);
            let frozen = FrozenMovie::new(p, root.path(), registry, 96 << 20, 7).unwrap();
            let input =
                PreparedPrerender::new(frozen, 1, 1, 1 << 20, &ExportControl::default()).unwrap();
            let mut runtime = NativePrerender::open(input, &ExportControl::default()).unwrap();
            let output = runtime
                .render(2.25, &ExportControl::default(), |_| {})
                .unwrap();
            let image = output.image();
            assert_eq!(image.contract.precision, precision);
            assert_eq!(
                image.storage,
                motion_render::image_formats::color_storage(precision)
            );
            assert_eq!(
                image.pixels.len() as u64,
                image.storage.capacity(18, 18).unwrap()
            );
            let expected = match precision {
                // The existing UNORM8 exposure resolve clamps straight RGB to
                // [0,1] and repremultiplies by alpha before storage. Red becomes
                // 64/255 linear, encoded as 137; ordinary rendering saturates
                // red to 255. Blue 0.125 stores as 99 in both paths. Preserve
                // those existing policies; raw readback adds no conversion.
                ChannelPrecision::Unorm8 if exposure => vec![137, 0, 99, 64],
                ChannelPrecision::Unorm8 => vec![255, 0, 99, 64],
                ChannelPrecision::Unorm16 => [65535u16, 0, 8192, 16384]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                ChannelPrecision::Float16 => [0x4000u16, 0xb000, 0x3000, 0x3400]
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
                ChannelPrecision::Float32 => [2f32, -0.125, 0.125, 0.25]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
            };
            // Uniform authored shader; inspect every native pixel. Binary-exact
            // powers of two also make the exposure average independently exact.
            for pixel in image.pixels.chunks_exact(expected.len()) {
                assert_eq!(pixel, expected, "{precision:?} exposure={exposure}");
            }
        }
    }
}
