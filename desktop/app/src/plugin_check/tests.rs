use super::*;
use std::{
    path::PathBuf,
    sync::{mpsc, Arc},
    time::Duration,
};

const SAMPLE: &str =
    include_str!("../../../../crates/motion-graph/examples/parameter-tint.msplugin");

#[test]
fn actual_source_checks_publish_before_blocking_and_preserve_default_error_channels() {
    const CHILD: &str = "MOTION_PLUGIN_CHECK_TEST_CHILD";
    const TEST: &str = "plugin_check::tests::actual_source_checks_publish_before_blocking_and_preserve_default_error_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let log = root.join("logs/motion-studio.log");
        let source = root.join("source.msplugin");
        let original = PluginProject::from_json(SAMPLE).unwrap();
        original.save(&source).unwrap();
        let checker = Arc::new(Checker::default());
        let report = checker.check(&source).unwrap();
        assert_eq!(
            (report.logic, report.parameters, report.layout_components),
            ("wgsl", 3, 4)
        );
        assert!(checker.current().is_none());

        let mut faults = Vec::new();
        let missing = checker.check(&root.join("missing.msplugin")).unwrap_err();
        assert!(std::error::Error::source(&missing)
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .is_some());
        faults.push(missing.to_string());
        std::fs::write(&source, "{\"format_version\":").unwrap();
        faults.push(checker.check(&source).unwrap_err().to_string());
        let mut dangling = original.clone();
        let motion_effects::native_layout::LayoutContent::Group { children, .. } =
            &mut dangling.interface.root.content
        else {
            unreachable!()
        };
        children[0].content = motion_effects::native_layout::LayoutContent::Parameter {
            parameter: "removed".into(),
            control: motion_effects::native_layout::ParameterControl::Auto,
        };
        dangling.save(&source).unwrap();
        let error = checker.check(&source).unwrap_err();
        assert_eq!(error.phase(), Phase::Bindings);
        assert!(error
            .to_string()
            .contains("node=enable-field field=parameter"));
        faults.push(error.to_string());
        let mut broken = original.clone();
        let PluginLogic::Wgsl { document } = &mut broken.logic else {
            unreachable!()
        };
        document.source = "fn plugin_fx(p: vec2<f32>) -> vec4<f32> {\n// PRIVATE_PLUGIN_AUTHORED_SOURCE\nreturn sample_source(p);".into();
        broken.save(&source).unwrap();
        let error = checker.check(&source).unwrap_err();
        assert_eq!(error.phase(), Phase::Compile);
        assert!(!error.to_string().contains("PRIVATE_PLUGIN_AUTHORED_SOURCE"));
        faults.push(error.to_string());
        let mut graph = motion_graph::demo_graph();
        graph.connections.clear();
        PluginProject::from_graph(graph, original.package.clone())
            .unwrap()
            .save(&source)
            .unwrap();
        let error = checker.check(&source).unwrap_err();
        assert!(error.to_string().contains("MissingInput"));
        faults.push(error.to_string());
        original.save(&source).unwrap();

        // Controlled providers pause at both native read and compiler entry.
        // They eventually call the real implementation, without altered budgets.
        let (ready_tx, ready_rx) = mpsc::channel();
        let (read_tx, read_rx) = mpsc::channel();
        let (compile_tx, compile_rx) = mpsc::channel();
        let worker = checker.clone();
        let file = source.clone();
        let compiler_ready = ready_tx.clone();
        let task = std::thread::spawn(move || {
            worker.check_with(
                &file,
                || {
                    ready_tx.send("read").unwrap();
                    read_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    PluginProject::load(&file)
                },
                |project| {
                    compiler_ready.send("compile").unwrap();
                    compile_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    project.compile_logic()
                },
            )
        });
        for phase in ["read", "compile"] {
            assert_eq!(
                ready_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
                phase
            );
            let flight = checker.current().expect("published before provider entry");
            assert_eq!(flight.phase, phase);
            let text = std::fs::read_to_string(&log).unwrap();
            assert!(text.lines().any(|line| line.contains(" INFO ")
                && line.contains(&format!("plugin.check start request={}", flight.request))
                && line.contains(&format!("phase={phase} in_flight=true"))));
            if phase == "read" {
                faults.push(checker.check(&source).unwrap_err().to_string());
                assert_eq!(
                    checker.current().unwrap().request,
                    flight.request,
                    "busy rejection preserves the active operation"
                );
                read_tx.send(()).unwrap();
            } else {
                compile_tx.send(()).unwrap();
            }
        }
        assert!(task.join().unwrap().unwrap().ok);
        assert!(checker.current().is_none());

        // Production propagates panics. The existing process hook records the
        // original panic/backtrace; scope cleanup releases only in-flight state.
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            checker.check_with(
                &source,
                || PluginProject::load(&source),
                |_| panic!("plugin compiler panic evidence"),
            )
        }))
        .is_err());
        assert!(checker.current().is_none());
        assert!(checker.check(&source).unwrap().ok);
        crate::logging::flush();
        let text = std::fs::read_to_string(&log).unwrap();
        for fault in &faults {
            assert!(text.contains(fault), "missing original fault: {fault}");
        }
        assert!(
            text.contains("plugin.check aborted")
                && text.contains("plugin compiler panic evidence")
        );
        assert!(!text.contains("PRIVATE_PLUGIN_AUTHORED_SOURCE"));
        assert!(
            !text.contains("plugin.check done"),
            "routine done remains Debug"
        );
        std::fs::write(
            root.join("results.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "ok":true, "faults":faults, "read_and_compile_published_before_entry":true,
                "panic_propagated_and_state_cleared":true, "recovery_compiled":true
            }))
            .unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ERROR motion_studio::plugin_check: plugin.check error"));
    assert!(stderr.contains("plugin compiler panic evidence"));
    assert!(!stderr.contains("PRIVATE_PLUGIN_AUTHORED_SOURCE"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("plugin.check start") && !stdout.contains("plugin.check error"));
    if let Some(evidence) = std::env::var_os("MOTION_PLUGIN_CHECK_EVIDENCE") {
        let evidence = PathBuf::from(evidence);
        std::fs::create_dir_all(&evidence).unwrap();
        for file in ["logs/motion-studio.log", "results.json"] {
            std::fs::copy(
                root.path().join(file),
                evidence.join(Path::new(file).file_name().unwrap()),
            )
            .unwrap();
        }
        std::fs::write(evidence.join("stderr.txt"), &output.stderr).unwrap();
        std::fs::write(evidence.join("stdout.txt"), &output.stdout).unwrap();
    }
}
