use super::*;
use motion_render::{
    exposure_scene::{ExposureDocument, ExposureScenes},
    ExposureFrame, ExposurePoll, RenderError, Renderer,
};
use std::{error::Error, sync::Arc};

fn native_io<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a std::io::Error> {
    if let Some(render) = error.downcast_ref::<RenderError>() {
        return match render {
            RenderError::Io(cause) => Some(cause),
            RenderError::Diagnostic { cause, .. } | RenderError::Context { cause, .. } => {
                native_io(cause.as_ref())
            }
            RenderError::Shared(cause) => native_io(cause.as_ref()),
            _ => None,
        };
    }
    if let Some(cause) = error.downcast_ref::<Box<RenderError>>() {
        return native_io(cause.as_ref());
    }
    if let Some(cause) = error.downcast_ref::<Arc<RenderError>>() {
        return native_io(cause.as_ref());
    }
    error.source().and_then(native_io)
}

fn project() -> motion_model::Project {
    let mut project = motion_model::Project::new(32, 32, 30, 60).unwrap();
    let mut layer =
        motion_model::Layer::solid(7, "Exposure source", [8.; 2], [16., 16., 0.], [1.; 4]);
    layer.sampling.motion_blur = Some(motion_model::render_sampling::Shutter {
        frames: 2.,
        samples: 4,
    });
    project.layers.push(layer);
    project
}

fn task(
    renderer: &mut Renderer,
    project: &motion_model::Project,
) -> ExposureFrame<motion_core::ExpressionEvaluator> {
    let document = ExposureDocument::new(Arc::new(project.clone())).unwrap();
    let scenes = ExposureScenes::new(
        document,
        &project.composition_id,
        1.,
        false,
        None,
        motion_core::ExpressionEvaluator,
    )
    .unwrap();
    ExposureFrame::new(renderer, scenes, 32, 32).unwrap()
}

#[test]
fn actual_exposure_io_and_terminal_replay_reach_default_reply_channels_then_new_task_recovers() {
    const CHILD: &str = "MOTION_EXPOSURE_NATIVE_ERROR_CHILD";
    const NAME: &str = "engine::failure::composition_tests::plan_error::exposure_frame::actual_exposure_io_and_terminal_replay_reach_default_reply_channels_then_new_task_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let project = project();
        let before = serde_json::to_value(&project).unwrap();
        let mut failed = task(&mut renderer, &project);
        let missing = root.join("missing-exposure-source.bin");
        let mut calls = 0;
        let mut pointer = None;
        let mut causes = Vec::new();
        for request in 1501..1503 {
            let failure = failed
                .poll(&mut renderer, Duration::from_secs(1), |_, _| {
                    calls += 1;
                    let cause = std::fs::File::open(&missing).unwrap_err();
                    Err(RenderError::Context {
                        message: format!("source path={}: {cause}", missing.display()),
                        cause: Box::new(cause.into()),
                    })
                })
                .unwrap_err();
            let host = motion_host::Error::from(failure);
            let original = host.to_string();
            let io = native_io(&host)
                .expect("original IO must survive reply conversion and terminal replay");
            assert_eq!(io.kind(), std::io::ErrorKind::NotFound);
            let current = io as *const std::io::Error as usize;
            if let Some(expected) = pointer {
                assert_eq!(current, expected);
            } else {
                pointer = Some(current);
            }
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_exposure_poll",
                started: Instant::now(),
            }));
            respond(reply, Err(host));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        assert_eq!(calls, 1, "terminal task must not run preparation again");
        assert_eq!(
            causes[0], causes[1],
            "short diagnostic must replay unchanged"
        );
        assert_eq!(serde_json::to_value(&project).unwrap(), before);
        drop(failed);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        let mut recovered = task(&mut renderer, &project);
        let mut complete = false;
        for _ in 0..64 {
            if matches!(
                recovered
                    .poll(&mut renderer, Duration::from_secs(1), |_, _| Ok(true))
                    .unwrap(),
                ExposurePoll::Ready(_)
            ) {
                complete = true;
                break;
            }
        }
        assert!(complete);
        assert!(renderer
            .capture_exposure(recovered.target())
            .unwrap()
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0));
        drop(recovered);
        assert_eq!(renderer.exposure_texture_bytes(), 0);
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
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
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 2);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (1501..1503).zip(causes) {
        let marker = format!("error #{request} test_exposure_poll ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1);
            for part in [
                &cause,
                "exposure composition=",
                "phase=task_Receiver",
                "sample=",
                "sample_frame=",
                "object=",
                "source path=",
                "missing-exposure-source.bin",
                "ms:",
            ] {
                assert!(lines[0].contains(part), "{text}");
            }
        }
        assert!(!stdout.contains(&cause));
        assert!(!stdout.contains(&marker));
    }
    assert!(!stderr.contains("logging initialized"));
    assert!(!stdout.contains("logging initialized"));
}
