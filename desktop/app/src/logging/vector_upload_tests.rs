use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Content, Layer, Project,
};
use motion_render::{Renderer, Scene};
use std::process::{Command, Stdio};

#[test]
fn native_vector_upload_failure_reaches_default_file_and_stderr() {
    const CHILD: &str = "MOTION_VECTOR_UPLOAD_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = init(root.join("logs")).unwrap();
        let mut project = Project::new(32, 32, 30, 60).unwrap();
        project.version = 21;
        let mut layer = Layer::solid(47, "Vector", [16.; 2], [16., 16., 0.], [1.; 4]);
        layer.content = Content::Vector {
            vector: VectorContent::shape(ShapeKind::Rectangle),
        };
        project.layers.push(layer);
        project.validate().unwrap();
        let mut scene = Scene::new(&project);
        scene
            .sample(&project, 3.25, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let target = renderer.render_target(32, 32).unwrap();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        renderer
            .encode(&scene, &target.view, 32, 32, &mut encoder, None)
            .unwrap();
        assert_eq!(renderer.vector_upload_info().unwrap().pending_buffers, 1);
        // Destroy the native device between encoding and submitting. Deferred
        // source remapping must report the genuine BufferAsyncError exactly once.
        renderer.device.destroy();
        renderer.queue.submit([encoder.finish()]);
        let poll = renderer.device.poll(wgpu::PollType::wait_indefinitely());
        let info = renderer.vector_upload_info().unwrap();
        assert_eq!(info.remap_failures_total, 1, "poll={poll:?} info={info:?}");
        assert_eq!(info.pending_buffers, 0);
        assert!(renderer.check_health().is_err());
        flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout = root.path().join("stdout.txt");
    let stderr = root.path().join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "logging::vector_upload_tests::native_vector_upload_failure_reaches_default_file_and_stderr", "--nocapture"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG")
        .stdout(Stdio::from(File::create(&stdout).unwrap()))
        .stderr(Stdio::from(File::create(&stderr).unwrap())).spawn().unwrap();
    let began = std::time::Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if began.elapsed() > Duration::from_secs(45) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("native vector upload child did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = fs::read_to_string(stdout).unwrap();
    let console = fs::read_to_string(stderr).unwrap();
    assert!(status.success(), "{output}\n{console}");
    let file = fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
    for (text, count) in [(&file, 1), (&console, 1), (&output, 0)] {
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("site=render.vector_upload phase=remap"))
                .count(),
            count,
            "{text}"
        );
    }
    assert!(file.contains("layer=47 frame=3.25 bytes="));
    assert!(file.contains("cause=Error occurred when trying to async map a buffer"));
}
