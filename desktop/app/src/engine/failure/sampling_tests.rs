//! Real frozen-render failures through the desktop reply, status and log sinks.
use super::*;
use crate::{Locale, Shell};
use motion_core::{vector::*, Command, Content, Layer, Project, Property};

fn finish_capture(shell: &mut Shell) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while shell.exporting_frame.is_some() || shell.engine.frame_capture_in_flight().is_some() {
        shell.poll_frame_export();
        assert!(Instant::now() < deadline, "capture did not terminate");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn camera_plane_refusal_reaches_shell_and_default_logs_then_recovers() {
    check_refusal(true, "engine::failure::sampling_tests::camera_plane_refusal_reaches_shell_and_default_logs_then_recovers");
}

#[test]
fn gpu_coordinate_refusal_reaches_shell_and_default_logs_then_recovers() {
    check_refusal(false, "engine::failure::sampling_tests::gpu_coordinate_refusal_reaches_shell_and_default_logs_then_recovers");
}

fn check_refusal(camera: bool, test_name: &str) {
    const CHILD: &str = "MOTION_SAMPLING_CAMERA_FAILURE_CHILD";
    let cause = if camera {
        "layer 7: continuous vector raster crosses or is behind the camera plane"
    } else {
        "object=7 phase=plane_coordinates: projection exceeds GPU coordinate range"
    };
    if let Some(root) = std::env::var_os(CHILD) {
        // Export publication resolves the parent directory before reporting its
        // path. Use that same physical fixture directory from the start: Windows
        // runners may provide TEMP through an 8.3 alias such as RUNNER~1.
        let root = PathBuf::from(root).canonicalize().unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        for (language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
            let mut project = Project::new(64, 64, 30, 60).unwrap();
            project.version = 21;
            project.camera.created = camera;
            project.camera.position.value = [32., 32., -10.];
            let mut layer =
                Layer::solid(7, "Camera plane study", [32.; 2], [32., 32., 0.], [1.; 4]);
            layer.content = Content::Vector {
                vector: VectorContent::shape(ShapeKind::Rectangle),
            };
            if camera {
                layer.three_d = true;
                layer.transform.rotation.value = [0., 60., 0.];
                layer.sampling.continuous_rasterization = true;
            } else {
                layer.transform.position.value = [10_000_000., 32., 0.];
                layer.transform.scale.value = [1e-39, 100., 100.];
            }
            project.layers.push(layer);
            let directory = root.join(language);
            motion_core::storage::save(&directory, &project).unwrap();
            let mut shell = Shell::new(directory, None, Some(locale)).unwrap();
            let before = shell.engine.state().unwrap();
            let destination = root.join(format!("{language}.png"));
            std::fs::write(&destination, b"existing output").unwrap();
            shell.start_frame_export(destination.clone()).unwrap();
            finish_capture(&mut shell);
            let message = shell.panels.message.as_deref().unwrap();
            let failure = format!(
                "PNG export revision=0 frame=0 path={}: PNG render: GPU validation: {cause}",
                destination.display()
            );
            assert_eq!(message, locale.error(&failure));
            std::fs::write(root.join(format!("{language}-cause.txt")), &failure).unwrap();
            std::fs::write(root.join(format!("{language}-message.txt")), message).unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"existing output");
            let after = shell.engine.state().unwrap();
            for key in ["project", "revision", "history", "canUndo", "canRedo"] {
                assert_eq!(before[key], after[key], "failure changed {key}");
            }
            assert!(!shell.engine.failed());
            // Explicit user edit and new request; no automatic render retry.
            let mut repair = vec![Command::SetVector {
                object: 7,
                property: Property::Position,
                frame: 0,
                value: [32., 32., 100.],
            }];
            if !camera {
                repair.push(Command::SetVector {
                    object: 7,
                    property: Property::Scale,
                    frame: 0,
                    value: [100., 100., 100.],
                });
            }
            shell.state = shell
                .engine
                .edit(serde_json::to_string(&repair).unwrap())
                .unwrap();
            shell.start_frame_export(destination.clone()).unwrap();
            finish_capture(&mut shell);
            let png = image::open(&destination).unwrap().to_rgba8();
            assert_eq!(png.dimensions(), (64, 64));
            assert!(
                png.pixels().any(|p| p[0] > 0),
                "recovery must render the layer"
            );
            assert!(shell
                .panels
                .message
                .as_deref()
                .unwrap()
                .contains(destination.to_str().unwrap()));
            assert!(!shell.engine.failed());
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name])
        .env(CHILD, root.path())
        .env("MOTION_UI_LAYOUT", root.path().join("isolated-layout.json"))
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(file.contains("logging initialized level=INFO"));
    let errors: Vec<_> = file.lines().filter(|line| line.contains(cause)).collect();
    assert_eq!(errors.len(), 2, "{file}");
    assert_eq!(stderr.matches(cause).count(), 2, "{stderr}");
    for language in ["zh", "en"] {
        let original =
            std::fs::read_to_string(root.path().join(format!("{language}-cause.txt"))).unwrap();
        assert_eq!(file.matches(&original).count(), 1);
        assert_eq!(stderr.matches(&original).count(), 1);
    }
    let mut previous_error_end = 0;
    for error in errors {
        assert!(error.contains("ERROR") && error.contains(" capture ") && error.contains("ms:"));
        let request = error
            .split("error #")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        let error_start = file.find(error).unwrap();
        // Each engine restarts request numbering; require fresh start records
        // after the previous failure, rather than matching the first engine.
        let interval = &file[previous_error_end..error_start];
        let serial = interval.find(&format!("start #{request} capture")).unwrap();
        let asynchronous = interval
            .find(&format!(
                "async start #{request} capture phase=freeze_render_publish"
            ))
            .unwrap();
        assert!(serial < asynchronous);
        previous_error_end = error_start + error.len();
    }
    for marker in [cause, "logging initialized", "async start #"] {
        assert!(!stdout.contains(marker), "log leaked to stdout: {stdout}");
    }
    if let Some(directory) = std::env::var_os("MOTION_SAMPLING_FAILURE_ARTIFACTS") {
        let directory =
            PathBuf::from(directory).join(if camera { "camera" } else { "coordinates" });
        std::fs::create_dir_all(&directory).unwrap();
        for file in [
            "motion-studio.log",
            "zh-message.txt",
            "en-message.txt",
            "zh.png",
            "en.png",
        ] {
            std::fs::copy(root.path().join(file), directory.join(file)).unwrap();
        }
        std::fs::write(directory.join("stderr.log"), stderr).unwrap();
        std::fs::write(directory.join("stdout.log"), stdout).unwrap();
    }
}
