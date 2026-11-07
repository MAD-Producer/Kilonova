use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_session_checkpoints_complete_ordinary_and_exposure_frames_and_restores_on_reattach() {
    const CASE:&str="session::pipeline_preparation::tests::native::cache::actual_session_checkpoints_complete_ordinary_and_exposure_frames_and_restores_on_reattach";
    const CHILD: &str = "MOTION_CACHE_SESSION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CASE, "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let mut receipts = Vec::new();
    for exposure in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (mut project, _) = fixture();
        if exposure {
            project.layers[0].sampling.motion_blur = Some(motion_core::render_sampling::Shutter {
                frames: 2.,
                samples: 4,
            });
            for (frame, x) in [(0, 3.), (2, 13.)] {
                project.layers[0]
                    .transform
                    .position
                    .upsert(frame, [x, 8., 0.], motion_core::Ease::Linear)
                    .unwrap();
            }
        }
        let mut session = Session::new(
            project,
            root.path().into(),
            Arc::new(crate::session::last_error_observation_tests::Stub),
        )
        .unwrap();
        session.preview.mode = motion_render::PreviewMode::High;
        let calls = Arc::new(AtomicUsize::new(0));
        session
            .set_driver_pipeline_cache_storage(super::super::cache::storage(
                &root.path().join("cache"),
                calls.clone(),
            ))
            .unwrap();
        attach(&events, &mut session);
        let original = session.engine.snapshot();
        complete(&mut session, 1.);
        assert_eq!(session.sampling.required(), exposure);
        let first = front(&session);
        let g = session.graphics.as_ref().unwrap();
        if !g.renderer.driver_pipeline_cache_info().supported {
            assert_eq!(
                session.pipeline_preparation.status()["driverCache"]["load"],
                "unsupported"
            );
            session.detach();
            continue;
        }
        let context = Context {
            document_generation: session.document_generation,
            project_revision: session.engine.revision(),
        };
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            session
                .pipeline_preparation
                .collect(&mut session.graphics.as_mut().unwrap().renderer, context)
                .unwrap();
            if session.pipeline_preparation.status()["driverCache"]["task"]["phase"] == "saved" {
                break;
            }
            assert!(Instant::now() < deadline, "{}", session.preview_info());
            std::thread::yield_now();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let worker = session
            .pipeline_preparation
            .worker
            .as_ref()
            .unwrap()
            .thread
            .thread()
            .id();
        let cold = session.pipeline_preparation.status();
        session.detach();
        attach(&events, &mut session);
        complete(&mut session, 1.);
        assert_eq!(
            session.pipeline_preparation.status()["driverCache"]["load"],
            "restored"
        );
        assert_eq!(
            front(&session),
            first,
            "cache restore changed a completed visible frame"
        );
        assert_eq!(session.engine.snapshot(), original);
        assert_eq!(
            session
                .pipeline_preparation
                .worker
                .as_ref()
                .unwrap()
                .thread
                .thread()
                .id(),
            worker
        );
        assert_eq!(
            session
                .graphics
                .as_ref()
                .unwrap()
                .renderer
                .effect_pipeline_cache_info()
                .foreground_builds,
            0
        );
        let g = session.graphics.as_ref().unwrap();
        receipts.push(json!({"exposure":exposure,"adapter":g.renderer.adapter_info.name,
            "backend":format!("{:?}",g.renderer.adapter_info.backend),"cold":cold,
            "warm":session.pipeline_preparation.status(),"allPixelsEqual":true,"projectUnchanged":true,
            "foregroundBuilds":0,"workerRetained":true}));
        session.detach();
    }
    if let Some(evidence) = std::env::var_os("MOTION_HOST_CACHE_EVIDENCE") {
        let evidence = std::path::PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(
            evidence.join("session-surface.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
    }
}
