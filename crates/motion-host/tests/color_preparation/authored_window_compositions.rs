//! Actual surface activation changes the source set atomically.
use super::super::compositions::{body, open, state};
use super::*;

fn qualify() {
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let reference = reference();
    let runtime = NativeColorRuntime::load(&runtime()).unwrap();
    let sample = [32768u16, 16384, 8192, 65535];
    let raw = sample.map(|v| f32::from(v) / 65535.);
    let mut checked = 0;
    for case in reference
        .cases
        .iter()
        .filter(|c| c.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        println!("composition phase=case config={} input={:?}",case.config,case.processor.input_alpha);
        let root = tempfile::tempdir().unwrap();
        let mut project = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut project).display.transfer = ColorTransfer::Srgb;
        project.layers.push(Layer::solid(
            1,
            "main",
            [64.; 2],
            [32., 32., 0.],
            case.input[3],
        ));
        let main = project.composition_id.clone();
        let mut target = body(&project, "comp-image");
        target.layers[0].id = 2;
        target.layers[0].content = Content::Image { asset: 1 };
        project.compositions.push(target);
        project.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/target.png".into(),
            width: 2,
            height: 2,
        });
        std::fs::create_dir_all(root.path().join("assets")).unwrap();
        let path = root.path().join("assets/target.png");
        let write = || {
            image::ImageBuffer::<image::Rgba<u16>, _>::from_raw(2, 2, sample.repeat(4))
                .unwrap()
                .save(&path)
                .unwrap()
        };
        write();
        let config = &reference.configs[case.config];
        let verified = VerifiedColorConfig::new(
            config.manifest.clone(),
            Arc::from(config.config.as_bytes()),
            BTreeMap::new(),
        )
        .unwrap();
        let input = runtime
            .prepare(
                &verified,
                &config.selection,
                ColorProcessorPhase::InputToWorking,
                case.processor.input_alpha,
                AlphaMode::Premultiplied,
            )
            .unwrap();
        let display = runtime
            .prepare(
                &verified,
                &config.selection,
                ColorProcessorPhase::WorkingToDisplay,
                AlphaMode::Premultiplied,
                AlphaMode::Premultiplied,
            )
            .unwrap();
        let rgb = display
            .apply_rgb(input.apply_rgb([raw[0], raw[1], raw[2]]).unwrap())
            .unwrap();
        let target_pixel =
            [rgb[0], rgb[1], rgb[2], 1.].map(|v| (v.clamp(0., 1.) * 255.).round() as u8);
        let main_pixel = expected(&reference, case, &runtime)[0];
        assert!(
            (0..3).any(|c| main_pixel[c].abs_diff(target_pixel[c]) > 2),
            "stale source negative control must be visible"
        );
        let mut session = Session::new(
            project.clone(),
            root.path().to_owned(),
            Arc::new(session_installation::NativePlatform(super::super::runtime())),
        )
        .unwrap();
        println!("composition phase=session_ready");
        attach(&mut session, &events);
        let adapter = &session.graphics.as_ref().unwrap().renderer.adapter_info;
        println!("managed composition adapter={adapter:?}");
        match std::env::var("WGPU_BACKEND").as_deref() {
            Ok("vulkan") => assert_eq!(adapter.backend, wgpu::Backend::Vulkan),
            Ok("dx12") => assert_eq!(adapter.backend, wgpu::Backend::Dx12),
            _ => {}
        }
        let previous = capture(&mut session, 0.5, &views(1), false);
        std::fs::remove_file(
            root.path()
                .join(&project.color_management.resources().unwrap()[0].path),
        )
        .unwrap();
        let history = session.engine.history_diagnostics();
        std::fs::remove_file(&path).unwrap();
        let before = state(&session);
        let error = open(&mut session, "comp-image").unwrap_err();
        let motion_host::Error::ColorPreparation(prep::Error::SurfaceDisplay(source)) = &error
        else {
            panic!("{error:?}")
        };
        let motion_host::Error::ColorPreparation(prep::Error::SourceImage {
            asset: 1,
            phase: "resolve_source",
            source,
            ..
        }) = source.as_ref()
        else {
            panic!("{error:?}")
        };
        let motion_host::Error::ImageResource(motion_render::image_resources::Error::Io(source)) =
            source.as_ref()
        else {
            panic!("{error:?}")
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(state(&session), before);
        assert_eq!(capture(&mut session, 0.5, &views(1), true), previous);
        write();
        let policy = session.graphics.as_ref().unwrap().renderer.memory_policy();
        let mut limited = policy;
        limited.preview_target_budget_bytes = 32;
        session.configure_memory_policy(limited).unwrap();
        let before = state(&session);
        for _ in 0..2 {
            let error = open(&mut session, "comp-image").unwrap_err();
            let motion_host::Error::ColorPreparation(prep::Error::SurfaceDisplay(source)) = &error
            else {
                panic!("{error:?}")
            };
            assert!(
                matches!(
                    source.as_ref(),
                    motion_host::Error::Render(motion_render::RenderError::Resource { .. })
                ),
                "{error:?}"
            );
            assert_eq!(state(&session), before);
        }
        session.configure_memory_policy(policy).unwrap();
        assert_eq!(capture(&mut session, 0.5, &views(1), false), previous);
        for count in [1, 2, 4] {
            println!("composition phase=switch count={count}");
            open(&mut session, "comp-image").unwrap();
            checked += check(
                &capture(&mut session, 0.5, &views(count), false),
                count,
                [target_pixel; 2],
            );
            open(&mut session, &main).unwrap();
            checked += check(
                &capture(&mut session, 0.5, &views(count), false),
                count,
                [main_pixel; 2],
            );
        }
        assert_eq!(session.engine.history_diagnostics(), history);
        println!("composition phase=case_done");
    }
    assert_eq!(checked, 107_520);
    println!("managed composition surface CPU pixels={checked}; six native inputs; 1/2/4 views; missing source and budget refusal preserve original composition/front; retained config; explicit recovery");
}

#[test]
#[ignore = "requires actual Windows surfaces, GPU and pinned native OCIO runtime"]
fn managed_composition_surface_activation_preserves_failed_sources_and_budget() {
    const TEST:&str="authored::window::compositions::managed_composition_surface_activation_preserves_failed_sources_and_budget";
    const CHILD: &str = "MOTION_MANAGED_COMPOSITIONS_CHILD";
    if std::env::var_os(CHILD).is_some() {
        qualify();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdout(std::fs::File::create(root.path().join("stdout")).unwrap())
        .stderr(std::fs::File::create(root.path().join("stderr")).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let waited = child.wait();
            panic!("composition surface exceeded 120s: kill={killed:?} wait={waited:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = std::fs::read_to_string(root.path().join("stdout")).unwrap();
    let stderr = std::fs::read_to_string(root.path().join("stderr")).unwrap();
    if let Some(dir) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let dir = std::path::PathBuf::from(dir).join("managed-composition-window");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("stdout"), &stdout).unwrap();
        std::fs::write(dir.join("stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(
        stdout.contains("composition surface CPU pixels=107520"),
        "{stdout}"
    );
}
