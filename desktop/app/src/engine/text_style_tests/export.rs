//! Real PNG, frozen MP4, a negative control, and an independent reopen process.
use super::*;
use motion_desktop_media::movie::ExportControl;

fn png(engine: &Engine, root: &std::path::Path, name: &str) -> image::RgbaImage {
    let output = root.join(format!("{name}.png"));
    let result = engine
        .capture_async(output.clone(), ExportControl::default())
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(result["width"], 320);
    assert_eq!(result["height"], 160);
    image::open(output).unwrap().to_rgba8()
}
fn ink(p: &[u8]) -> bool {
    u32::from(p[0]) * u32::from(p[3]) / 255 > 160
}
fn overlap(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut intersection, mut union) = (0, 0);
    for (a, b) in a.pixels().zip(b.pixels()) {
        let (a, b) = (ink(&a.0), ink(&b.0));
        intersection += usize::from(a && b);
        union += usize::from(a || b);
    }
    assert!(union > 50, "actual text pixels are required");
    intersection as f64 / union as f64
}

#[test]
#[cfg_attr(
    not(feature = "gpu-tests"),
    ignore = "requires actual GPU, FFmpeg export and independent reopen"
)]
fn text_style_png_movie_and_independent_reopen_preserve_authored_pixels() {
    const NAME:&str="engine::text_style_tests::export::text_style_png_movie_and_independent_reopen_preserve_authored_pixels";
    const REOPEN: &str = "MOTION_TEXT_STYLE_EXPORT_REOPEN";
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os(REOPEN)
        .or_else(|| std::env::var_os("MOTION_TEXT_STYLE_EXPORT_ARTIFACTS"))
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let directory = root.join("project");
    if std::env::var_os(REOPEN).is_some() {
        let expected: Value =
            serde_json::from_slice(&std::fs::read(root.join("expected-project.json")).unwrap())
                .unwrap();
        let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(engine.state().unwrap()["project"], expected);
        let reopened = png(&engine, &root, "reopened");
        assert_eq!(
            reopened,
            image::open(root.join("styled.png")).unwrap().to_rgba8()
        );
        return;
    }
    motion_core::storage::save(
        &directory,
        &motion_core::Project::new(320, 160, 30, 2).unwrap(),
    )
    .unwrap();
    let engine =
        Engine::start(directory.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let before = engine
        .create_text("Original".into(), 24., 240, None)
        .unwrap();
    let original = png(&engine, &root, "original");
    let mut edit = request(&before);
    let mut style = TextStyle::new(32., 240);
    style.line_height = Some(50.);
    style.tracking = 2.;
    style.alignment = TextAlign::Center;
    style.indent_left = 4.;
    style.indent_right = 8.;
    style.indent_first = 10.;
    style.stroke_width = 1.5;
    edit["text"] = "Styled\nText".into();
    edit["style"] = serde_json::to_value(&style).unwrap();
    let after = apply(&engine, &before, edit).unwrap();
    engine.save().unwrap();
    std::fs::write(
        root.join("expected-project.json"),
        serde_json::to_vec_pretty(&after["project"]).unwrap(),
    )
    .unwrap();
    let styled = png(&engine, &root, "styled");
    assert_ne!(original, styled);
    let control = ExportControl::default();
    let frozen = engine
        .freeze_movie(control.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(frozen.frames(), 2);
    let undone = engine.history(0).unwrap();
    assert_eq!(undone["project"], before["project"]);
    assert_eq!(png(&engine, &root, "undone"), original);
    drop(engine);
    let movie = root.join("styled.mp4");
    let result = frozen.export(&movie, &control, |_| {}).unwrap();
    assert_eq!((result.width, result.height, result.frames), (320, 160, 2));
    let platform = motion_desktop_media::platform();
    let probe = platform
        .probe_video(&movie, None, None, &|| Ok(()))
        .unwrap();
    assert_eq!(probe.timestamps.len(), 2);
    let mut decoder = platform
        .open_decoder(&movie, probe.asset, probe.timestamps)
        .unwrap();
    let mut scores = Vec::new();
    for (index, time) in [0, 33333].into_iter().enumerate() {
        let frame = decoder.frame(time, &|| Ok(())).unwrap();
        let decoded = image::RgbaImage::from_raw(
            frame.width,
            frame.height,
            frame.rgba().unwrap().into_owned(),
        )
        .unwrap();
        let positive = overlap(&styled, &decoded);
        let negative = overlap(&original, &decoded);
        assert!(
            positive > 0.75 && positive > negative + 0.3,
            "styled IoU={positive}, original IoU={negative}"
        );
        decoded
            .save(root.join(format!("decoded-{index}.png")))
            .unwrap();
        scores.push(serde_json::json!({"frame":index,"pts":frame.pts,"styled_iou":positive,"original_iou":negative}));
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            NAME,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(REOPEN, &root)
        .output()
        .unwrap();
    std::fs::write(root.join("reopen.stdout.log"), &child.stdout).unwrap();
    std::fs::write(root.join("reopen.stderr.log"), &child.stderr).unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(root.join("reopened.png").is_file());
    std::fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "png":"styled.png","independent_reopen_exact":true,"undo_exact":true,
            "movie_frozen_before_undo":true,"decoded_frames":scores,"style":style
        }))
        .unwrap(),
    )
    .unwrap();
}
