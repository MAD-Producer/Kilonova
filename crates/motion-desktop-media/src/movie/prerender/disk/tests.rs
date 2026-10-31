use super::*;
use motion_effects::image_contract::ChannelPrecision;
use motion_render::CompositionFrameContext;
mod concurrency;
mod quota_reopen;
mod observations;

fn image(precision: ChannelPrecision) -> CapturedImage {
    let contract = ImageContract::new(
        ImageMeaning::Color {
            space: WorkingSpace::Linear,
            alpha: AlphaMode::Premultiplied,
        },
        precision,
    );
    let pixel = match precision {
        ChannelPrecision::Unorm8 => vec![192, 48, 80, 128],
        ChannelPrecision::Unorm16 => [32768u16, 8192, 16384, 32768]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        // Native half bits for -2, 4, 0.125, 0.5: no conversion through SDR.
        ChannelPrecision::Float16 => [0xc000u16, 0x4400, 0x3000, 0x3800]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect(),
        ChannelPrecision::Float32 => [-2.0f32, 4., 0.125, 0.5]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect(),
    };
    CapturedImage {
        storage: contract.preferred_storage().unwrap(),
        contract,
        width: 2,
        height: 2,
        pixels: pixel.repeat(4),
    }
}
fn context() -> CompositionFrameContext {
    CompositionFrameContext {
        document_generation: 7,
        device_generation: 9,
        content: [3; 32],
        output: [4; 32],
    }
}
fn key(
    image: &CapturedImage,
    context: CompositionFrameContext,
    frame: f64,
    producer: u8,
) -> FrameKey {
    let composition = CompositionFrameKey::new(
        context,
        frame,
        image.width,
        image.height,
        motion_render::image_formats::native_format(image.storage),
    )
    .unwrap();
    FrameKey::new(
        composition,
        image.width,
        image.height,
        image.contract,
        [producer; 32],
    )
    .unwrap()
}
fn fixture(quota: u64, max_frame: u64) -> (tempfile::TempDir, Cache) {
    let temp = tempfile::tempdir().unwrap();
    let config = Config::new(temp.path().join("cache"), quota).unwrap();
    assert!(
        !config.directory.exists(),
        "configuration must not do filesystem IO"
    );
    let cache = Cache::open(config, max_frame, &ExportControl::default()).unwrap();
    (temp, cache)
}

#[test]
fn cold_warm_and_reopened_entries_preserve_all_four_native_storage_bytes() {
    let (_temp, cache) = fixture(1 << 20, 1 << 16);
    let control = ExportControl::default();
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        let image = image(precision);
        let key = key(&image, context(), 1.25, 5);
        assert!(cache.load(&key, &control).unwrap().is_none());
        assert!(cache.store(&key, &image, &control).unwrap());
        let warm = cache.load(&key, &control).unwrap().unwrap();
        assert_eq!(warm.pixels, image.pixels, "{precision:?}");
        assert_eq!(warm.contract, image.contract);
        assert_eq!(warm.storage, image.storage);
        let reopened =
            Cache::open(cache.config().clone(), cache.max_frame_bytes, &control).unwrap();
        assert_eq!(
            reopened.load(&key, &control).unwrap().unwrap().pixels,
            image.pixels
        );
    }
}

#[test]
fn content_output_frame_layout_and_producer_invalidate_but_live_generations_rebind() {
    let (_temp, cache) = fixture(1 << 20, 1 << 16);
    let control = ExportControl::default();
    let image = image(ChannelPrecision::Float32);
    let original = key(&image, context(), 1., 5);
    cache.store(&original, &image, &control).unwrap();
    let mut current = context();
    current.document_generation = 100;
    current.device_generation = 200;
    let rebound = key(&image, current, 1., 5);
    assert_eq!(rebound.name(), original.name());
    assert_eq!(rebound.composition.context(), current);
    assert!(cache.load(&rebound, &control).unwrap().is_some());
    for index in 0..5 {
        let mut changed = current;
        let mut frame = 1.;
        let mut producer = 5;
        let mut changed_image = self::image(ChannelPrecision::Float32);
        match index {
            0 => changed.content[0] ^= 1,
            1 => changed.output[0] ^= 1,
            2 => frame = 1.5,
            3 => producer = 6,
            _ => {
                changed_image.width = 4;
                changed_image.height = 1;
            }
        }
        assert!(cache
            .load(&key(&changed_image, changed, frame, producer), &control)
            .unwrap()
            .is_none());
    }
    let wrong = self::image(ChannelPrecision::Unorm8);
    assert!(FrameKey::new(
        original.composition,
        wrong.width,
        wrong.height,
        wrong.contract,
        [5; 32]
    )
    .is_err());
}

#[test]
fn truncated_corrupt_wrong_version_identity_and_trailing_entries_are_errors_then_regenerable() {
    let (_temp, cache) = fixture(1 << 20, 1 << 16);
    let control = ExportControl::default();
    let image = image(ChannelPrecision::Float16);
    let key = key(&image, context(), 2., 5);
    for defect in 0..6 {
        cache.store(&key, &image, &control).unwrap();
        let path = cache.directory.join(key.name());
        let mut bytes = fs::read(&path).unwrap();
        match defect {
            0 => {
                bytes.truncate(12);
            }
            1 => bytes[0] ^= 1,
            2 => bytes[8] ^= 1,
            3 => bytes[40] ^= 1,
            4 => bytes[HEADER_BYTES as usize] ^= 1,
            _ => bytes.push(1),
        }
        fs::write(&path, bytes).unwrap();
        assert!(cache.load(&key, &control).is_err(), "defect {defect}");
        cache.store(&key, &image, &control).unwrap();
        assert_eq!(
            cache.load(&key, &control).unwrap().unwrap().pixels,
            image.pixels
        );
    }
}

#[test]
fn quota_eviction_only_removes_owned_entries_and_never_project_data() {
    let (temp, cache) = fixture(202, 1024);
    let control = ExportControl::default();
    let image = image(ChannelPrecision::Unorm8);
    let project = temp.path().join("project.msproj");
    fs::write(&project, b"project original").unwrap();
    let unknown = cache.directory.join("keep.txt");
    fs::write(&unknown, [9; 10]).unwrap();
    for frame in [0., 1., 2.] {
        assert!(cache
            .store(&key(&image, context(), frame, 5), &image, &control)
            .unwrap());
    }
    let mut bytes = 0;
    let mut owned = 0;
    for entry in fs::read_dir(&cache.directory).unwrap() {
        let entry = entry.unwrap();
        bytes += entry.metadata().unwrap().len();
        owned += usize::from(owned_name(&entry.file_name().to_string_lossy()));
    }
    assert_eq!(owned, 2);
    assert!(bytes <= cache.config.quota_bytes());
    assert_eq!(fs::read(unknown).unwrap(), vec![9; 10]);
    assert_eq!(fs::read(project).unwrap(), b"project original");
    assert!(cache
        .load(&key(&image, context(), 2., 5), &control)
        .unwrap()
        .is_some());
}

#[test]
fn cancelled_and_injected_partial_write_failures_publish_nothing_and_preserve_primary_cause() {
    let (temp, cache) = fixture(4 << 20, 2 << 20);
    let control = ExportControl::default();
    let mut image = image(ChannelPrecision::Unorm8);
    image.width = 256;
    image.height = 256;
    image.pixels = vec![128; 256 * 256 * 4];
    let key = key(&image, context(), 3., 5);
    let project = temp.path().join("project.msproj");
    fs::write(&project, b"original project").unwrap();
    let error = cache
        .store_with(&key, &image, &control, |index, _| {
            if index == 1 {
                Err(format!(
                    "owned injected write failure: {}",
                    std::io::Error::from(std::io::ErrorKind::StorageFull)
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
    assert!(error.contains("owned injected write failure"), "{error}");
    assert!(cache.load(&key, &control).unwrap().is_none());
    assert_eq!(
        fs::read_dir(&cache.directory).unwrap().count(),
        0,
        "temporary file must be dropped on error"
    );
    let cancelled = ExportControl::default();
    assert_eq!(
        cache
            .store_with(&key, &image, &cancelled, |index, control| {
                if index == 1 {
                    control.cancel();
                }
                Ok(())
            })
            .unwrap_err(),
        "movie export cancelled"
    );
    assert_eq!(fs::read_dir(&cache.directory).unwrap().count(), 0);
    assert_eq!(fs::read(project).unwrap(), b"original project");
}

#[test]
fn oversized_frames_and_unowned_quota_occupation_do_not_delete_other_files() {
    let (_temp, cache) = fixture(100, 8);
    let control = ExportControl::default();
    let image = image(ChannelPrecision::Float32);
    let key = key(&image, context(), 1., 5);
    assert!(!cache.store(&key, &image, &control).unwrap());
    assert!(cache.load(&key, &control).unwrap().is_none());
    let (_temp, cache) = fixture(100, 1024);
    let image = self::image(ChannelPrecision::Unorm8);
    let key = self::key(&image, context(), 2., 5);
    let foreign = cache.directory.join("unowned");
    fs::write(&foreign, [1; 100]).unwrap();
    assert!(cache
        .store(&key, &image, &control)
        .unwrap_err()
        .contains("unowned entries"));
    assert_eq!(fs::read(foreign).unwrap(), vec![1; 100]);
    assert!(Config::new(PathBuf::from("relative"), 1024).is_err());
    assert!(Config::new(cache.directory.clone(), 0).is_err());
    assert!(Config::new(cache.directory.clone(), MAX_QUOTA + 1).is_err());
}
