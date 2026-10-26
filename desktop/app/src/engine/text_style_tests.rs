//! Actual text preflight, transaction, persistence and default output channels.
use super::*;
use motion_core::{TextAlign, TextStyle};
#[cfg(feature = "ffmpeg")]
mod export;

fn apply(engine: &Engine, state: &Value, mut request: Value) -> Result<Value, String> {
    request["op"] = "font_edit_text".into();
    request["revision"] = state["revision"].clone();
    engine.call(|reply| Command::TextLayout {
        source: crate::layer_clipboard::KeyframeSource::of(state),
        request,
        reply,
    })
}
fn request(state: &Value) -> Value {
    let layer = &state["project"]["layers"][0];
    serde_json::json!({"object":layer["id"],"text":layer["content"]["text"],
        "font":layer["content"]["font"],"style":layer["content"]["style"]})
}
#[test]
fn text_styles_are_one_transaction_preserve_transform_reopen_and_cancel_stale_sources() {
    let root = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        root.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let before = engine
        .create_text("Original\nSecond line".into(), 24., 320, None)
        .unwrap();
    let transform = before["project"]["layers"][0]["transform"].clone();
    let mut edit = request(&before);
    let mut style = TextStyle::new(32., 280);
    style.line_height = Some(60.);
    style.tracking = 4.;
    style.indent_left = 8.;
    style.indent_right = 12.;
    style.indent_first = 6.;
    style.alignment = TextAlign::Right;
    style.stroke_width = 1.5;
    edit["style"] = serde_json::to_value(&style).unwrap();
    edit["text"] = "Edited\nNew text".into();
    let after = apply(&engine, &before, edit).unwrap();
    assert_eq!(after["textEdit"]["cancelled"], false);
    let layer = &after["project"]["layers"][0];
    assert_eq!(
        layer["content"]["style"],
        serde_json::to_value(&style).unwrap()
    );
    assert_eq!(layer["transform"], transform);
    assert_eq!(layer["size"][0], 280.);
    let id = layer["content"]["raster_asset"].as_u64().unwrap();
    let path = after["project"]["assets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap()["path"]
        .as_str()
        .unwrap();
    let pixels = image::open(root.path().join(path)).unwrap().to_rgba8();
    assert!(pixels.pixels().any(|p| p[3] > 0));
    let undo = engine.history(0).unwrap();
    assert_eq!(undo["project"], before["project"]);
    let redo = engine.history(1).unwrap();
    assert_eq!(redo["project"], after["project"]);
    engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
        after["project"]
    );
    // A previously captured object ID cannot mutate a newer revision.
    let cancelled = apply(&engine, &before, request(&before)).unwrap();
    assert_eq!(cancelled["textEdit"]["cancelled"], true);
    assert_eq!(cancelled["project"], after["project"]);
    assert_eq!(cancelled["revision"], redo["revision"]);
    assert_eq!(cancelled["history"], redo["history"]);
    assert_eq!(
        image::open(root.path().join(path)).unwrap().to_rgba8(),
        pixels
    );
}

#[test]
fn text_layout_failures_keep_model_history_and_default_file_stderr_stdout_boundaries() {
    const NAME:&str="engine::text_style_tests::text_layout_failures_keep_model_history_and_default_file_stderr_stdout_boundaries";
    const CHILD: &str = "MOTION_TEXT_STYLE_ERRORS_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let temp = tempfile::tempdir().unwrap();
        let root = std::env::var_os("MOTION_TEXT_STYLE_ERROR_ARTIFACTS")
            .map(PathBuf::from)
            .unwrap_or_else(|| temp.path().into());
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
            .env(CHILD, &root)
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
        std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let file = std::fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        for cause in [
            "font size must be 4..256 pixels",
            "U+4E2D",
            "phase=font_import",
        ] {
            assert!(
                file.lines().any(|l| l.contains(" ERROR ")
                    && l.contains("text_layout")
                    && l.contains(cause)),
                "{file}"
            );
            assert!(stderr.contains(cause), "{stderr}");
            assert!(!stdout.contains(cause), "{stdout}");
        }
        assert_eq!(
            file.lines()
                .filter(|l| l.contains(" ERROR ") && l.contains("text_layout"))
                .count(),
            3
        );
        assert!(file
            .lines()
            .any(|l| l.contains(" INFO ") && l.contains("start #") && l.ends_with(" text_layout")));
        for content in [&file, stderr.as_ref(), stdout.as_ref()] {
            assert!(!content.contains("PRIVATE_DRAFT_SENTINEL"));
        }
        assert!(!stderr.contains("start #") && !stdout.contains("start #"));
        return;
    };
    let root = PathBuf::from(root);
    let _logging = crate::logging::init(root.join("logs")).unwrap();
    let engine = Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let before = engine
        .create_text("Original".into(), 24., 320, None)
        .unwrap();
    let mut invalid = request(&before);
    invalid["style"]["font_size"] = (-1).into();
    let mut missing = request(&before);
    missing["text"] = "中 PRIVATE_DRAFT_SENTINEL".into();
    let mut file = request(&before);
    file["font_path"] = root
        .join("missing-font.ttf")
        .to_string_lossy()
        .as_ref()
        .into();
    for invalid in [invalid, missing, file] {
        let cause = apply(&engine, &before, invalid).unwrap_err();
        assert!(!cause.is_empty());
        let after = engine.state().unwrap();
        assert_eq!(after["project"], before["project"]);
        assert_eq!(after["revision"], before["revision"]);
        assert_eq!(after["history"], before["history"]);
        assert_eq!(after["history"]["gestureActive"], false);
    }
    let mut valid = request(&before);
    valid["text"] = "Healthy after failure".into();
    let after = apply(&engine, &before, valid).unwrap();
    assert_eq!(after["textEdit"]["cancelled"], false);
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
}
