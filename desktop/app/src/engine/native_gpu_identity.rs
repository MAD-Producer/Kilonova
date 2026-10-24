//! Actual worker/surface/report identity acceptance. This small correctness
//! driver is not a steady-state performance or monitor scanout benchmark.
use super::{Command, Engine};
use crate::{workspace, Locale, Shell};
use motion_core::{render_sampling::Shutter, vector::*, Content, Ease, Layer, Project};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::{Command as ProcessCommand, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::event_loop::EventLoop;

fn study(
    event_loop: &mut EventLoop<()>,
    engine: &Engine,
    operation: fn(i64) -> motion_host::Result<Value>,
) -> Value {
    let reply = engine
        .request(|reply| Command::TestStudy { operation, reply })
        .unwrap();
    crate::native_test_support::receive(event_loop, &reply, Duration::from_secs(30))
        .unwrap()
        .unwrap()
}
fn start(id: i64) -> motion_host::Result<Value> {
    let commands = motion_host::ops::preview::start_command_profiling(id, 32)?;
    if commands["gpuCommands"].is_null() {
        return Ok(json!({"available":false}));
    }
    motion_host::ops::preview::start_command_trace(id)?;
    motion_host::ops::preview::start_profiling(id, 4)?;
    Ok(json!({"available":true}))
}
fn stop(id: i64) -> motion_host::Result<Value> {
    motion_host::ops::preview::stop_command_trace(id)?;
    let report = motion_host::ops::preview::stop_profiling(id)?;
    let commands = motion_host::ops::preview::stop_command_profiling(id)?;
    Ok(json!({"file":report["file"],"commands":commands}))
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_surface_reports_join_native_encoders_for_normal_and_exposure_frames() {
    const TEST: &str = "engine::native_gpu_identity::actual_surface_reports_join_native_encoders_for_normal_and_exposure_frames";
    const CHILD: &str = "MOTION_GPU_PRESENT_IDENTITY_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        use winit::platform::windows::EventLoopBuilderExtWindows;
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let mut event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
        let mut identities = Vec::new();
        let mut totals = 0;
        for (mode, exposure) in [("normal", false), ("exposure", true)] {
            let directory = root.join(mode);
            let mut project = Project::new(64, 64, 30, 60).unwrap();
            project.version = motion_model::CURRENT_PROJECT_VERSION;
            project.background = [0.0, 0.0, 0.0, 1.0];
            let mut layer = Layer::solid(
                1,
                "Native identity fixture",
                [16.0, 16.0],
                [24.0, 32.0, 0.0],
                [1.0; 4],
            );
            layer.content = Content::Vector {
                vector: VectorContent::shape(ShapeKind::Ellipse),
            };
            layer
                .transform
                .position
                .upsert(0, [24.0, 32.0, 0.0], Ease::Linear)
                .unwrap();
            layer
                .transform
                .position
                .upsert(30, [40.0, 32.0, 0.0], Ease::Linear)
                .unwrap();
            layer.sampling.motion_blur = exposure.then_some(Shutter {
                frames: 0.5,
                samples: 4,
            });
            project.layers.push(layer);
            motion_core::storage::save(&directory, &project).unwrap();
            let mut shell = Shell::new(directory.clone(), None, Some(Locale::En)).unwrap();
            shell.home = None;
            shell.workspace = workspace::Workspace::default();
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(1280, 800)),
                    )
                    .unwrap(),
            );
            shell.state = crate::native_test_support::receive(
                &mut event_loop,
                &shell
                    .engine
                    .attach_async(window.clone(), 1280, 800)
                    .unwrap(),
                Duration::from_secs(30),
            )
            .unwrap()
            .unwrap();
            shell.window = Some(window);
            shell.attached = true;
            let original = shell.state["project"].clone();
            if study(&mut event_loop, &shell.engine, start)["available"] != true {
                fs::write(root.join("report.json"), b"{\"available\":false}").unwrap();
                return;
            }
            for frame in [0.25, 1.25] {
                let reply = shell.engine.seek_async(frame).unwrap();
                shell.accept_edit_state(
                    crate::native_test_support::receive(
                        &mut event_loop,
                        &reply,
                        Duration::from_secs(30),
                    )
                    .unwrap()
                    .unwrap(),
                );
                let deadline = Instant::now() + Duration::from_secs(30);
                loop {
                    let (paint, actions) = shell.draw_commands([1280.0, 800.0]);
                    assert!(actions.is_empty());
                    let ui = shell.ui_frame(paint, [1280.0, 800.0], false);
                    assert!(ui.composition.is_some());
                    let reply = shell.engine.present_async(ui).unwrap();
                    let rendered = crate::native_test_support::receive(
                        &mut event_loop,
                        &reply,
                        Duration::from_secs(30),
                    )
                    .unwrap()
                    .unwrap();
                    if rendered {
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "native {mode} frame did not complete"
                    );
                }
            }
            let stopped = study(&mut event_loop, &shell.engine, stop);
            fs::write(
                directory.join("command-stop.json"),
                serde_json::to_vec_pretty(&stopped).unwrap(),
            )
            .unwrap();
            let report: Value =
                serde_json::from_slice(&fs::read(stopped["file"].as_str().unwrap()).unwrap())
                    .unwrap();
            let trace = &report["metadata"]["graphics"]["gpuCommandTrace"];
            assert_eq!(trace["truncated"], false);
            assert_eq!(trace["omittedRecords"], 0);
            assert!(trace["active"].as_array().unwrap().is_empty());
            let commands = trace["records"].as_array().unwrap();
            let frames = report["frames"].as_array().unwrap();
            assert_eq!(frames.len(), 2, "{mode}: {report}");
            assert_eq!(report["presentationCommandIdentityCount"], 2);
            for entry in frames {
                let identity = &entry["presentationCommand"];
                let matched: Vec<_> = commands
                    .iter()
                    .filter(|record| {
                        record["epoch"] == identity["epoch"] && record["id"] == identity["id"]
                    })
                    .collect();
                assert_eq!(matched.len(), 1, "{mode}: {entry}");
                let record = matched[0];
                assert_eq!(record["phase"], "complete");
                assert_eq!(record["stage"], "composition_and_presentation");
                let words = record["timestampWords"].as_array().unwrap();
                assert!(words[1].as_u64().unwrap() > words[0].as_u64().unwrap());
                assert!(record["gpuElapsedUs"]
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v > 0.0));
                assert_eq!(record["contexts"].as_array().unwrap().len(), 1);
                assert!(record["contexts"][0]["request"].as_u64().is_some());
                if exposure {
                    assert_eq!(record["exposure"], entry["exposure"]);
                    assert!(entry["exposure"]["request"].as_u64().is_some());
                    for stage in ["exposure_phase", "exposure_resolve"] {
                        assert!(commands.iter().any(|record| record["stage"] == stage
                            && record["exposure"]["request"] == entry["exposure"]["request"]));
                    }
                } else {
                    assert!(entry["exposure"].is_null());
                }
                identities.push(
                    json!({"mode":mode,"sequence":entry["sequence"],"frame":entry["frame"],
                    "command":identity,"exposure":entry["exposure"]}),
                );
            }
            totals += commands.len();
            assert_eq!(shell.engine.state().unwrap()["project"], original);
            assert!(!shell.engine.failed());
        }
        fs::write(root.join("report.json"),serde_json::to_vec_pretty(&json!({
            "available":true,"actualSuccessfulPresentations":4,"actualCommandRecords":totals,
            "identities":identities,"normalAndExposure":true,
            "scope":"actual native surface via worker, real successful FrameRecorder entries and original query words; no scanout, scene pixel-oracle or steady-state performance claim"
        })).unwrap()).unwrap();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = match std::env::var_os("MOTION_GPU_PRESENT_IDENTITY_ARTIFACTS") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(!path.exists());
            fs::create_dir(&path).unwrap();
            path
        }
        None => temporary.path().to_path_buf(),
    };
    let stdout = root.join("stdout.txt");
    let stderr = root.join("stderr.txt");
    let mut child = ProcessCommand::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_UI_LAYOUT", root.join("layout.json"))
        .env_remove("MOTION_LOG")
        .stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native identity child timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "child status={status}\n{}\n{}",
        fs::read_to_string(stdout).unwrap(),
        fs::read_to_string(stderr).unwrap()
    );
    let report: Value =
        serde_json::from_slice(&fs::read(root.join("report.json")).unwrap()).unwrap();
    if report["available"] != true {
        eprintln!("GPU_PRESENT_IDENTITY available=false; no native surface identity acceptance");
        return;
    }
    assert_eq!(report["actualSuccessfulPresentations"], 4);
    eprintln!("GPU_PRESENT_IDENTITY {report}");
}
