//! Actual PNG / FFmpeg output with an unconverted-timing negative control.
use super::*;
use crate::engine::{Command as WorkerCommand, Engine};
use motion_desktop_media::movie::ExportControl;
use motion_host::Platform;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

const FRAMES: [u32; 6] = [0, 4, 8, 12, 16, 19];

fn capture(engine: &Engine, root: &Path, prefix: &str, frame: u32) -> image::RgbaImage {
    engine.seek(f64::from(frame)).unwrap();
    let output = root.join(format!("{prefix}-{frame}.png"));
    let result = engine
        .capture_async(output.clone(), ExportControl::default())
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(
        (result["width"].as_u64(), result["height"].as_u64()),
        (Some(128), Some(64))
    );
    image::open(output).unwrap().to_rgba8()
}

fn center(image: &image::RgbaImage) -> [f64; 2] {
    let (mut x, mut y, mut count) = (0., 0., 0.);
    for (px, py, value) in image.enumerate_pixels() {
        if value[0] > 200 && value[1] > 200 && value[2] > 200 && value[3] > 200 {
            x += f64::from(px) + 0.5;
            y += f64::from(py) + 0.5;
            count += 1.;
        }
    }
    assert!(count > 80., "the moving solid must be visible");
    [x / count, y / count]
}

#[test]
#[ignore = "manual real GPU, FFmpeg and independent reopen acceptance for Roving timing"]
fn roving_png_frozen_movie_and_independent_reopen_match_model_samples_and_reject_legacy_timing() {
    const NAME: &str = "panels::animation_menu::roving::export_tests::roving_png_frozen_movie_and_independent_reopen_match_model_samples_and_reject_legacy_timing";
    const REOPEN: &str = "MOTION_ROVING_EXPORT_REOPEN";
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os(REOPEN)
        .or_else(|| std::env::var_os("MOTION_ROVING_EXPORT_ARTIFACTS"))
        .map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let _logger = crate::logging::init(root.join(if std::env::var_os(REOPEN).is_some() {
        "reopen-logs"
    } else {
        "logs"
    }))
    .unwrap();
    let directory = root.join("project");
    if std::env::var_os(REOPEN).is_some() {
        let expected: Value =
            serde_json::from_slice(&std::fs::read(root.join("expected-project.json")).unwrap())
                .unwrap();
        let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
        assert_eq!(engine.state().unwrap()["project"], expected);
        for frame in FRAMES {
            assert_eq!(
                capture(&engine, &root, "reopened", frame),
                image::open(root.join(format!("roving-{frame}.png")))
                    .unwrap()
                    .to_rgba8()
            );
        }
        engine.request_shutdown().unwrap();
        return;
    }
    let mut project = motion_core::Project::new(128, 64, 30, 20).unwrap();
    let mut layer =
        motion_core::Layer::solid(7, "Roving output", [12.; 2], [16., 32., 0.], [1.; 4]);
    for (frame, x) in [(0, 16.), (10, 32.), (19, 112.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 32., 0.], Ease::Linear)
            .unwrap();
    }
    project.layers.push(layer);
    motion_core::storage::save(&directory, &project).unwrap();
    let engine =
        Engine::start(directory.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let legacy = center(&capture(&engine, &root, "legacy", 8));
    let state = engine.state().unwrap();
    let mut panels = Panels {
        selected: Some(7),
        focused: Some(Panel::Timeline),
        ..Default::default()
    };
    panels
        .selected_keys
        .apply_addressed(
            &state,
            [key_selection::Key {
                target: KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                frame: 10,
            }
            .into()],
            key_selection::Mode::Replace,
        )
        .unwrap();
    let mut actions = vec![];
    super::super::apply(&mut panels, &state, Operation::Roving(true), &mut actions);
    let [Action::EditorAction { source, commands }] = actions.as_slice() else {
        panic!("one typed conversion action");
    };
    let state = engine
        .request(|reply| WorkerCommand::EditorAction {
            source: source.clone(),
            commands: commands.clone(),
            reply,
        })
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    panels.normalize_selection(&state);
    assert_eq!(panels.selected_keys.spatial_selections()[0].1, [2]);
    assert_eq!(state["history"]["undoEntries"], 1);
    let expected: motion_core::Project = serde_json::from_value(state["project"].clone()).unwrap();
    assert!(
        (expected.layers[0]
            .transform
            .position
            .key_frames()
            .nth(1)
            .unwrap()
            - 19. / 6.)
            .abs()
            < 1e-8
    );
    engine.save().unwrap();
    std::fs::write(
        root.join("expected-project.json"),
        // Match the complete worker JSON representation. Serializing Project
        // directly emits f32 decimal spellings, whereas State's Value stores
        // their exact f64 promotion. Both reopen to the same f32, but the JSON
        // equality below deliberately compares every original worker field.
        serde_json::to_vec_pretty(&state["project"]).unwrap(),
    )
    .unwrap();
    let mut centers = vec![];
    for frame in FRAMES {
        let png = capture(&engine, &root, "roving", frame);
        let position = expected.layers[0]
            .transform
            .position
            .sample(f64::from(frame))
            .unwrap();
        let pixel = center(&png);
        assert!((pixel[0] - f64::from(position[0])).abs() < 1.);
        assert!((pixel[1] - f64::from(position[1])).abs() < 1.);
        centers.push(pixel);
    }
    assert!(
        (centers[2][0] - legacy[0]).abs() > 20.,
        "old integer timing must visibly disagree"
    );
    let control = ExportControl::default();
    let frozen = engine
        .freeze_movie(control.clone())
        .unwrap()
        .recv_timeout(Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert_eq!(frozen.frames(), 20);
    assert_eq!(
        engine.history(0).unwrap()["project"],
        serde_json::to_value(&project).unwrap()
    );
    drop(engine);
    let movie = root.join("roving.mp4");
    let exported = frozen.export(&movie, &control, |_| {}).unwrap();
    assert_eq!(
        (exported.width, exported.height, exported.frames),
        (128, 64, 20)
    );
    let platform = motion_desktop_media::platform();
    let probe = platform
        .probe_video(&movie, None, None, &|| Ok(()))
        .unwrap();
    assert_eq!(probe.timestamps.len(), 20);
    let times = probe.timestamps.clone();
    let mut decoder = platform
        .open_decoder(&movie, probe.asset, probe.timestamps)
        .unwrap();
    let mut report = vec![];
    for (index, frame) in FRAMES.into_iter().enumerate() {
        let decoded = decoder.frame(times[frame as usize], &|| Ok(())).unwrap();
        let png = image::RgbaImage::from_raw(
            decoded.width,
            decoded.height,
            decoded.rgba().unwrap().into_owned(),
        )
        .unwrap();
        let pixel = center(&png);
        assert!(
            (pixel[0] - centers[index][0]).abs() < 1. && (pixel[1] - centers[index][1]).abs() < 1.
        );
        if frame == 8 {
            assert!((pixel[0] - legacy[0]).abs() > 20.);
        }
        png.save(root.join(format!("decoded-{frame}.png"))).unwrap();
        report.push(serde_json::json!({"frame": frame,"pts": decoded.pts,"model": expected.layers[0].transform.position.sample(f64::from(frame)).unwrap(),"png_center":centers[index],"decoded_center":pixel,"legacy_frame8_center":legacy}));
    }
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            NAME,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(REOPEN, &root)
        .env_remove("MOTION_LOG")
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
    std::fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}
