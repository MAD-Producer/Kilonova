//! Real portable parsing/native validation faults through the production sink
//! and the real Session guard, at the default Info level.
use super::*;
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_render::{Renderer, Scene};
use std::process::{Command, Stdio};
use std::sync::Arc;

const TARGET: &str = "motion_render::pointwise_compile";
fn fixture() -> (Project, Scene) {
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let mut layer = Layer::solid(
        47,
        "Compiler acceptance",
        [32.; 2],
        [16., 16., 0.],
        [0.2, 0.4, 0.6, 1.],
    );
    for (id, name) in [(11, "tint"), (12, "invert")] {
        let definition = package
            .manifest
            .effects
            .iter()
            .find(|e| e.id == name)
            .unwrap();
        layer.effects.push(EffectInstance::new(
            id,
            &package.manifest.id,
            &package.manifest.version,
            &package.hash,
            definition,
            [32.; 2],
        ));
    }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene
        .sample(&project, 0., None, &ExpressionEvaluator)
        .unwrap();
    (project, scene)
}

#[test]
fn compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries() {
    compiler_failure_acceptance(
        false,
        "logging::pointwise_compile_tests::compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries",
    );
}

#[test]
fn warm_compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries() {
    compiler_failure_acceptance(
        true,
        "logging::pointwise_compile_tests::warm_compiler_failures_reach_default_file_and_stderr_after_unlock_without_retries",
    );
}

fn compiler_failure_acceptance(warm: bool, test: &str) {
    const CHILD: &str = "MOTION_POINTWISE_FILE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let root = PathBuf::from(root);
        let _guard = init(root.join("logs")).unwrap();
        let (project, scene) = fixture();
        let project_root = root.join("project");
        fs::create_dir_all(&project_root).unwrap();
        let id =
            motion_host::session::open(project_root, project, motion_desktop_media::platform())
                .unwrap();
        let mode = std::env::var("MOTION_POINTWISE_FILE_MODE").unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        renderer.set_pointwise_fusion(true);
        let target = renderer.render_target(32, 32).unwrap();
        if warm {
            let capture = renderer.capture_target(32, 32).unwrap();
            let pixels = renderer.capture(&scene, &capture).unwrap().0;
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            assert_eq!(renderer.pointwise_fusion_report().portable_compiles, 1);
            let mut registry = motion_effects::Registry::new_with_builtins().unwrap();
            for package in registry.packages.values_mut() {
                *package = Arc::new(
                    motion_effects::EffectPackage::from_bytes(package.bytes.clone()).unwrap(),
                );
            }
            renderer.set_effect_registry(registry);
            assert!(renderer.effect_pipeline_cache_info().previous_entries > 0);
        }
        if mode == "portable" {
            renderer.debug_inject_pointwise_portable_failure();
        } else {
            renderer.debug_inject_pointwise_native_failure();
        }
        let log_path = root.join("logs/motion-studio.log");
        flush();
        let initial = fs::read_to_string(&log_path).unwrap();
        let primary = motion_host::session::with_session::<()>(id, |session| {
            session.editor_renderer = Some(renderer);
            let renderer = session.editor_renderer.as_mut().unwrap();
            let primary = renderer.draw(&scene, &target.view, 32, 32).unwrap_err();
            assert!(
                primary.to_string().contains("pointwise_diagnostic_invalid"),
                "{primary}"
            );
            let builds = renderer.effect_pipeline_builds();
            for _ in 0..70 {
                let repeated = renderer.draw(&scene, &target.view, 32, 32).unwrap_err();
                assert_eq!(repeated.to_string(), primary.to_string());
                assert_eq!(renderer.effect_pipeline_builds(), builds);
            }
            // A flushed Error would be immediately visible even though Info is
            // buffered. The compiler must not call the sink under this guard.
            assert_eq!(fs::read_to_string(&log_path).unwrap(), initial);
            Err(primary.into())
        })
        .unwrap_err();
        assert!(primary.to_string().contains("pointwise_diagnostic_invalid"));
        let text = fs::read_to_string(&log_path).unwrap();
        assert_eq!(
            text.lines()
                .filter(|l| l.contains(TARGET) && l.contains("phase=failure"))
                .count(),
            1
        );
        assert!(text.contains("object=Some(47) effects=[11, 12]"));
        assert!(text.contains(&format!("session=Some({id})")));
        assert!(text.contains("deferred=true"));
        // A replacement renderer has a fresh bounded compiler/native cache.
        // Recovery must produce real pixels, rather than clearing the failure.
        motion_host::session::with_session(id, |session| {
            let mut healthy = pollster::block_on(Renderer::headless())?;
            healthy.set_pointwise_fusion(true);
            let capture = healthy.capture_target(32, 32)?;
            healthy.draw(&scene, &capture.view, 32, 32)?;
            let pixels = healthy.read_target(&capture)?;
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            let builds = healthy.effect_pipeline_builds();
            let compiles = healthy.pointwise_fusion_report().portable_compiles;
            for _ in 0..65 {
                healthy.draw(&scene, &capture.view, 32, 32)?;
                // Verify a completed healthy frame before submitting the next.
                // Metal callbacks need device polling; queuing 66 frames without
                // reading any output can exhaust the 64-encoder admission bound.
                let pixels = healthy.read_target(&capture)?;
                assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
                assert_eq!(healthy.effect_pipeline_builds(), builds);
                assert_eq!(
                    healthy.pointwise_fusion_report().portable_compiles,
                    compiles
                );
            }
            let pixels = healthy.read_target(&capture)?;
            assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
            session.editor_renderer = Some(healthy);
            Ok(())
        })
        .unwrap();
        motion_host::session::close(id);
        flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let artifact = std::env::var_os("MOTION_POINTWISE_FILE_ARTIFACTS").map(PathBuf::from);
    let base = artifact.as_deref().unwrap_or(temporary.path());
    let base = if warm {
        base.join("warm-cache")
    } else {
        base.to_owned()
    };
    for mode in ["portable", "native"] {
        let root = base.join(mode);
        assert!(!root.exists(), "acceptance output must be new");
        fs::create_dir_all(&root).unwrap();
        let stdout = root.join("stdout.txt");
        let stderr = root.join("stderr.txt");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture"])
            .env(CHILD, &root)
            .env("MOTION_POINTWISE_FILE_MODE", mode)
            .env_remove("MOTION_LOG")
            .stdout(Stdio::from(File::create(&stdout).unwrap()))
            .stderr(Stdio::from(File::create(&stderr).unwrap()))
            .spawn()
            .unwrap();
        let began = std::time::Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if began.elapsed() > Duration::from_secs(90) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{mode}: compiler acceptance child hung");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = fs::read_to_string(&stdout).unwrap();
        let console = fs::read_to_string(&stderr).unwrap();
        assert!(status.success(), "{mode}: {output}\n{console}");
        let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        for (text, count) in [(&file, 1), (&console, 1), (&output, 0)] {
            assert_eq!(
                text.lines()
                    .filter(|l| l.contains(TARGET) && l.contains("phase=failure"))
                    .count(),
                count,
                "{text}"
            );
        }
        assert!(
            !console
                .lines()
                .any(|l| l.contains(TARGET) && l.contains("phase=start"))
        );
        assert!(!output.contains(TARGET));
        let operation = if mode == "portable" {
            "pointwise_compile"
        } else {
            "pointwise_native_pipeline"
        };
        assert!(file.contains(&format!("operation={operation} phase=start")));
        assert_eq!(
            file.lines()
                .filter(
                    |l| l.contains(TARGET) && l.contains("operation=pointwise_compile phase=start")
                )
                .count(),
            if warm && mode == "portable" { 3 } else { 2 }
        );
        assert_eq!(
            file.lines()
                .filter(|l| l.contains(TARGET)
                    && l.contains("operation=pointwise_native_pipeline phase=start"))
                .count(),
            (if mode == "portable" { 1 } else { 2 }) + usize::from(warm)
        );
    }
}
