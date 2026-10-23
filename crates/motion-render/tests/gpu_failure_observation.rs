use motion_core::{Layer, Project};
use motion_render::{Renderer, Scene};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    sync::Mutex,
};

struct Capture(Mutex<BufWriter<File>>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) && record.target().ends_with("renderer::gpu_failure") {
            let mut file = self.0.lock().unwrap();
            writeln!(file, "{} {}", record.level(), record.args()).unwrap();
            if record.level() <= log::Level::Warn {
                file.flush().unwrap();
            }
        }
    }
    fn flush(&self) {
        self.0.lock().unwrap().flush().unwrap();
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_validation_callbacks_leave_default_level_evidence_before_health_polling() {
    const CHILD: &str = "MOTION_GPU_VALIDATION_LOG_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        let capture = Box::leak(Box::new(Capture(Mutex::new(BufWriter::new(
            File::create(&path).unwrap(),
        )))));
        log::set_logger(capture).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let mut project = Project::new(64, 64, 30, 60).unwrap();
        project.layers.push(Layer::solid(
            1,
            "independent test shape",
            [64.; 2],
            [32., 32., 0.],
            [1.; 4],
        ));
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let target = renderer.capture_target(64, 64).unwrap();
        renderer.draw(&scene, &target.view, 64, 64).unwrap();
        assert!(fs::read_to_string(&path).unwrap().is_empty());
        let invalid = |label: &str| {
            renderer
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(label),
                    source: wgpu::ShaderSource::Wgsl("this is invalid WGSL".into()),
                })
        };
        drop(invalid("controlled-first-validation"));
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        // No gpu_error(), check_health() or further draw is allowed to be the
        // producer of this evidence: the actual callback must flush it itself.
        let first = fs::read_to_string(&path).unwrap();
        assert!(
            first.contains("ERROR GPU uncaptured error renderer="),
            "missing immediate GPU callback evidence: {first}"
        );
        assert!(
            first.contains("count=1 kind=validation retained_first_cause=false"),
            "{first}"
        );
        assert!(first.contains("controlled-first-validation"), "{first}");
        assert!(
            first.contains("elapsed_ms=")
                && first.contains("adapter=")
                && first.contains("backend="),
            "{first}"
        );
        for _ in 0..120 {
            drop(invalid("controlled-followup-validation"));
        }
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        let observed = fs::read_to_string(&path).unwrap();
        assert_eq!(
            observed
                .matches("ERROR GPU uncaptured error renderer=")
                .count(),
            1,
            "{observed}"
        );
        assert_eq!(
            observed
                .matches("WARN GPU follow-up errors suppressed renderer=")
                .count(),
            1,
            "{observed}"
        );
        assert!(
            observed.contains("count=60 last_kind=validation retained_first_cause=true"),
            "{observed}"
        );
        let cause = renderer.gpu_error().unwrap();
        assert!(cause.contains("controlled-first-validation"), "{cause}");
        assert!(!cause.contains("controlled-followup-validation"), "{cause}");
        assert!(renderer.draw(&scene, &target.view, 64, 64).is_err());
        renderer.device.destroy();
        renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
        assert!(renderer
            .gpu_error()
            .unwrap()
            .contains("GPU device lost: Destroyed"));
        drop(target);
        drop(renderer);
        let mut recreated = pollster::block_on(Renderer::headless()).unwrap();
        let target = recreated.capture_target(64, 64).unwrap();
        assert!(recreated
            .capture(&scene, &target)
            .unwrap()
            .0
            .iter()
            .any(|v| *v != 0));
        assert!(recreated.gpu_error().is_none());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            observed,
            "normal destroy/recreation must not add a terminal failure"
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "real_validation_callbacks_leave_default_level_evidence_before_health_polling",
        ])
        .env(CHILD, root.path().join("events.log"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("GPU uncaptured error"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("GPU uncaptured error"));
}
