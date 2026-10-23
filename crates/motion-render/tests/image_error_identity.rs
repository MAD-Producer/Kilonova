use motion_render::image_resources::{self as images, DecodeTask, Resolution, Source};
use std::{
    any::Any,
    fmt::Debug,
    time::{Duration, Instant},
};

fn native<T: Debug + 'static>(error: &T) {
    assert!(
        (error as &dyn Any).downcast_ref::<String>().is_none(),
        "image failure lost native identity: {error:?}"
    );
}

fn png(root: &std::path::Path) -> Source {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let path = root.join("assets/image.png");
    image::save_buffer(&path, &[20, 40, 60, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    Source::new(
        root,
        &motion_core::Asset {
            id: 17,
            path: "assets/image.png".into(),
            width: 1,
            height: 1,
        },
    )
    .unwrap()
}

#[test]
fn missing_file_inspection_preserves_native_error_and_recovers() {
    let root = tempfile::tempdir().unwrap();
    let error = images::inspect(&root.path().join("image.png")).unwrap_err();
    native(&error);
    let source = png(root.path());
    assert_eq!(images::inspect(&source.path).unwrap().0, 1);
}

#[test]
fn invalid_registered_path_preserves_native_model_error() {
    let root = tempfile::tempdir().unwrap();
    let error = Source::new(
        root.path(),
        &motion_core::Asset {
            id: 17,
            path: "../outside.png".into(),
            width: 1,
            height: 1,
        },
    )
    .unwrap_err();
    native(&error);
    assert!(!root.path().join("outside.png").exists());
    assert_eq!(png(root.path()).id, 17);
}

#[test]
fn asynchronous_missing_file_preserves_native_error_and_worker_recovers() {
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    std::fs::remove_file(&source.path).unwrap();
    let mut task = DecodeTask::default();
    task.start(source.clone(), Resolution::Full, false).unwrap();
    let begin = Instant::now();
    let error = loop {
        if let Some((returned, mode, speculative, result)) = task.poll() {
            assert_eq!(returned, source);
            assert_eq!(mode, Resolution::Full);
            assert!(!speculative);
            break result.err().expect("deleted file must fail");
        }
        assert!(begin.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    };
    native(&error);
    assert!(!task.busy());
    let source = png(root.path());
    task.start(source, Resolution::Full, false).unwrap();
    let begin = Instant::now();
    loop {
        if let Some((_, _, _, result)) = task.poll() {
            assert_eq!(result.unwrap().rgba, [20, 40, 60, 255]);
            break;
        }
        assert!(begin.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
}

#[test]
fn native_sources_distinguish_io_model_and_corrupt_png() {
    use images::Error;
    let root = tempfile::tempdir().unwrap();
    let missing = images::inspect(&root.path().join("absent.png")).unwrap_err();
    assert!(matches!(&missing, Error::Io(cause) if cause.kind() == std::io::ErrorKind::NotFound));
    let invalid = Source::new(
        root.path(),
        &motion_core::Asset {
            id: 17,
            path: "../outside.png".into(),
            width: 1,
            height: 1,
        },
    )
    .unwrap_err();
    assert!(matches!(&invalid, Error::Model(_)));

    let header = root.path().join("truncated.png");
    std::fs::write(&header, b"\x89PNG\r\n\x1a\n").unwrap();
    let error = images::inspect(&header).unwrap_err();
    assert!(matches!(error, Error::Image(_)));

    let source = png(root.path());
    let healthy = std::fs::read(&source.path).unwrap();
    let mut corrupt = healthy.clone();
    let idat = corrupt
        .windows(4)
        .position(|bytes| bytes == b"IDAT")
        .unwrap();
    let length = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + length] ^= 1; // Real decoder CRC refusal, valid dimensions.
    std::fs::write(&source.path, &corrupt).unwrap();
    assert_eq!(images::inspect(&source.path).unwrap().0, 1);
    let error = images::decode(&source, Resolution::Full).err().unwrap();
    assert!(matches!(&error, Error::Png(_)), "{error:?}");
    std::fs::write(&source.path, healthy).unwrap();
    assert_eq!(
        images::decode(&source, Resolution::Full).unwrap().rgba,
        [20, 40, 60, 255]
    );
}

#[test]
fn cancellation_has_identity_and_external_text_cannot_spoof_it() {
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    let mut task = DecodeTask::default();
    task.start(source, Resolution::Full, false).unwrap();
    task.cancel();
    let begin = Instant::now();
    loop {
        if let Some((_, _, _, result)) = task.poll() {
            assert!(matches!(result, Err(images::Error::Cancelled)));
            assert!(!task.busy());
            break;
        }
        assert!(begin.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    let text: images::Error = "image decode superseded".into();
    assert!(matches!(text, images::Error::Invalid(_)));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn renderer_keeps_image_sources_on_configure_sync_and_async_failures() {
    use motion_render::{RenderError, Renderer, Scene};
    let root = tempfile::tempdir().unwrap();
    let source = png(root.path());
    let healthy = std::fs::read(&source.path).unwrap();
    let mut corrupt = healthy.clone();
    let idat = corrupt.windows(4).position(|b| b == b"IDAT").unwrap();
    let length = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + length] ^= 1;
    let mut project = motion_core::Project::new(32, 32, 30, 90).unwrap();
    project.assets.push(motion_core::Asset {
        id: source.id,
        path: "assets/image.png".into(),
        width: 1,
        height: 1,
    });
    let mut layer = motion_core::Layer::solid(1, "Image", [1.; 2], [0.; 3], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: source.id };
    project.layers.push(layer);
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&project, root.path()).unwrap();
    let catalog = renderer.asset_ids().to_vec();
    let count = renderer.image_count();
    let mut invalid = project.clone();
    invalid.assets.push(motion_core::Asset {
        id: 18,
        path: "assets/absent.png".into(),
        width: 1,
        height: 1,
    });
    let error = renderer
        .configure_assets(&invalid, root.path())
        .unwrap_err();
    assert!(
        matches!(error, RenderError::ImageResource(images::Error::Io(cause)) if cause.kind() == std::io::ErrorKind::NotFound)
    );
    assert_eq!(renderer.asset_ids(), catalog);
    assert_eq!(renderer.image_count(), count);
    std::fs::write(&source.path, corrupt).unwrap();
    let error = renderer
        .prepare_scene_assets(&scene, Resolution::Full, false)
        .unwrap_err();
    assert!(
        matches!(error, RenderError::Diagnostic { cause, .. } if matches!(cause.as_ref(), RenderError::ImageResource(images::Error::Png(_))))
    );
    let began = Instant::now();
    loop {
        match renderer.prepare_scene_assets(&scene, Resolution::Full, true) {
            Err(error) => {
                assert!(
                    matches!(error, RenderError::Diagnostic { cause, .. } if matches!(cause.as_ref(), RenderError::ImageResource(images::Error::Png(_))))
                );
                break;
            }
            Ok(pending) => assert!(!pending),
        }
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    assert_eq!(renderer.image_count(), count);
    std::fs::write(&source.path, healthy).unwrap();
    let began = Instant::now();
    while !renderer
        .prepare_scene_assets(&scene, Resolution::Full, true)
        .unwrap()
    {
        assert!(began.elapsed() < Duration::from_secs(5));
        std::thread::yield_now();
    }
    assert_eq!(renderer.image_dimensions(source.id), Some((1, 1)));
    assert_eq!(renderer.image_decodes, 1);
    assert_eq!(renderer.asset_ids(), catalog);
}
