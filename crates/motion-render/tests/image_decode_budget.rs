use motion_core::Asset;
use motion_render::{
    ResourceBudgetScope, ResourceRejection,
    image_resources::{self as images, DecodeTask, Resolution, Source},
    resource_policy::{DEFAULT_DECODED_IMAGE_BUDGET, MAX_DECODED_IMAGE_BUDGET, MemoryPolicy},
};
use std::{
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

fn source(root: &Path, name: &str, width: u32, height: u32) -> Source {
    Source::new(
        root,
        &Asset {
            id: 17,
            path: format!("assets/{name}"),
            width,
            height,
        },
    )
    .unwrap()
}

fn png(root: &Path) -> Source {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    image::save_buffer(
        root.join("assets/image.png"),
        &[
            255, 0, 0, 255, 255, 255, 255, 0, 0, 0, 255, 255, 0, 0, 255, 255,
        ],
        4,
        1,
        image::ColorType::Rgba8,
    )
    .unwrap();
    source(root, "image.png", 4, 1)
}

fn refusal(error: &images::Error, required: u64, budget: u64) {
    assert_eq!(
        error.resource_rejection(),
        Some(&ResourceRejection::Budget {
            scope: ResourceBudgetScope::DecodedImages,
            required_bytes: required,
            budget_bytes: budget,
        })
    );
}

fn poll(
    task: &mut DecodeTask,
) -> (
    Source,
    Resolution,
    bool,
    Result<images::Pixels, images::Error>,
) {
    let began = Instant::now();
    loop {
        if let Some(result) = task.poll() {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "decode worker never returned"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn explicit_result_limits_preserve_pixels_and_cached_proxies_recheck_the_limit() {
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    let original = std::fs::read(&source.path).unwrap();
    let full = images::decode_with_budget(&source, Resolution::Full, 16).unwrap();
    assert_eq!(
        full.rgba,
        images::decode(&source, Resolution::Full).unwrap().rgba
    );
    assert_eq!(
        full.rgba,
        [255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255]
    );
    refusal(
        &images::decode_with_budget(&source, Resolution::Full, 8)
            .err()
            .unwrap(),
        16,
        8,
    );
    let mut direct = [91; 16];
    refusal(
        &images::decode_into_with_budget(&source, Resolution::Full, &mut direct, 8).unwrap_err(),
        16,
        8,
    );
    assert_eq!(direct, [91; 16]);
    images::decode_into_with_budget(&source, Resolution::Full, &mut direct, 16).unwrap();
    assert_eq!(direct.as_slice(), full.rgba);

    // Streaming PNG proxies require their output capacity, not a full-sized
    // RGBA allocation. The cache must not bypass the current output allowance.
    let proxy = images::decode_with_budget(&source, Resolution::Preview(2), 8).unwrap();
    assert_eq!(proxy.rgba, [188, 0, 0, 128, 0, 0, 255, 255]);
    assert!(!proxy.cached);
    let cached = images::decode_with_budget(&source, Resolution::Preview(2), 8).unwrap();
    assert!(cached.cached);
    assert_eq!(cached.rgba, proxy.rgba);
    refusal(
        &images::decode_with_budget(&source, Resolution::Preview(2), 4)
            .err()
            .unwrap(),
        8,
        4,
    );
    assert_eq!(
        images::decode_with_budget(&source, Resolution::Preview(1), 4)
            .unwrap()
            .rgba
            .len(),
        4
    );
    assert_eq!(std::fs::read(&source.path).unwrap(), original);
}

#[test]
fn malformed_inputs_and_invalid_output_buffers_are_not_capacity_refusals() {
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    let mut wrong = [77; 15];
    let error =
        images::decode_into_with_budget(&source, Resolution::Full, &mut wrong, 4).unwrap_err();
    assert!(matches!(error, images::Error::Invalid(_)));
    assert!(error.resource_rejection().is_none());
    assert_eq!(wrong, [77; 15]);
    let mut mismatch = source.clone();
    mismatch.width += 1;
    let error = images::decode_with_budget(&mismatch, Resolution::Full, 4)
        .err()
        .unwrap();
    assert!(matches!(error, images::Error::Invalid(_)));
    assert!(error.resource_rejection().is_none());
    std::fs::remove_file(&source.path).unwrap();
    let error = images::decode_with_budget(&source, Resolution::Full, 4)
        .err()
        .unwrap();
    assert!(matches!(error, images::Error::Io(_)));
    assert!(error.resource_rejection().is_none());
}

#[test]
fn jpeg_proxy_refuses_full_source_capacity_and_recovers_with_an_explicit_allowance() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    image::save_buffer(
        root.path().join("assets/image.jpg"),
        &[34, 91, 187].repeat(16),
        4,
        4,
        image::ColorType::Rgb8,
    )
    .unwrap();
    let source = source(root.path(), "image.jpg", 4, 4);
    let error = images::decode_with_budget(&source, Resolution::Preview(1), 4)
        .err()
        .unwrap();
    refusal(&error, 64, 4);
    assert!(error.to_string().contains("phase=non_png_source"));
    let expected = images::decode(&source, Resolution::Preview(1)).unwrap();
    let actual = images::decode_with_budget(
        &source,
        Resolution::Preview(1),
        DEFAULT_DECODED_IMAGE_BUDGET,
    )
    .unwrap();
    assert_eq!(actual.rgba, expected.rgba);
    assert_eq!(actual.rgba.len(), 4);
}

#[test]
fn asynchronous_policy_replacement_discards_stale_results_and_then_recovers() {
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    let mut task = DecodeTask::default();
    task.start_with_budget(source.clone(), Resolution::Full, false, 8)
        .unwrap();
    let (returned, resolution, speculative, result) = poll(&mut task);
    assert_eq!(returned, source);
    assert_eq!(resolution, Resolution::Full);
    assert!(!speculative);
    refusal(&result.err().unwrap(), 16, 8);
    assert!(!task.busy());

    task.start_with_budget(source.clone(), Resolution::Full, true, 16)
        .unwrap();
    // Cancellation is checked when delivering a result too, so this does not
    // depend on whether the native decoder happened to finish before the call.
    task.cancel_unwanted_with_budget(&[source.id].into(), Resolution::Full, 8);
    assert!(task.busy());
    assert!(
        task.start_with_budget(source.clone(), Resolution::Full, false, 8)
            .is_err()
    );
    let (_, _, speculative, result) = poll(&mut task);
    assert!(speculative);
    assert!(matches!(result, Err(images::Error::Cancelled)));
    task.start_with_budget(source.clone(), Resolution::Full, false, 16)
        .unwrap();
    assert_eq!(
        poll(&mut task).3.unwrap().rgba,
        images::decode(&source, Resolution::Full).unwrap().rgba
    );
    assert!(!task.busy());
}

#[test]
fn policy_validation_is_atomic_and_cpu_allowance_is_independent_of_gpu_allowance() {
    let mut builder = motion_render::effect_plan::PlanBuilder::new(
        motion_effects::Registry::new_with_builtins().unwrap(),
    )
    .unwrap();
    let policy = MemoryPolicy {
        decoded_image_budget_bytes: 4,
        ..MemoryPolicy::default()
    };
    builder.set_memory_policy(policy).unwrap();
    assert_eq!(builder.memory_policy(), policy);
    assert_eq!(
        policy.source_texture_budget_bytes,
        MemoryPolicy::default().source_texture_budget_bytes
    );
    for budget in [0, 3, MAX_DECODED_IMAGE_BUDGET + 1, u64::MAX] {
        assert!(
            builder
                .set_memory_policy(MemoryPolicy {
                    decoded_image_budget_bytes: budget,
                    ..policy
                })
                .is_err()
        );
        assert_eq!(builder.memory_policy(), policy);
    }
    let mut json = serde_json::to_value(policy).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("decodedImageBudgetBytes");
    assert_eq!(
        serde_json::from_value::<MemoryPolicy>(json)
            .unwrap()
            .decoded_image_budget_bytes,
        DEFAULT_DECODED_IMAGE_BUDGET
    );
}

#[test]
fn configured_full_png_can_cross_the_previous_128_mib_result_limit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let (width, height) = (8192, 4097);
    let mut encoder = png::Encoder::new(
        std::fs::File::create(root.path().join("assets/large.png")).unwrap(),
        width,
        height,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    {
        let mut stream = writer.stream_writer().unwrap();
        let row = [31, 129, 221, 255].repeat(width as usize);
        for _ in 0..height {
            stream.write_all(&row).unwrap();
        }
        stream.finish().unwrap();
    }
    writer.finish().unwrap();
    let source = source(root.path(), "large.png", width, height);
    let required = u64::from(width) * u64::from(height) * 4;
    assert!(required > DEFAULT_DECODED_IMAGE_BUDGET);
    refusal(
        &images::decode(&source, Resolution::Full).err().unwrap(),
        required,
        DEFAULT_DECODED_IMAGE_BUDGET,
    );
    let actual = images::decode_with_budget(&source, Resolution::Full, required).unwrap();
    assert_eq!(actual.rgba.len() as u64, required);
    assert!(!actual.cached);
    assert!(
        actual
            .rgba
            .chunks_exact(4)
            .all(|pixel| pixel == [31, 129, 221, 255])
    );
}
