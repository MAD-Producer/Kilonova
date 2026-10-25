//! Actual background compiler -> engine candidate -> GPU readback, with default
//! failure channels tested in a separate process that owns its native event loop.
use super::*;
use crate::i18n::Locale;
use crate::node_editor::{NodeEditor, Request};
use crate::node_tasks::{NodeTasks, ReadyPreview};
use std::time::{Duration, Instant};

fn compile(tasks: &mut NodeTasks, editor: &mut NodeEditor) -> ReadyPreview {
    tasks
        .submit(editor.request(Request::Compile).unwrap(), Locale::En, false)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        assert!(Instant::now() < deadline);
        let (_, mut ready) = tasks.poll(editor, Locale::En);
        if let Some(ready) = ready.pop() {
            assert!(editor.accept_compile(&ready.snapshot, Ok(ready.compiled.report.clone())));
            return ready;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn node_numeric_reuse_records_real_faults_and_restores_preview() {
    const CHILD: &str = "MOTION_NODE_REUSE_LOG_CHILD";
    let Some(root) = std::env::var_os(CHILD) else {
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "engine::numeric_reuse_tests::node_numeric_reuse_records_real_faults_and_restores_preview", "--nocapture"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        assert!(log.lines().any(|line| line.contains(" INFO ")
            && line.contains("start #")
            && line.contains("node_install")));
        let faults: Vec<_> = log
            .lines()
            .filter(|line| {
                line.contains(" ERROR ")
                    && line.contains("node_install")
                    && line.contains("error #")
            })
            .collect();
        assert_eq!(faults.len(), 2, "{log}");
        assert!(faults
            .iter()
            .all(|line| line.contains("ms:") && line.contains("node graph")));
        assert!(faults.iter().any(|line| line.contains("N1 compiled graph")));
        assert!(faults
            .iter()
            .any(|line| line.contains("uniform tail capacity")));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("N1 compiled graph") && stderr.contains("uniform tail capacity"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("node_install"));
        if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
            std::fs::create_dir_all(&out).unwrap();
            std::fs::copy(
                root.path().join("logs/motion-studio.log"),
                std::path::Path::new(&out).join("node-reuse-default.log"),
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&out).join("node-reuse-stderr.log"),
                &output.stderr,
            )
            .unwrap();
            std::fs::write(
                std::path::Path::new(&out).join("node-reuse-stdout.log"),
                &output.stdout,
            )
            .unwrap();
        }
        return;
    };
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        event_loop
            .create_window(winit::window::Window::default_attributes().with_visible(false))
            .unwrap(),
    );
    let engine = Engine::start(
        root.join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine
        .attach_async(window, 64, 64)
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mut graph = motion_graph::demo_graph();
    graph.public_parameters.clear();
    graph.nodes = vec![
        motion_graph::Node::new(
            "color",
            motion_graph::NodeOp::Color {
                value: [0.2, 0.3, 0.8, 1.],
            },
        ),
        motion_graph::Node::new("output", motion_graph::NodeOp::ColorOutput),
    ];
    graph.connections = vec![motion_graph::Connection::new(
        "color", "value", "output", "color",
    )];
    let mut editor = NodeEditor::new(graph);
    let mut tasks = NodeTasks::start().unwrap();
    let mut fingerprint = None;
    for i in 0..3 {
        if i != 0 {
            editor
                .set_constant("color", 0, 0.2 + 0.25 * i as f32)
                .unwrap();
        }
        let ready = compile(&mut tasks, &mut editor);
        assert_eq!(
            ready.compiled.report.portable_compilations,
            u64::from(i == 0)
        );
        engine.node_expect(ready.snapshot.preview_stamp()).unwrap();
        assert!(engine
            .node_install(ready.snapshot, ready.compiled)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap());
        let state = engine.state().unwrap();
        assert_eq!(
            state["desktop"]["nodePreview"]["candidatePipelineBuilds"],
            if i == 0 { 2 } else { 0 }
        );
        let capture = engine
            .node_capture(0., root.join(format!("preview-{i}.png")))
            .unwrap();
        if let Some(previous) = &fingerprint {
            assert_ne!(previous, &capture["fingerprint"]);
        }
        fingerprint = Some(capture["fingerprint"].clone());
    }
    for bad_shader in [true, false] {
        let mut ready = compile(&mut tasks, &mut editor);
        assert_eq!(ready.compiled.report.portable_compilations, 0);
        if bad_shader {
            ready.compiled.shader.wgsl = "this is not valid WGSL".into();
        } else {
            ready.compiled.manifest.effects[0]
                .shader_data
                .as_mut()
                .unwrap()
                .uniforms = vec![[0.; 4]; 33];
        }
        let error = engine
            .node_install(ready.snapshot, ready.compiled)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        assert!(error.contains("node graph"), "{error}");
        let capture = engine.node_capture(0., root.join("retained.png")).unwrap();
        assert_eq!(fingerprint.as_ref().unwrap(), &capture["fingerprint"]);
        assert_eq!(
            engine.state().unwrap()["desktop"]["nodePreview"]["installs"],
            3
        );
    }
    editor.set_constant("color", 0, 0.95).unwrap();
    let ready = compile(&mut tasks, &mut editor);
    assert_eq!(ready.compiled.report.portable_compilations, 0);
    engine.node_expect(ready.snapshot.preview_stamp()).unwrap();
    assert!(engine
        .node_install(ready.snapshot, ready.compiled)
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap());
    assert_eq!(
        engine.state().unwrap()["desktop"]["nodePreview"]["candidatePipelineBuilds"],
        0
    );
    assert_ne!(
        fingerprint.unwrap(),
        engine.node_capture(0., root.join("recovered.png")).unwrap()["fingerprint"]
    );
    drop(engine);
    crate::logging::flush();
}
