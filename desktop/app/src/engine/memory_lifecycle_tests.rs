//! Actual serial worker, native surfaces and readbacks; no application input driver.
use super::*;
use motion_core::{Asset, Content, Ease, Layer, Project};
use motion_desktop_media::movie::ExportControl;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn reply<T>(
    events: &mut EventLoop<()>,
    engine: &Engine,
    command: impl FnOnce(Sender<Result<T, String>>) -> Command,
) -> T {
    let result = engine.request(command).unwrap();
    crate::native_test_support::receive(events, &result, Duration::from_secs(30))
        .unwrap()
        .unwrap()
}
fn ui(size: u32) -> UiFrame {
    UiFrame {
        paint: Default::default(),
        // CaptureUi requires a complete atlas even for a glyph-free frame.
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [size as f32; 2],
        composition: Some([0., 0., size as f32, size as f32]),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}
fn present(events: &mut EventLoop<()>, engine: &Engine, size: u32) -> u128 {
    let began = Instant::now();
    loop {
        if reply(events, engine, |reply| Command::Present {
            ui: ui(size),
            reply,
        }) {
            break;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "preview replacement never completed"
        );
    }
    began.elapsed().as_micros()
}
fn pixels(path: &std::path::Path) -> ([u32; 2], [u8; 4]) {
    let image = image::open(path).unwrap().to_rgba8();
    (
        [image.width(), image.height()],
        image.get_pixel(image.width() / 2, image.height() / 2).0,
    )
}
fn fixture(root: &std::path::Path, name: &str, color: [u8; 4]) -> (PathBuf, Project) {
    let directory = root.join(name);
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    image::save_buffer(
        directory.join("assets/source.png"),
        &color.repeat(256 * 64),
        256,
        64,
        image::ColorType::Rgba8,
    )
    .unwrap();
    let mut project = Project::new(64, 64, 30, 30).unwrap();
    project.background = [0., 0., 0., 1.];
    project.assets.push(Asset {
        id: 1,
        path: "assets/source.png".into(),
        width: 256,
        height: 64,
    });
    let mut layer = Layer::solid(
        1,
        "Generated lifecycle image",
        [64.; 2],
        [32., 32., 0.],
        [1.; 4],
    );
    layer.content = Content::Image { asset: 1 };
    for (frame, opacity) in [(0, 1.), (15, 0.5), (29, 1.)] {
        layer
            .transform
            .opacity
            .upsert(frame, opacity, Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    motion_core::storage::save(&directory, &project).unwrap();
    (directory, project)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn repeated_native_resize_seek_and_project_switch_retire_capacity_without_pixel_or_history_drift() {
    const NAME:&str="engine::memory_lifecycle_tests::repeated_native_resize_seek_and_project_switch_retire_capacity_without_pixel_or_history_drift";
    const CHILD: &str = "MOTION_MEMORY_LIFECYCLE_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_MEMORY_LIFECYCLE_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            std::fs::create_dir_all(&destination).unwrap();
            for name in ["report.json", "motion-studio.log"] {
                let source = root.path().join(name);
                if source.exists() {
                    std::fs::copy(source, destination.join(name)).unwrap();
                }
            }
            std::fs::write(destination.join("stdout.log"), &output.stdout).unwrap();
            std::fs::write(destination.join("stderr.log"), &output.stderr).unwrap();
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        assert!(log.contains("start #"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("logging initialized"));
        return;
    };
    let root = PathBuf::from(directory);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let projects = [
        fixture(&root, "red", [255, 0, 0, 255]),
        fixture(&root, "blue", [0, 0, 255, 255]),
    ];
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(96, 96)),
            )
            .unwrap(),
    );
    let engine = Engine::start(
        projects[0].0.clone(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let attached = engine.attach_async(window.clone(), 96, 96).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mut policy = motion_render::resource_policy::MemoryPolicy::default();
    policy.session_allocation_budget_bytes = 8 << 20;
    reply(&mut events, &engine, |reply| Command::Plugin {
        request: serde_json::json!({"op":"memory_policy","policy":policy}).to_string(),
        reply,
    });
    // One warmup cycle, one baseline cycle, then two repeated verification cycles.
    // Reusing the same asset id and relative path across roots catches stale input
    // identity as well as leaked ownership after project replacement.
    let cases = [
        (0, 96, 1.),
        (1, 128, 10.),
        (0, 80, 20.),
        (1, 96, 1.),
        (0, 128, 10.),
        (1, 80, 20.),
    ];
    let mut baselines = std::collections::BTreeMap::new();
    let mut references = std::collections::BTreeMap::new();
    let mut observations = Vec::new();
    for cycle in 0..4 {
        for (case, (project, size, frame)) in cases.into_iter().enumerate() {
            let before = reply(&mut events, &engine, |reply| Command::OpenDirectory {
                directory: projects[project].0.clone(),
                reply,
            });
            let authored = before["project"].clone();
            let revision = before["revision"].clone();
            let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(size, size));
            reply(&mut events, &engine, |reply| Command::Resize {
                window: 0,
                width: size,
                height: size,
                reply,
            });
            reply(&mut events, &engine, |reply| Command::Seek { frame, reply });
            let elapsed = present(&mut events, &engine, size);
            let screenshot = root.join("preview.png");
            crate::native_test_support::capture_ui(
                &mut events,
                &engine,
                ui(size),
                screenshot.clone(),
            )
            .unwrap();
            let (actual_size, actual) = pixels(&screenshot);
            assert_eq!(actual_size, [size; 2]);
            let reference = if let Some(reference) = references.get(&case) {
                *reference
            } else {
                let path = root.join(format!("formal-{case}.png"));
                let request = engine
                    .capture_async(path.clone(), ExportControl::default())
                    .unwrap();
                crate::native_test_support::receive(&mut events, &request, Duration::from_secs(30))
                    .unwrap()
                    .unwrap();
                let (dimensions, reference) = pixels(&path);
                assert_eq!(dimensions, [64; 2], "formal output must keep authored size");
                references.insert(case, reference);
                reference
            };
            let difference = actual
                .into_iter()
                .zip(reference)
                .map(|(a, b)| a.abs_diff(b))
                .max()
                .unwrap();
            assert!(
                difference <= 1,
                "cycle={cycle} case={case} preview={actual:?} formal={reference:?}"
            );
            let state = engine.state().unwrap();
            assert_eq!(state["project"], authored);
            assert_eq!(state["revision"], revision);
            assert_eq!(state["frame"], frame);
            assert_eq!(state["lastPresentedFrame"], frame);
            assert_eq!(state["canUndo"], false);
            let info = engine
                .plugin("{\"op\":\"allocation_budget\"}".into())
                .unwrap();
            let info = &info["allocationBudget"];
            let admitted = info["admittedBytes"].as_u64().unwrap();
            assert_eq!(info["budgetBytes"], 8 << 20);
            assert!(admitted <= 8 << 20);
            let sum: u64 = info["categories"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c["admittedBytes"].as_u64().unwrap())
                .sum();
            assert_eq!(sum, admitted, "category gauge must equal combined gauge");
            if cycle == 1 {
                baselines.insert(case, admitted);
            }
            if cycle >= 2 {
                assert!(admitted<=baselines[&case],"capacity grew after warmup cycle={cycle} case={case} admitted={admitted} baseline={}",baselines[&case]);
            }
            let peaks = engine.allocation_frame_peaks().unwrap();
            assert!(
                peaks.active.is_empty(),
                "readback must retire all earlier surface submissions"
            );
            assert!(!peaks.tracking_fault);
            assert_eq!(peaks.unconfirmed_submissions, 0);
            for frame in &peaks.recent {
                let end: u64 = frame
                    .categories
                    .iter()
                    .map(|c| c.end_admitted_bytes.unwrap())
                    .sum();
                assert_eq!(Some(end), frame.end_admitted_bytes);
            }
            observations.push(
                serde_json::json!({"cycle":cycle,"case":case,"project":project,
                "surfaceSize":size,"frame":frame,"renderCycleUs":elapsed,"center":actual,
                "formalCenter":reference,"allocationBudget":info,"preview":state["preview"]}),
            );
            std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "scope":"Actual Windows serial Engine and hidden native resize/seek/project replacement, generated images, GPU readback pixels and enrolled logical capacity; not physical VRAM, application input dispatch or a cross-device FPS benchmark",
                "cases":observations})).unwrap()).unwrap();
        }
    }
    drop(engine);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_ui_atlas_refusal_recovers_in_worker_without_a_second_upload() {
    const NAME: &str = "engine::memory_lifecycle_tests::actual_ui_atlas_refusal_recovers_in_worker_without_a_second_upload";
    const CHILD: &str = "MOTION_UI_ATLAS_WORKER_CHILD";
    let Some(directory) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        let mut copy_errors = Vec::new();
        if let Some(destination) = std::env::var_os("MOTION_UI_ATLAS_WORKER_ARTIFACTS") {
            let destination = PathBuf::from(destination);
            if let Err(error) = std::fs::create_dir_all(&destination) {
                copy_errors.push(error.to_string());
            } else {
                for name in [
                    "report.json",
                    "motion-studio.log",
                    "before.png",
                    "after.png",
                ] {
                    let source = root.path().join(name);
                    if source.exists() {
                        if let Err(error) = std::fs::copy(source, destination.join(name)) {
                            copy_errors.push(format!("{name}: {error}"));
                        }
                    }
                }
                for (name, bytes) in [
                    ("stdout.log", &output.stdout),
                    ("stderr.log", &output.stderr),
                ] {
                    if let Err(error) = std::fs::write(destination.join(name), bytes) {
                        copy_errors.push(format!("{name}: {error}"));
                    }
                }
            }
        }
        assert!(
            output.status.success(),
            "{}\n{}\nartifact copy: {copy_errors:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(copy_errors.is_empty(), "artifact copy: {copy_errors:?}");
        let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(file.contains("logging initialized level=INFO"));
        let failures: Vec<_> = file
            .lines()
            .filter(|line| {
                line.contains("error #")
                    && line.contains("phase=atlas")
                    && line.contains("UI glyph atlas")
            })
            .collect();
        assert_eq!(
            failures.len(),
            2,
            "actual worker refusal and pending refusal must both be reported"
        );
        for line in failures {
            let marker = &line[line.find("error #").unwrap()..];
            assert!(stderr.contains(marker), "stderr missing {marker}");
            assert!(!stdout.contains(marker), "stdout leaked {marker}");
            assert!(marker.contains("present ") && marker.contains("window=0"));
        }
        assert!(!file
            .lines()
            .any(|line| line.contains("start #") && line.ends_with(" present")));
        return;
    };
    let root = PathBuf::from(directory);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let project = fixture(&root, "generated-worker-atlas", [255, 0, 0, 255]);
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(96, 96)),
            )
            .unwrap(),
    );
    let engine = Engine::start(project.0, motion_desktop_media::platform(), 8 << 30).unwrap();
    let attached = engine.attach_async(window, 96, 96).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(30))
        .unwrap()
        .unwrap();
    reply(&mut events, &engine, |reply| Command::Seek {
        frame: 1.,
        reply,
    });
    let frame = |atlas| {
        let mut frame = ui(96);
        frame.atlas = atlas;
        frame.paint.push_glyph(motion_ui::paint::Glyph {
            atlas_size: None,
            uv: [0., 0., 1., 1.],
            position: [4., 4.],
            size: [8., 8.],
            color: motion_ui::Color::rgb(0, 255, 0),
        });
        frame
    };
    let bitmap = |size: u32| AtlasUpload {
        pixels: vec![255; (size * size * 4) as usize],
        width: size,
        height: size,
    };
    let began = Instant::now();
    while !reply(&mut events, &engine, |reply| Command::Present {
        ui: frame(Some(bitmap(1))),
        reply,
    }) {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "initial native frame never completed"
        );
    }
    crate::native_test_support::capture_ui(
        &mut events,
        &engine,
        frame(Some(bitmap(1))),
        root.join("before.png"),
    )
    .unwrap();
    let before = engine.state().unwrap();
    let allocation = engine
        .plugin("{\"op\":\"allocation_budget\"}".into())
        .unwrap();
    let allocation = &allocation["allocationBudget"];
    let occupied = allocation["admittedBytes"].as_u64().unwrap();
    let original_limit = allocation["budgetBytes"].as_u64().unwrap();
    let atlas_bytes = |info: &serde_json::Value| {
        info["allocationBudget"]["categories"]
            .as_array()
            .unwrap()
            .iter()
            .find(|category| category["category"] == "ui_atlas")
            .unwrap()["admittedBytes"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(
        atlas_bytes(
            &engine
                .plugin("{\"op\":\"allocation_budget\"}".into())
                .unwrap()
        ),
        4
    );
    let mut policy = motion_render::resource_policy::MemoryPolicy::default();
    policy.session_allocation_budget_bytes = occupied + 63;
    reply(&mut events, &engine, |reply| Command::Plugin {
        request: serde_json::json!({"op":"memory_policy", "policy":policy}).to_string(),
        reply,
    });
    let mut failures = Vec::new();
    for atlas in [Some(bitmap(4)), None] {
        let pending = engine
            .request(|reply| Command::Present {
                ui: frame(atlas),
                reply,
            })
            .unwrap();
        let error =
            crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(30))
                .unwrap()
                .unwrap_err();
        assert!(
            error.contains("window=0")
                && error.contains("phase=atlas")
                && error.contains("UI glyph atlas")
        );
        assert!(error.contains(&(occupied + 64).to_string()));
        assert!(error.contains(&(occupied + 63).to_string()));
        failures.push(error);
        let current = engine
            .plugin("{\"op\":\"allocation_budget\"}".into())
            .unwrap();
        assert_eq!(current["allocationBudget"]["admittedBytes"], occupied);
        assert_eq!(atlas_bytes(&current), 4);
        let state = engine.state().unwrap();
        for key in [
            "project",
            "revision",
            "frame",
            "lastPresentedFrame",
            "canUndo",
        ] {
            assert_eq!(state[key], before[key], "refusal changed {key}");
        }
    }
    assert_eq!(
        failures[0], failures[1],
        "pending refusal must preserve the original cause"
    );
    policy.session_allocation_budget_bytes = original_limit;
    reply(&mut events, &engine, |reply| Command::Plugin {
        request: serde_json::json!({"op":"memory_policy", "policy":policy}).to_string(),
        reply,
    });
    let began = Instant::now();
    while !reply(&mut events, &engine, |reply| Command::Present {
        ui: frame(None),
        reply,
    }) {
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "native atlas recovery never completed"
        );
    }
    crate::native_test_support::capture_ui(
        &mut events,
        &engine,
        frame(Some(bitmap(4))),
        root.join("after.png"),
    )
    .unwrap();
    let recovered = engine
        .plugin("{\"op\":\"allocation_budget\"}".into())
        .unwrap();
    assert_eq!(
        atlas_bytes(&recovered),
        64,
        "actual worker must upload its retained bitmap without a producer resend"
    );
    let expected = image::open(root.join("before.png")).unwrap().to_rgba8();
    let actual = image::open(root.join("after.png")).unwrap().to_rgba8();
    assert_eq!(
        actual, expected,
        "healthy pixels changed after budget recovery"
    );
    let state = engine.state().unwrap();
    for key in [
        "project",
        "revision",
        "frame",
        "lastPresentedFrame",
        "canUndo",
    ] {
        assert_eq!(state[key], before[key], "recovery changed {key}");
    }
    std::fs::write(root.join("report.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "scope":"Actual Windows serial Engine, admitted glyph atlas and native surfaces; no physical VRAM or application input claim",
        "before":allocation, "occupiedBytes":occupied, "refusals":failures,
        "recovered":recovered, "workerRecoveryProducerResends":0,
        "pixelsEqual":true, "authoredStateUnchanged":true,
    })).unwrap()).unwrap();
    drop(engine);
}
