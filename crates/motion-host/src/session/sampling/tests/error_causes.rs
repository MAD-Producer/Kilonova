use super::*;

fn shared(error: &RenderError) -> &Arc<RenderError> {
    match error {
        RenderError::Shared(cause) => cause,
        other => panic!("terminal preview lost original error identity: {other:?}"),
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn exposure_scene_admission_retains_composition_detail_across_terminal_polls_and_recovers() {
    let root = tempfile::tempdir().unwrap();
    let p = project();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&renderer, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    state.synchronize(&p, 23, &mut videos).unwrap();
    let mut invalid = key();
    invalid.frame = f64::from(p.frames);
    let error = poll(&mut state, &mut renderer, &mut scratch, &mut videos,
        &mut proxies, root.path(), invalid.clone(), Duration::from_secs(1)).unwrap_err();
    let original = shared(&error).clone();
    assert!(error.to_string().contains("phase=request"));
    assert!(error.to_string().contains("exposure_scene_init"));
    assert!(crate::Error::from(error).composition_detail().is_some());
    let request = state.request.as_ref().unwrap().id;
    let cpu = state.cpu_us();
    let text = original.to_string();
    for _ in 0..65 {
        let again = poll(&mut state, &mut renderer, &mut scratch, &mut videos,
            &mut proxies, root.path(), invalid.clone(), Duration::from_secs(1)).unwrap_err();
        assert!(Arc::ptr_eq(&original, shared(&again)));
        assert_eq!(again.to_string(), text);
        assert!(crate::Error::from(again).composition_detail().is_some());
    }
    assert_eq!(state.request.as_ref().unwrap().id, request);
    assert_eq!(state.cpu_us(), cpu);
    assert!(state.request.as_ref().unwrap().task.is_none());
    assert_eq!(state.status()["state"], "failed");
    assert_eq!(renderer.exposure_texture_bytes(), 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match poll(&mut state, &mut renderer, &mut scratch, &mut videos,
            &mut proxies, root.path(), key(), Duration::from_secs(1)).unwrap() {
            ExposurePoll::Ready(_) => break,
            ExposurePoll::Pending(_) => assert!(Instant::now() < deadline),
        }
    }
    assert_ne!(state.request.as_ref().unwrap().id, request);
    assert!(state.request.as_ref().unwrap().failure.is_none());
    let pixels = renderer.capture_exposure(state.request.as_ref().unwrap().task.as_ref().unwrap().target()).unwrap();
    assert!(pixels.chunks_exact(4).any(|rgba| rgba[3] != 0));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_corrupt_png_retains_native_decoder_cause_without_replaying_failed_exposure() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("assets")).unwrap();
    let path = root.path().join("assets/image.png");
    image::save_buffer(&path, &[20, 40, 60, 255], 1, 1, image::ColorType::Rgba8).unwrap();
    let mut corrupt = std::fs::read(&path).unwrap();
    let idat = corrupt.windows(4).position(|bytes| bytes == b"IDAT").unwrap();
    let len = u32::from_be_bytes(corrupt[idat - 4..idat].try_into().unwrap()) as usize;
    corrupt[idat + 4 + len] ^= 1;
    std::fs::write(&path, corrupt).unwrap();
    let mut p = project();
    p.assets.push(motion_core::Asset { id: 17, path: "assets/image.png".into(), width: 1, height: 1 });
    p.layers[0].content = motion_core::Content::Image { asset: 17 };
    p.validate().unwrap();
    let before = serde_json::to_value(&p).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    renderer.configure_assets(&p, root.path()).unwrap();
    let mut scratch = PreviewFrameTargets::new(&renderer, 32, 32).unwrap();
    let mut videos = VideoFrames::default();
    let mut proxies = Default::default();
    let mut state = SamplingPreview::default();
    state.synchronize(&p, 24, &mut videos).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let first = loop {
        match poll(&mut state, &mut renderer, &mut scratch, &mut videos,
            &mut proxies, root.path(), key(), Duration::from_secs(1)) {
            Err(error) => break error,
            Ok(ExposurePoll::Ready(_)) => panic!("corrupt PNG was accepted"),
            Ok(ExposurePoll::Pending(_)) => assert!(Instant::now() < deadline),
        }
        std::thread::yield_now();
    };
    let original = shared(&first).clone();
    let mut current = original.as_ref();
    let image = loop {
        match current {
            RenderError::Diagnostic { cause, .. } | RenderError::Context { cause, .. } => current = cause,
            RenderError::Shared(cause) => current = cause,
            RenderError::ImageResource(image) => break image,
            other => panic!("lost native image cause: {other:?}"),
        }
    };
    assert!(matches!(image, motion_render::image_resources::Error::Png(_)));
    assert!(std::error::Error::source(image).is_some());
    assert!(std::error::Error::source(original.as_ref()).is_some());
    let cpu = state.cpu_us();
    for _ in 0..65 {
        let error = poll(&mut state, &mut renderer, &mut scratch, &mut videos,
            &mut proxies, root.path(), key(), Duration::from_secs(1)).unwrap_err();
        assert!(Arc::ptr_eq(&original, shared(&error)));
        assert_eq!(error.to_string(), first.to_string());
    }
    assert_eq!(state.cpu_us(), cpu);
    assert_eq!(serde_json::to_value(&p).unwrap(), before);
    assert_eq!(state.status()["state"], "failed");
    assert!(!scratch.valid());
}
