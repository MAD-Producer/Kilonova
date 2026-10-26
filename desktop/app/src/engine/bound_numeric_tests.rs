//! Real compiler worker -> bound install -> native effect rendering -> disk.
use super::*;
use crate::{node_editor::NodeEditor, node_tasks::NodeTasks};

fn probe(id: i64) -> motion_host::Result<Value> {
    motion_host::session::with_session(id, |s| {
        s.sample()?;
        let effect = &s.engine.project().layers[0].effects[0];
        let package = s
            .effects
            .registry
            .resolve(&effect.plugin, &effect.version, &effect.hash)?;
        let identity = serde_json::json!({"hash":effect.hash,"package_pointer":Arc::as_ptr(&package) as usize});
        let graphics = s.graphics.as_mut().ok_or("native GPU is missing")?;
        let target = graphics.renderer.capture_target(64, 64)?;
        let (pixels, stats) = graphics.renderer.capture(&s.scene, &target)?;
        let fingerprint = pixels.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        Ok(
            serde_json::json!({"identity":identity,"fingerprint":fingerprint,
            "pipeline_builds":graphics.renderer.effect_pipeline_builds(),
            "private_upload_bytes":stats.parameter_resource_upload_bytes}),
        )
    })
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn node_bound_worker_reuses_canonical_and_native_programs_with_default_failure_channels() {
    const CHILD: &str = "MOTION_BOUND_NUMERIC_CHILD";
    const NAME: &str = "engine::bound_numeric_tests::node_bound_worker_reuses_canonical_and_native_programs_with_default_failure_channels";
    let Some(root) = std::env::var_os(CHILD) else {
        let folder = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, folder.path())
            .env_remove("MOTION_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let log = std::fs::read_to_string(folder.path().join("logs/motion-studio.log")).unwrap();
        assert!(
            log.lines().any(|l| l.contains(" INFO ")
                && l.contains("start #")
                && l.contains("node_install"))
        );
        let faults: Vec<_> = log
            .lines()
            .filter(|l| {
                l.contains(" ERROR ") && l.contains("node_install") && l.contains("error #")
            })
            .collect();
        assert_eq!(faults.len(), 2, "{log}");
        assert!(
            faults
                .iter()
                .all(|l| l.contains("ms:") && l.contains("node canonical graph"))
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("N1 compiled graph") && stderr.contains("uniform tail capacity"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("node_install"));
        if let Some(out) = std::env::var_os("MOTION_EXECUTION_ARTIFACTS") {
            let out = PathBuf::from(out);
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(out.join("bound-worker-default.log"), log).unwrap();
            std::fs::write(out.join("bound-worker-stderr.log"), output.stderr).unwrap();
            std::fs::write(out.join("bound-worker-stdout.log"), output.stdout).unwrap();
            std::fs::copy(
                folder.path().join("rows.json"),
                out.join("bound-worker-rows.json"),
            )
            .unwrap();
        }
        return;
    };
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(root);
    let _logger = crate::logging::init(root.join("logs")).unwrap();
    let mut events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(winit::window::Window::default_attributes().with_visible(false))
            .unwrap(),
    );
    let project_root = root.join("project");
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Layer",
        [64.; 2],
        [32., 32., 0.],
        [1.; 4],
    ));
    motion_core::storage::save(&project_root, &project).unwrap();
    let engine = Engine::start(
        project_root.clone(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let attached = engine.attach_async(window, 64, 64).unwrap();
    crate::native_test_support::receive(&mut events, &attached, Duration::from_secs(60))
        .unwrap()
        .unwrap();
    let mut graph = motion_graph::demo_graph();
    let parameter = graph.public_parameters[0].clone();
    graph.public_parameters = (0..32)
        .map(|i| {
            let mut p = parameter.clone();
            p.id = format!("p{i}");
            p
        })
        .collect();
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
    editor.layer_target = Some(crate::node_layer::Target {
        root: project_root.clone(),
        composition: project.composition_id.clone(),
        object: 1,
        effect: 1,
        key: "bound-worker".into(),
        label: "Layer".into(),
        creating: true,
    });
    editor
        .toggle_preview(motion_graph::PortRef::new("color", "value"))
        .unwrap();
    let mut tasks = NodeTasks::start().unwrap();
    let mut rows = vec![];
    for i in 0..3 {
        if i > 0 {
            editor
                .set_constant("color", 0, 0.2 + 0.3 * i as f32)
                .unwrap();
        }
        let ready = super::numeric_reuse_tests::compile(&mut tasks, &mut editor);
        assert_eq!(
            ready.compiled.report.portable_compilations,
            u64::from(i == 0)
        );
        assert!(ready.canonical.is_some());
        if i > 0 {
            assert_eq!(
                ready
                    .canonical
                    .as_ref()
                    .unwrap()
                    .report
                    .portable_compilations,
                0
            );
        }
        engine.node_expect(ready.snapshot.preview_stamp()).unwrap();
        let reply = engine
            .node_install_bound(ready.snapshot, ready.compiled, ready.canonical)
            .unwrap();
        assert!(
            crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                .unwrap()
                .unwrap()
        );
        let reply = engine
            .request(|reply| Command::TestStudy {
                operation: probe,
                reply,
            })
            .unwrap();
        let row = crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
            .unwrap()
            .unwrap();
        if let Some(first) = rows.first() {
            let first: &Value = first;
            assert_eq!(row["identity"], first["identity"]);
            assert_eq!(row["pipeline_builds"], first["pipeline_builds"]);
            assert_ne!(row["fingerprint"], first["fingerprint"]);
            let state = engine.state().unwrap();
            assert_eq!(
                state["desktop"]["nodePreview"]["candidatePipelineBuilds"],
                0
            );
            assert_eq!(
                state["desktop"]["nodePreview"]["canonicalCandidatePipelineBuilds"],
                0
            );
        }
        rows.push(row);
    }
    for bad_shader in [true, false] {
        let mut ready = super::numeric_reuse_tests::compile(&mut tasks, &mut editor);
        let canonical = ready.canonical.as_mut().unwrap();
        if bad_shader {
            canonical.shader.wgsl = "not valid WGSL".into();
        } else {
            canonical.manifest.effects[0]
                .shader_data
                .as_mut()
                .unwrap()
                .uniforms = vec![[0.; 4]; 33];
        }
        let reply = engine
            .node_install_bound(ready.snapshot, ready.compiled, ready.canonical)
            .unwrap();
        let reason =
            crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                .unwrap()
                .unwrap_err();
        assert!(reason.contains("node canonical graph"), "{reason}");
        let reply = engine
            .request(|reply| Command::TestStudy {
                operation: probe,
                reply,
            })
            .unwrap();
        let retained =
            crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
                .unwrap()
                .unwrap();
        for key in ["identity", "fingerprint", "pipeline_builds"] {
            assert_eq!(
                retained[key],
                rows.last().unwrap()[key],
                "failed candidate must preserve {key}"
            );
        }
        assert_eq!(retained["private_upload_bytes"], 0);
    }
    editor.set_constant("color", 0, 0.95).unwrap();
    let ready = super::numeric_reuse_tests::compile(&mut tasks, &mut editor);
    engine.node_expect(ready.snapshot.preview_stamp()).unwrap();
    let reply = engine
        .node_install_bound(ready.snapshot, ready.compiled, ready.canonical)
        .unwrap();
    assert!(
        crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
            .unwrap()
            .unwrap()
    );
    let reply = engine
        .request(|reply| Command::TestStudy {
            operation: probe,
            reply,
        })
        .unwrap();
    let recovered =
        crate::native_test_support::receive(&mut events, &reply, Duration::from_secs(60))
            .unwrap()
            .unwrap();
    assert_eq!(recovered["identity"], rows[0]["identity"]);
    assert_ne!(
        recovered["fingerprint"],
        rows.last().unwrap()["fingerprint"]
    );
    rows.push(recovered);
    engine.save().unwrap();
    drop(engine);
    let loaded = motion_core::storage::load(&project_root).unwrap();
    loaded.validate().unwrap();
    let mut reopened =
        motion_host::Session::new(loaded, project_root, motion_desktop_media::platform()).unwrap();
    reopened.sample().unwrap();
    assert_eq!(
        reopened.engine.project().layers[0].effects[0].hash,
        rows[0]["identity"]["hash"].as_str().unwrap()
    );
    let data = reopened.engine.project().layers[0].effects[0]
        .shader_data
        .as_ref()
        .unwrap();
    assert!(!data.texture.is_empty());
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    renderer.set_effect_registry(reopened.effects.registry.clone());
    let target = renderer.capture_target(64, 64).unwrap();
    let pixels = renderer.capture(&reopened.scene, &target).unwrap().0;
    let fingerprint = pixels.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(
        fingerprint,
        rows.last().unwrap()["fingerprint"].as_u64().unwrap()
    );
    std::fs::write(
        root.join("rows.json"),
        serde_json::to_vec_pretty(&rows).unwrap(),
    )
    .unwrap();
    crate::logging::flush();
}
