//! Actual preparation failure and blocking state through the production logger.
//! Native shader-source preservation has separate native-compiler tests.
use super::*;
use motion_core::{ExpressionEvaluator, Project};
use motion_render::{
    PipelinePreparationContext, PipelinePreparationError, PipelinePreparationPhase, RenderError,
    Renderer, Scene,
};
use std::{
    process::{Command, Stdio},
    sync::mpsc,
};

const NAME:&str="logging::pipeline_preparation_tests::preparation_start_precedes_blocking_and_failure_reaches_default_sinks";
const CHILD: &str = "MOTION_PIPELINE_LOG_CHILD";
const TARGET: &str = "motion_render::pipeline_preparation";

#[test]
fn preparation_start_precedes_blocking_and_failure_reaches_default_sinks() {
    if let Some(root) = std::env::var_os(CHILD) {
        assert!(std::env::var_os("MOTION_LOG").is_none());
        let root = PathBuf::from(root);
        let _guard = init(root.join("logs")).unwrap();
        let mut project = Project::new(16, 16, 30, 60).unwrap();
        project.background = [0.2, 0.4, 0.6, 1.];
        let mut scene = Scene::new(&project).unwrap();
        scene
            .sample(&project, 0., None, &ExpressionEvaluator)
            .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let context = PipelinePreparationContext {
            document_generation: 104,
            project_revision: 7,
        };
        let job = renderer
            .prepare_effect_pipeline_job(vec![scene.clone()], 16, 16, context)
            .unwrap();
        let ticket = job.ticket();
        let request = ticket.status().request;
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            job.run_with_scenes(|scenes| {
                started_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                scenes.clear(); // Actual API admission failure, not a mocked result.
                Ok(())
            })
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(ticket.status().phase, PipelinePreparationPhase::Planning);
        assert_eq!(ticket.status().compiled_pipelines, 0);
        flush();
        let before = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        let start = before
            .lines()
            .find(|line| {
                line.contains(TARGET) && line.contains(&format!("start request={request} "))
            })
            .unwrap();
        assert!(start.contains("INFO") && start.contains("generation=104 revision=7 phase=plan"));
        assert!(!before.contains("pipeline preparation failed"));
        release_tx.send(()).unwrap();
        let failure = match worker.join().unwrap() {
            Err(error) => error,
            Ok(_) => panic!("invalid scene collection accepted"),
        };
        assert!(matches!(
            &failure,
            RenderError::PipelinePreparation(PipelinePreparationError::SceneCount)
        ));
        assert_eq!(ticket.status().phase, PipelinePreparationPhase::Failed);
        let cause = failure.to_string();
        for _ in 0..65 {
            assert_eq!(ticket.status().phase, PipelinePreparationPhase::Failed);
        }
        let job = renderer
            .prepare_effect_pipeline_job(vec![scene.clone()], 16, 16, context)
            .unwrap();
        let healthy = job.ticket();
        let prepared = std::thread::spawn(move || job.run())
            .join()
            .unwrap()
            .unwrap();
        renderer
            .install_prepared_effect_pipelines(prepared, context)
            .unwrap();
        assert_eq!(healthy.status().phase, PipelinePreparationPhase::Installed);
        let target = renderer.capture_target(16, 16).unwrap();
        let pixels = renderer.capture(&scene, &target).unwrap().0;
        assert!(pixels.chunks_exact(4).all(|pixel| pixel[3] == 255));
        renderer.check_health().unwrap();
        flush();
        let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert_eq!(file.lines().filter(|line|line.contains(TARGET)&&line.contains("pipeline preparation failed")).count(),1);
        assert!(file.contains(&cause));
        fs::write(root.join("expected-cause.txt"), cause).unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let artifact = std::env::var_os("MOTION_PIPELINE_LOG_EVIDENCE").map(PathBuf::from);
    let root = artifact.as_deref().unwrap_or(temp.path());
    fs::create_dir_all(root).unwrap();
    let stdout = root.join("stdout.txt");
    let stderr = root.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, root)
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
            panic!("preparation logging acceptance child hung");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let file = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let cause = fs::read_to_string(root.join("expected-cause.txt")).unwrap();
    for text in [&file, &console] {
        let failures: Vec<_> = text
            .lines()
            .filter(|line| line.contains(TARGET) && line.contains("pipeline preparation failed"))
            .collect();
        assert_eq!(failures.len(), 1, "{text}");
        assert!(failures[0].contains("generation=104 revision=7 elapsed_ms="));
        assert!(failures[0].contains(&cause));
    }
    assert!(file
        .lines()
        .any(|line| line.contains(TARGET) && line.contains("INFO") && line.contains("phase=plan")));
    assert!(!console
        .lines()
        .any(|line| line.contains(TARGET) && line.contains("preparation start")));
    assert!(
        !output.contains(TARGET) && !output.contains(&cause),
        "stdout carried logger records"
    );
}
