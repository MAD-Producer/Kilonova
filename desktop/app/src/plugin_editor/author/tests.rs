use super::*;
pub(super) fn example() -> PluginProject {
    PluginProject::from_json(include_str!(
        "../../../../../crates/motion-graph/examples/parameter-workbench.msplugin"
    ))
    .unwrap()
}
#[test]
fn layout_transactions_are_atomic_and_preserve_author_source() {
    let original = example();
    let mut d = model::Draft::new(original.clone());
    let root = d.project.interface.root.id.clone();
    d.edit(|p| {
        model::insert(p, &root, 0, model::Component::Group)?;
        Ok(())
    })
    .unwrap();
    let group = d
        .project
        .interface
        .root
        .children()
        .next()
        .unwrap()
        .id
        .clone();
    let field = d
        .project
        .interface
        .root
        .children()
        .nth(1)
        .unwrap()
        .id
        .clone();
    d.edit(|p| model::move_node(p, &field, &group, 0)).unwrap();
    assert_eq!(
        model::find(&d.project.interface.root, &group)
            .unwrap()
            .children()
            .next()
            .unwrap()
            .id,
        field
    );
    let before = d.project.clone();
    assert!(d.edit(|p| model::move_node(p, &group, &field, 0)).is_err());
    assert_eq!(d.project, before);
    d.undo();
    d.undo();
    assert_eq!(d.project, original);
    d.redo();
    assert!(d.dirty());
    d.begin();
    d.update(|p| {
        p.interface.root.width =
            motion_effects::native_layout::LayoutExtent::Points { value: 400. };
        Ok(())
    })
    .unwrap();
    d.cancel();
    assert_ne!(
        d.project.interface.root.width,
        motion_effects::native_layout::LayoutExtent::Points { value: 400. }
    );
}
#[test]
fn authored_diagnostics_exclude_host_prelude_and_keep_utf8_presentation() {
    let mut p = example();
    if let PluginLogic::Wgsl { document } = &mut p.logic {
        document.source="fn plugin_fx(p: vec2<f32>) -> vec4<f32> {\n  let broken = ;\n  return vec4<f32>(1.0);\n}".into();
    }
    let error = p
        .compile_logic()
        .err()
        .expect("real compiler must reject syntax");
    assert_eq!(p.compile_error_location(&error).map(|p| p.0), Some(2));
    let s = motion_ui::style::default_stylesheet_arc();
    for text in ["fn 颜色(p: vec2<f32>) { // 注释", "let λ = 0.25;", ""] {
        let parts = source::highlight(text, &s);
        assert_eq!(parts.into_iter().map(|p| p.0).collect::<String>(), text);
    }
}

#[test]
fn source_and_layout_share_history_and_old_save_cannot_clear_new_edits() {
    let original = example();
    let mut draft = model::Draft::new(original.clone());
    draft
        .edit(|p| {
            if let PluginLogic::Wgsl { document } = &mut p.logic {
                document.source.push_str("\n// author change");
            }
            Ok(())
        })
        .unwrap();
    let source_only = draft.project.clone();
    let root = draft.project.interface.root.id.clone();
    draft
        .edit(|p| {
            model::insert(p, &root, 0, model::Component::Group)?;
            Ok(())
        })
        .unwrap();
    let combined = draft.project.clone();
    draft.saved(source_only.clone()); // A save already in flight finishes late.
    assert!(draft.dirty());
    draft.undo();
    assert_eq!(draft.project, source_only);
    assert!(!draft.dirty());
    draft.undo();
    assert_eq!(draft.project, original);
    assert!(draft.dirty());
    draft.redo();
    draft.redo();
    assert_eq!(draft.project, combined);
}

#[test]
fn worker_failure_channels_publish_before_blocking_and_preserve_drafts() {
    const CHILD: &str = "MOTION_PLUGIN_AUTHOR_FAILURE_CHILD";
    const TEST: &str = "plugin_editor::author::tests::worker_failure_channels_publish_before_blocking_and_preserve_drafts";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logs = crate::logging::init(root.join("logs")).unwrap();
        let w = worker::Worker::new().unwrap();
        let (release, gate) = std::sync::mpsc::channel();
        w.submit(worker::Job::Block(gate)).unwrap();
        let deadline = Instant::now();
        loop {
            let state = w.flight.lock().unwrap().clone();
            if let Some(state) = state {
                assert_eq!(state.phase, "controlled_block");
                break;
            }
            assert!(deadline.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(matches!(
            w.reply.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        crate::logging::flush();
        let logs = || {
            std::fs::read_dir(root.join("logs"))
                .unwrap()
                .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
                .collect::<String>()
        };
        assert!(logs().contains("phase=controlled_block in_flight=true"));
        release.send(()).unwrap();
        assert!(matches!(
            w.reply.recv_timeout(Duration::from_secs(5)).unwrap(),
            worker::Event::Cancelled
        ));
        assert!(w.flight.lock().unwrap().is_none());
        w.submit(worker::Job::Load(Some(root.join("missing.msplugin"))))
            .unwrap();
        assert!(matches!(
            w.reply.recv_timeout(Duration::from_secs(5)).unwrap(),
            worker::Event::Failed(worker::Fault { phase: "read", .. })
        ));
        let blocker = root.join("regular-file");
        std::fs::write(&blocker, b"not a directory").unwrap();
        let mut ws = Workspace::new(example(), blocker.join("draft.msplugin"), false).unwrap();
        ws.draft
            .edit(|p| {
                p.package.version = "0.3.0".into();
                Ok(())
            })
            .unwrap();
        ws.checked = ws.draft.revision;
        let before = ws.draft.project.clone();
        ws.save(false).unwrap();
        let began = Instant::now();
        while ws.busy {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(5));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(ws.draft.dirty());
        assert_eq!(ws.draft.project, before);
        assert_eq!(ws.message.as_ref().unwrap().phase, "write");
        // A real old compilation is delivered after a newer author revision.
        ws.message = None;
        ws.check().unwrap();
        ws.draft
            .edit(|p| {
                p.package.version = "0.4.0".into();
                Ok(())
            })
            .unwrap();
        let began = Instant::now();
        while ws.busy {
            ws.poll().unwrap();
            assert!(began.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(ws.take_ready().is_none());
        assert!(ws.message.is_none());
        ws.checked = ws.draft.revision;
        w.submit(worker::Job::Panic).unwrap();
        assert!(matches!(
            w.reply.recv_timeout(Duration::from_secs(5)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
        assert!(w.flight.lock().unwrap().is_none());
        crate::logging::flush();
        let file = logs();
        for marker in [
            "phase=read",
            "phase=write",
            "phase=resource_decode",
            "plugin.author aborted",
            "controlled plugin author handler panic",
        ] {
            assert!(file.contains(marker), "missing file diagnostic: {marker}");
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PLUGIN_AUTHOR_FAILURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
        .env(CHILD, &root)
        .env("MOTION_LOG", "info")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.txt"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.txt"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("plugin.author error") && stderr.contains("plugin.author aborted"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("plugin.author start")
            && !stdout.contains("plugin.author error")
            && !stdout.contains("motion_studio::")
    );
}
