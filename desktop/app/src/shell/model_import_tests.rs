//! Actual shell import and native GPU capture; no physical OS input injection.
use crate::*;
use winit::platform::windows::EventLoopBuilderExtWindows;

const NAME: &str =
    "shell::model_import_tests::dropped_obj_publishes_an_editable_model_and_native_editor_pixels";
const CHILD: &str = "MOTION_MODEL_IMPORT_UI_CHILD";

#[test]
fn dropped_obj_publishes_an_editable_model_and_native_editor_pixels() {
    if let Some(out) = std::env::var_os(CHILD) {
        capture(out.into());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let out = std::env::var_os("MOTION_MODEL_IMPORT_UI_ARTIFACTS")
        .map_or_else(|| temp.path().join("capture"), PathBuf::from);
    std::fs::create_dir_all(&out).unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &out)
        .output()
        .unwrap();
    std::fs::write(out.join("child.stdout.txt"), &result.stdout).unwrap();
    std::fs::write(out.join("child.stderr.txt"), &result.stderr).unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stderr)
    );
}

fn capture(out: PathBuf) {
    let _logging = crate::logging::init(out.join("logs")).unwrap();
    let root = out.join("project");
    motion_core::storage::save(&root, &Project::new(960, 540, 30, 180).unwrap()).unwrap();
    let original = out.join("editable-triangle.obj");
    let mtl = out.join("paint.mtl");
    std::fs::write(&mtl, "newmtl Red\nKd 1 0 0\nillum 0\n").unwrap();
    std::fs::write(
        &original,
        "mtllib paint.mtl\nv -1 -1 0\nv 1 -1 0\nv 0 1 0\nusemtl Red\nf 1 2 3\n",
    )
    .unwrap();
    let mut shell = Shell::new(root.clone(), None, Some(Locale::Zh)).unwrap();
    shell.home = None;
    let before = shell.engine.state().unwrap();
    let error = shell
        .engine
        .import_model(
            out.join("missing.obj"),
            before["revision"].as_u64().unwrap(),
        )
        .unwrap_err();
    assert!(
        error.contains("model_import") && error.contains("missing.obj"),
        "{error}"
    );
    assert_eq!(shell.engine.state().unwrap()["project"], before["project"]);
    // Main-window DroppedFile dispatches this action. Exercise that actual
    // route, rather than invoking a helper that might have no UI caller.
    shell
        .action(panels::Action::ImportPath(original.clone()), None)
        .unwrap();
    let began = Instant::now();
    while shell.importing_image.is_some() {
        assert!(
            began.elapsed() < Duration::from_secs(15),
            "shell model import did not finish"
        );
        shell.poll_import().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        shell.state["project"]["model_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let object = shell.state["modelImport"]["object"].as_u64().unwrap();
    shell.panels.select_only(Some(object));
    shell.panels.timeline.expanded.insert(object);
    shell.engine.save().unwrap();
    std::fs::remove_file(original).unwrap();
    std::fs::remove_file(mtl).unwrap();
    let mut events = EventLoop::<()>::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(1600, 1000)),
            )
            .unwrap(),
    );
    let attach = shell
        .engine
        .attach_async(window.clone(), 1600, 1000)
        .unwrap();
    shell.state =
        crate::native_test_support::receive(&mut events, &attach, Duration::from_secs(60))
            .unwrap()
            .unwrap();
    shell.window = Some(window);
    shell.attached = true;
    let (paint, _) = shell.draw_commands([1600., 1000.]);
    let ui = shell.ui_frame(paint, [1600., 1000.], true);
    let pending = shell.engine.present_async(ui).unwrap();
    assert!(
        crate::native_test_support::receive(&mut events, &pending, Duration::from_secs(60))
            .unwrap()
            .unwrap()
    );
    let (paint, _) = shell.draw_commands([1600., 1000.]);
    let ui = shell.ui_frame(paint, [1600., 1000.], true);
    let path = out.join("editor-imported-model.png");
    crate::native_test_support::capture_ui(&mut events, &shell.engine, ui, path.clone()).unwrap();
    let pixels = image::open(&path).unwrap().into_rgba8();
    let view = panels::composition_view(shell.workspace.display_root(), [1600., 1000.]);
    let region = image::imageops::crop_imm(
        &pixels,
        view.min[0] as u32,
        view.min[1] as u32,
        view.width() as u32,
        view.height() as u32,
    )
    .to_image();
    let red = region
        .pixels()
        .filter(|p| p[0] > p[1].saturating_add(20) && p[0] > p[2].saturating_add(20))
        .count();
    assert!(
        red > 200,
        "native editor has only {red} red triangle pixels"
    );
    let saved = motion_core::storage::load(&root).unwrap();
    assert_eq!(
        serde_json::to_value(&saved).unwrap(),
        shell.engine.state().unwrap()["project"]
    );
    std::fs::write(out.join("accepted.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "sourceFilesRemoved":true,"redCompositionPixels":red,"object":object,"nativeCapture":path,
        "physicalOsInputInjected":false,"project":saved,
    })).unwrap()).unwrap();
    shell.engine.request_shutdown().unwrap();
}
