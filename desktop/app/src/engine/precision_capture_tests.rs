//! Real worker display captures must not inherit the scene working format.
use super::*;
use motion_effects::image_contract::ChannelPrecision;
use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows};

fn frame() -> UiFrame {
    UiFrame {
        paint: Default::default(),
        atlas: Some(AtlasUpload {
            pixels: vec![255; 4],
            width: 1,
            height: 1,
        }),
        logical_size: [64.; 2],
        composition: Some([0., 0., 64., 64.]),
        composition_image: None,
        window: 0,
        node_preview: None,
    }
}
fn receive<T>(
    events: &mut EventLoop<()>,
    pending: &std::sync::mpsc::Receiver<Result<T, String>>,
) -> T {
    crate::native_test_support::receive(events, pending, Duration::from_secs(60))
        .unwrap()
        .unwrap()
}
fn present(events: &mut EventLoop<()>, engine: &Engine) {
    let began = Instant::now();
    loop {
        let pending = engine
            .request(|reply| Command::Present { ui: frame(), reply })
            .unwrap();
        if receive(events, &pending) {
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(60),
            "precision front never completed"
        );
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_worker_ui_and_node_pngs_remain_display_encoded_in_all_four_precisions() {
    const NAME: &str = "engine::precision_capture_tests::real_worker_ui_and_node_pngs_remain_display_encoded_in_all_four_precisions";
    const CHILD: &str = "MOTION_PRECISION_CAPTURE_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, root.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        if let Some(destination) = std::env::var_os("MOTION_PRECISION_OUTPUT_ARTIFACTS") {
            std::fs::create_dir_all(&destination).unwrap();
            for file in std::fs::read_dir(root.path()).unwrap() {
                let file = file.unwrap();
                if file.file_type().unwrap().is_file() {
                    std::fs::copy(
                        file.path(),
                        PathBuf::from(&destination).join(file.file_name()),
                    )
                    .unwrap();
                }
            }
        }
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
        assert!(log.contains("logging initialized level=INFO"));
        assert!(
            log.lines()
                .any(|line| line.contains("start #") && line.ends_with(" capture_ui"))
        );
        assert!(
            log.lines()
                .any(|line| line.contains("start #") && line.ends_with(" node_capture"))
        );
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.clone()).unwrap();
    let mut project = motion_core::Project::new(16, 16, 30, 12).unwrap();
    project.background = [0., 1., 0., 1.];
    project.layers.push(motion_core::Layer::solid(
        1,
        "Display precision probe",
        [8.; 2],
        [8., 8., 0.],
        [1., 0., 0., 1.],
    ));
    let directory = root.join("project");
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
            )
            .unwrap(),
    );
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    let attached = engine.attach_async(window, 64, 64).unwrap();
    receive(&mut events, &attached);
    let editor = crate::node_editor::NodeEditor::default();
    let snapshot = editor.snapshot();
    engine.node_expect(snapshot.preview_stamp()).unwrap();
    let installed = engine
        .node_install(snapshot.clone(), snapshot.compile().unwrap())
        .unwrap();
    assert!(receive(&mut events, &installed));
    let mut baseline_ui = None;
    let mut baseline_node = None;
    let mut report = Vec::new();
    for (precision, native) in [
        (ChannelPrecision::Unorm8, "Rgba8UnormSrgb"),
        (ChannelPrecision::Unorm16, "Rgba16Unorm"),
        (ChannelPrecision::Float16, "Rgba16Float"),
        (ChannelPrecision::Float32, "Rgba32Float"),
    ] {
        engine
            .edit(
                serde_json::json!({"op":"set_working_precision","precision":precision}).to_string(),
            )
            .unwrap();
        present(&mut events, &engine);
        let state = engine.state().unwrap();
        let ledger = engine.plugin(r#"{"op":"resource_ledger"}"#.into()).unwrap();
        assert!(
            ledger["resources"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["format"] == native),
            "{ledger}"
        );
        let path = root.join(format!("{precision:?}-ui.png"));
        crate::native_test_support::capture_ui(&mut events, &engine, frame(), path.clone())
            .unwrap();
        let image = image::open(&path).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), (64, 64));
        assert_eq!(image.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(image.get_pixel(32, 32).0, [255, 0, 0, 255]);
        if let Some(baseline) = &baseline_ui {
            assert!(image.as_raw() == baseline, "UI pixels differ at {precision:?}");
        } else {
            baseline_ui = Some(image.into_raw());
        }
        let path = root.join(format!("{precision:?}-node.png"));
        let pending = engine
            .request(|reply| Command::NodeCapture {
                seconds: 0.,
                path: path.clone(),
                reply,
            })
            .unwrap();
        let node = receive(&mut events, &pending);
        let image = image::open(path).unwrap().into_rgba8();
        assert_eq!(
            image.dimensions(),
            (
                crate::node_preview::RASTER_SIZE,
                crate::node_preview::RASTER_SIZE
            )
        );
        assert!(image.as_raw().chunks_exact(4).any(|p| p[3] != 0));
        if let Some(baseline) = &baseline_node {
            assert!(image.as_raw() == baseline, "node pixels differ at {precision:?}");
        } else {
            baseline_node = Some(image.into_raw());
        }
        let after = engine.state().unwrap();
        assert_eq!(state["revision"], after["revision"]);
        assert_eq!(state["project"], after["project"]);
        report.push(
            serde_json::json!({"precision":precision,"nativeFormat":native,
            "revision":state["revision"],"nodeFingerprint":node["fingerprint"]}),
        );
    }
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    drop(engine);
    crate::logging::flush();
}
