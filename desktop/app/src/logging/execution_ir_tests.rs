//! Real IR checks and GPU state through the owning worker and production sink.
use super::*;
use crate::engine::{Command as EngineCommand, Engine};
use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
use motion_effects::execution_plan::ExecutionPlanError;
use motion_render::{
    CaptureTarget, RenderError, Renderer, Scene, effect_plan::ExecutionDiagnosticFault,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    process::{Command, Stdio},
};

struct Fixture {
    scene: Scene,
    target: CaptureTarget,
    pixels: Vec<u8>,
    builds: u64,
    failures: Vec<Value>,
    recoveries: usize,
}
thread_local! {
    // GPU resources are created, used and released on the actual worker.
    static FIXTURE: RefCell<Option<Fixture>> = const { RefCell::new(None) };
}
const CHILD: &str = "MOTION_EXECUTION_IR_CHILD";
const TEST: &str = "logging::execution_ir_tests::execution_ir_faults_reach_default_worker_logs_and_preserve_gpu_pixels";

fn root() -> PathBuf {
    PathBuf::from(std::env::var_os(CHILD).unwrap())
}
fn setup(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |session| {
        let mut project = Project::new(32, 32, 30, 60)?;
        let package = motion_effects::builtin::package()?;
        let mut layer = Layer::solid(
            47,
            "IR acceptance",
            [32.; 2],
            [16., 16., 0.],
            [0.2, 0.4, 0.6, 1.],
        );
        for (instance, name) in [(11, "tint"), (12, "invert")] {
            let definition = package
                .manifest
                .effects
                .iter()
                .find(|e| e.id == name)
                .unwrap();
            layer.effects.push(EffectInstance::new(
                instance,
                &package.manifest.id,
                &package.manifest.version,
                &package.hash,
                definition,
                layer.size,
            ));
        }
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate()?;
        let mut scene = Scene::new(&project)?;
        scene.sample(&project, 0., None, &ExpressionEvaluator)?;
        let mut renderer = pollster::block_on(Renderer::headless())?;
        let target = renderer.capture_target(32, 32)?;
        let pixels = renderer.capture(&scene, &target)?.0;
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
        let builds = renderer.effect_pipeline_builds();
        assert!(builds > 0);
        let adapter = renderer.adapter.get_info();
        fs::write(root().join("adapter.json"), serde_json::to_vec_pretty(&json!({
            "name": adapter.name, "driver": adapter.driver, "backend": format!("{:?}", adapter.backend),
        })).unwrap()).unwrap();
        fs::write(root().join("baseline.rgba"), &pixels).unwrap();
        FIXTURE.with(|state| {
            *state.borrow_mut() = Some(Fixture {
                scene,
                target,
                pixels,
                builds,
                failures: vec![],
                recoveries: 0,
            })
        });
        session.editor_renderer = Some(renderer);
        Ok(json!({"ready": true, "native_builds": builds}))
    })
}
fn typed(error: &RenderError) -> &ExecutionPlanError {
    match error {
        RenderError::ExecutionPlan(cause) => cause,
        RenderError::Context { cause, .. } | RenderError::Diagnostic { cause, .. } => typed(cause),
        RenderError::Shared(cause) => typed(cause),
        other => panic!("original execution-IR cause was lost: {other:?}"),
    }
}
fn fail<const MODE: u8>(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |session| {
        FIXTURE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            let renderer = session.editor_renderer.as_mut().unwrap();
            let fault = match MODE {
                0 => ExecutionDiagnosticFault::Cycle,
                1 => ExecutionDiagnosticFault::TypeMismatch,
                2 => ExecutionDiagnosticFault::PassBudget,
                _ => unreachable!(),
            };
            renderer.debug_inject_execution_fault(fault);
            let primary = renderer
                .draw(&state.scene, &state.target.view, 32, 32)
                .unwrap_err();
            match (MODE, typed(&primary)) {
                (0, ExecutionPlanError::Cycle { members, total }) => {
                    assert_eq!(*total, 1);
                    assert_eq!(members.len(), 1);
                }
                (
                    1,
                    ExecutionPlanError::TypeMismatch {
                        expected, actual, ..
                    },
                ) => {
                    assert_ne!(expected.image.precision, actual.image.precision);
                }
                (
                    2,
                    ExecutionPlanError::Budget {
                        node: Some(_),
                        resource: "passes",
                        required,
                        maximum,
                    },
                ) => {
                    assert!(required > maximum);
                }
                (_, cause) => panic!("wrong actual IR refusal for {fault:?}: {cause:?}"),
            }
            assert_eq!(renderer.effect_pipeline_builds(), state.builds);
            assert_eq!(renderer.read_target(&state.target)?, state.pixels);
            let cause = primary.to_string();
            assert!(
                cause.contains("phase=effect_ir_check") && cause.contains("layer=47"),
                "{cause}"
            );
            assert!(cause.contains("effect=Some(11)"), "{cause}");
            state.failures.push(json!({"mode": MODE, "cause": cause,
            "pixels_preserved": true, "pipeline_builds_added": 0}));
            Err(primary.into())
        })
    })
}
fn recover(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |session| {
        FIXTURE.with(|state| {
            let mut state = state.borrow_mut();
            let state = state.as_mut().unwrap();
            let renderer = session.editor_renderer.as_mut().unwrap();
            // Same renderer, same target and original scene; one-shot corruption
            // has been consumed by the failed checker, not hidden by a new owner.
            renderer.draw(&state.scene, &state.target.view, 32, 32)?;
            assert_eq!(renderer.read_target(&state.target)?, state.pixels);
            assert_eq!(renderer.effect_pipeline_builds(), state.builds);
            state.recoveries += 1;
            fs::write(
                root().join("receipt.json"),
                serde_json::to_vec_pretty(&json!({
                    "failures": state.failures, "recoveries": state.recoveries,
                    "same_renderer": true, "healthy_pixels_equal": true, "pipeline_builds_added": 0,
                }))
                .unwrap(),
            )
            .unwrap();
            Ok(json!({"recovered": true}))
        })
    })
}
fn cleanup(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |session| {
        // Release the test-owned target before its renderer and before TLS
        // destruction. GPU teardown may itself access thread-local state.
        FIXTURE.with(|state| drop(state.borrow_mut().take()));
        session.editor_renderer = None;
        Ok(json!({"released": true}))
    })
}
fn call(
    engine: &Engine,
    operation: fn(i64) -> motion_host::Result<Value>,
) -> Result<Value, String> {
    engine.call(|reply| EngineCommand::TestStudy { operation, reply })
}

#[test]
fn execution_ir_faults_reach_default_worker_logs_and_preserve_gpu_pixels() {
    if std::env::var_os(CHILD).is_some() {
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let _logger = init(root().join("logs")).unwrap();
        let engine = Engine::start(
            root().join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let before = engine.state().unwrap();
        assert_eq!(call(&engine, setup).unwrap()["ready"], true);
        // Two independent failure/recovery episodes for every category.
        for _ in 0..2 {
            for operation in [fail::<0>, fail::<1>, fail::<2>] {
                let reason = call(&engine, operation).unwrap_err();
                assert!(reason.contains("phase=effect_ir_check"));
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
                assert_eq!(call(&engine, recover).unwrap()["recovered"], true);
            }
        }
        assert_eq!(call(&engine, cleanup).unwrap()["released"], true);
        drop(engine);
        flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let base = std::env::var_os("MOTION_EXECUTION_ARTIFACTS").map(PathBuf::from);
    let directory = base
        .as_deref()
        .unwrap_or(temporary.path())
        .join("default-ir-faults");
    fs::create_dir_all(&directory).unwrap();
    let stdout_path = directory.join("stdout.log");
    let stderr_path = directory.join("stderr.log");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, &directory)
        .env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(180) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("actual execution-IR fault acceptance child hung");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(status.success(), "{stdout}\n{stderr}");
    let file = fs::read_to_string(directory.join("logs/motion-studio.log")).unwrap();
    for (output, expected) in [(&file, 6), (&stderr, 6), (&stdout, 0)] {
        let lines: Vec<_> = output
            .lines()
            .filter(|line| line.contains("error #") && line.contains("phase=effect_ir_check"))
            .collect();
        assert_eq!(lines.len(), expected, "{output}");
        for line in lines {
            assert!(
                line.contains("test_study") && line.contains("ms:") && line.contains("layer=47")
            );
            let request = line
                .split_once("error #")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap();
            let start = format!("start #{request} test_study");
            assert!(
                file.find(&start).unwrap()
                    < file.find(line.split_once("error #").unwrap().1).unwrap()
            );
        }
    }
    for category in [
        "phase=topology: cycle",
        "phase=value_contract:",
        "phase=admission:",
    ] {
        assert_eq!(
            file.lines()
                .filter(|line| line.contains("error #") && line.contains(category))
                .count(),
            2
        );
        assert!(stderr.contains(category));
        assert!(!stdout.contains(category));
    }
    assert!(!stderr.contains("start #"));
    assert!(!stdout.contains("start #"));
    let receipt: Value =
        serde_json::from_slice(&fs::read(directory.join("receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["failures"].as_array().unwrap().len(), 6);
    assert_eq!(receipt["recoveries"], 6);
    assert_eq!(receipt["same_renderer"], true);
}
