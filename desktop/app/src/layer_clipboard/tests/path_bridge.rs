use super::*;
use crate::engine::{Command as Request, Engine};
use crate::layer_clipboard::controller::{Clipboard, Completion, Domain, Intent, Status};
use motion_core::{
    vector::{FillRule, PathNode, VectorContent, VectorPath, VectorSource},
    PathDestination, PathSelection, PathTarget, Track,
};

fn document(root: &Path, nodes: u64, keys: u32) -> motion_core::Project {
    let mut p = motion_core::Project::new(64, 64, 30, 12000).unwrap();
    for id in [7, 9] {
        let mut layer =
            motion_core::Layer::solid(id, "Authored geometry", [32.; 2], [0.; 3], [1.; 4]);
        layer.content = motion_core::Content::Vector {
            vector: VectorContent {
                source: VectorSource::Paths {
                    paths: if id == 9 {
                        vec![]
                    } else {
                        vec![VectorPath {
                            tangents: Default::default(), id: 3,
                            closed: true,
                            nodes: (0..nodes)
                                .map(|i| PathNode {
                                    id: i + 10,
                                    geometry: Track {
                                        axes: None,
                                        value: [i as f32, 0., -1., 2., 3., -4.],
                                        keys: (0..keys)
                                            .map(|frame| motion_core::Keyframe {
                                                frame: frame as i32,
                                                value: [i as f32, frame as f32, -1., 2., 3., -4.],
                                                ease: motion_core::Ease::InOut,
                                                curve: None,
                                                spatial: None,
                                            })
                                            .collect(),
                                    },
                                })
                                .collect(),
                        }]
                    },
                },
                fill: Some(Track::constant([1.; 4])),
                fill_rule: FillRule::NonZero,
                stroke: None,
                trim: None,
            },
        };
        p.layers.push(layer);
    }
    motion_core::storage::save(root, &p).unwrap();
    p
}
fn selection() -> Vec<PathSelection> {
    vec![PathSelection {
        target: PathTarget::Vector {
            object: 7,
            item: None,
        },
        path: 3,
        nodes: None,
    }]
}
fn target() -> PathDestination {
    PathDestination::Vector {
        object: 9,
        item: None,
    }
}

#[test]
fn path_operations_reject_stale_documents_before_resolving_reused_ids_and_undo_once() {
    use crate::layer_clipboard::PathOperation;
    let root = tempfile::tempdir().unwrap();
    document(root.path(), 4, 2);
    let engine = super::engine(root.path());
    let before = engine.state().unwrap();
    for field in ["root", "composition", "document_generation", "revision"] {
        let mut stale = before.clone();
        stale[field] = if matches!(field, "root" | "composition") { "different".into() }
            else { (before[field].as_u64().unwrap() + 1).into() };
        let result = engine.call(|reply| Request::PathOperation {
            source: PathSource::of(&stale), operation: PathOperation::FirstVertex {
                target: selection()[0].target.clone(), path: 3, node: 12,
            }, reply,
        }).unwrap();
        assert_eq!(result["path_operation_status"], "superseded");
        assert_eq!(result["project"], before["project"]);
        assert_eq!(result["revision"], before["revision"]);
    }
    let result = engine.call(|reply| Request::PathOperation {
        source: PathSource::of(&before), operation: PathOperation::FirstVertex {
            target: selection()[0].target.clone(), path: 3, node: 12,
        }, reply,
    }).unwrap();
    assert_eq!(result["path_operation_status"], "applied");
    assert_eq!(result["project"]["layers"][0]["content"]["vector"]["source"]["paths"][0]["nodes"][0]["id"], 12);
    assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
}

#[test]
fn path_operation_failures_publish_start_and_default_file_stderr_without_stdout() {
    use crate::layer_clipboard::PathOperation;
    const NAME: &str = "layer_clipboard::tests::path_bridge::path_operation_failures_publish_start_and_default_file_stderr_without_stdout";
    const CHILD: &str = "MOTION_PATH_OPERATION_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        document(&root.join("project"), 4, 2);
        let engine = super::engine(&root.join("project"));
        let before = engine.state().unwrap();
        for operation in [
            PathOperation::FirstVertex { target: selection()[0].target.clone(), path: 3, node: 999 },
            PathOperation::CopyAs { target: selection()[0].target.clone(), path: 999 },
            PathOperation::Tangents { target: selection()[0].target.clone(), path: 999, mode: motion_core::vector::TangentMode::Automatic },
        ] {
            let error = engine.call(|reply| Request::PathOperation { source: PathSource::of(&before), operation, reply }).unwrap_err();
            assert!(error.contains("object=7") && error.contains("path="), "{error}");
            assert_eq!(engine.state().unwrap()["project"], before["project"]);
        }
        let restored = engine.call(|reply| Request::PathOperation { source: PathSource::of(&before),
            operation: PathOperation::CopyAs { target: selection()[0].target.clone(), path: 3 }, reply }).unwrap();
        assert_eq!(restored["path_operation_status"], "applied");
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        drop(engine);
        // Admitted author values can still overshoot at sampling time. Exercise
        // that real failure through Seek, rather than injecting a mocked Err.
        let sampling_root = root.join("sampling-project");
        let mut project = document(&sampling_root,4,0);
        let motion_core::Content::Vector{vector}=&mut project.layers[0].content else {panic!()};
        let VectorSource::Paths{paths}=&mut vector.source else {panic!()};
        paths[0].tangents=motion_core::vector::TangentMode::Automatic;
        paths[0].nodes[0].geometry.keys=[(0,-32768.),(10,32768.)].into_iter().map(|(frame,x)|motion_core::Keyframe {
            frame,value:[x,0.,0.,0.,0.,0.],ease:motion_core::Ease::Linear,
            curve:Some(motion_core::Curve{space:motion_core::CurveSpace::Progress,shape:motion_core::CurveShape::Cubic {
                control1:[0.25,8.],control2:[0.75,8.],start:0.,end:1.
            }}),spatial:None
        }).collect();
        motion_core::storage::save(&sampling_root,&project).unwrap();
        let engine=super::engine(&sampling_root);
        let before=engine.state().unwrap();
        let error=engine.seek(5.).unwrap_err();
        assert!(error.contains("path=3 frame=5")&&error.contains("invalid geometry node=0"),"{error}");
        let recovered=engine.seek(0.).unwrap();
        assert_eq!(recovered["project"],before["project"]);
        assert_eq!(recovered["revision"],before["revision"]);
        drop(engine);
        crate::logging::flush();
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PATH_OPERATION_ARTIFACTS").map(PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"]).env(CHILD, &root).env_remove("MOTION_LOG").output().unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(log.contains("logging initialized level=INFO"));
    assert!(log.lines().any(|l| l.contains(" INFO ") && l.contains("start #") && l.ends_with("path_operation")));
    for reason in ["node=Some(999)", "operation=copy_as object=7 path=999", "operation=set_tangents object=7 path=999", "path=3 frame=5"] {
        assert!(log.lines().any(|l| l.contains(" ERROR ") && l.contains(reason)), "{reason}\n{log}");
        assert!(stderr.contains(reason));
        assert!(!stdout.contains(reason));
    }
}
fn read(engine: &Engine, selections: Vec<PathSelection>) -> Result<PathRead, String> {
    engine.call(|reply| Request::PathClipboardRead {
        source: PathSource::of(&engine.state().unwrap()),
        selections,
        reply,
    })
}
fn finish(clipboard: &mut Clipboard, engine: &Engine) -> Result<Completion, String> {
    let began = Instant::now();
    loop {
        if let Some(result) = clipboard.poll(engine, true) {
            return result;
        }
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "stuck at {:?}",
            clipboard.phase()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn begin(clipboard: &mut Clipboard, engine: &Engine, intent: Intent) {
    assert!(clipboard
        .start_paths(
            engine,
            intent,
            PathSource::of(&engine.state().unwrap()),
            selection(),
            Some(target())
        )
        .unwrap());
}
fn ready(clipboard: &mut Clipboard, engine: &Engine) {
    let began = Instant::now();
    while clipboard.phase() != Some("ready") {
        assert!(clipboard.poll(engine, false).is_none());
        assert!(began.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn paths(state: &Value, index: usize) -> &Vec<Value> {
    state["project"]["layers"][index]["content"]["vector"]["source"]["paths"]
        .as_array()
        .unwrap()
}

#[test]
fn typed_geometry_above_json_limit_survives_source_shutdown_cross_project_save_and_one_undo() {
    let source = tempfile::tempdir().unwrap();
    document(source.path(), 128, 64);
    let engine = super::engine(source.path());
    let frozen = read(&engine, selection()).unwrap().clipboard.unwrap();
    let json = serde_json::to_string(&motion_core::Command::PastePaths {
        clipboard: Box::new((*frozen).clone()),
        target: target(),
        at_frame: Some(100),
    })
    .unwrap();
    assert!(json.len() > crate::engine::REQUEST_LIMIT);
    assert_eq!(
        engine.edit(json).unwrap_err(),
        "edit request exceeds 256 KiB"
    );
    drop(engine);
    drop(source);
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 3, 0);
    let engine = super::engine(root.path());
    let stamp = read(&engine, vec![]).unwrap().stamp;
    let result = engine
        .call(|reply| Request::ClipboardCommit {
            edit: Edit::PastePaths {
                stamp,
                clipboard: frozen,
                target: target(),
                at_frame: Some(100),
                cancel: Arc::new(AtomicBool::new(false)),
            },
            reply,
        })
        .unwrap();
    assert_eq!(result["clipboard_status"], "applied");
    let path = &paths(&result, 1)[0];
    assert_eq!(path["nodes"].as_array().unwrap().len(), 128);
    assert_eq!(path["nodes"][0]["geometry"]["keys"][0]["frame"], 100);
    assert_eq!(path["nodes"][0]["geometry"]["keys"][63]["frame"], 163);
    assert_eq!(
        engine.history(0).unwrap()["project"],
        serde_json::to_value(original).unwrap()
    );
    assert_eq!(engine.history(1).unwrap()["project"], result["project"]);
    engine.save().unwrap();
    assert_eq!(
        serde_json::to_value(motion_core::storage::load(root.path()).unwrap()).unwrap(),
        result["project"]
    );
}

#[test]
fn cut_freezes_before_delete_cancel_and_stale_commit_preserve_model_and_caches() {
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 4, 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    assert!(!clipboard
        .start_paths(
            &engine,
            Intent::Paste { in_place: true },
            PathSource::of(&engine.state().unwrap()),
            vec![],
            Some(target())
        )
        .unwrap());
    begin(&mut clipboard, &engine, Intent::Cut);
    ready(&mut clipboard, &engine);
    assert!(clipboard.paths_available());
    assert!(!clipboard.available() && !clipboard.keyframes_available());
    assert_eq!(paths(&engine.state().unwrap(), 0).len(), 1);
    clipboard.cancel();
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Cancelled
    );
    assert_eq!(
        engine.state().unwrap()["project"],
        serde_json::to_value(&original).unwrap()
    );
    begin(&mut clipboard, &engine, Intent::Cut);
    ready(&mut clipboard, &engine);
    let changed = engine
        .edit(
            serde_json::to_string(&motion_core::Command::SetVector {
                object: 7,
                property: motion_core::Property::Position,
                frame: 0,
                value: [5.; 3],
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Superseded
    );
    assert_eq!(engine.state().unwrap()["project"], changed["project"]);
    begin(&mut clipboard, &engine, Intent::Cut);
    let cut = finish(&mut clipboard, &engine).unwrap();
    assert_eq!(cut.domain, Domain::Paths);
    assert_eq!(cut.status, Status::Applied);
    assert!(paths(&cut.state.unwrap(), 0).is_empty());
    assert_eq!(engine.history(0).unwrap()["project"], changed["project"]);
    begin(&mut clipboard, &engine, Intent::Paste { in_place: true });
    let pasted = finish(&mut clipboard, &engine).unwrap().state.unwrap();
    assert_eq!(paths(&pasted, 1).len(), 1);
    begin(&mut clipboard, &engine, Intent::Paste { in_place: true });
    let twice = finish(&mut clipboard, &engine).unwrap().state.unwrap();
    assert_eq!(paths(&twice, 1).len(), 2);
    assert_ne!(paths(&twice, 1)[0]["id"], paths(&twice, 1)[1]["id"]);
    assert_eq!(engine.history(0).unwrap()["project"], pasted["project"]);
}

#[test]
fn pending_read_keeps_slot_until_reply_and_rejects_queued_stale_source() {
    let root = tempfile::tempdir().unwrap();
    document(root.path(), 4, 2);
    let engine = super::engine(root.path());
    let source = PathSource::of(&engine.state().unwrap());
    let (entered, blocked) = std::sync::mpsc::channel();
    let (release, proceed) = std::sync::mpsc::channel();
    let wait = engine
        .request(|reply| Request::TestWait {
            entered,
            release: proceed,
            reply,
        })
        .unwrap();
    blocked.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(engine.in_flight().unwrap().0, "test_wait");
    let mut clipboard = Clipboard::default();
    assert!(clipboard
        .start_paths(
            &engine,
            Intent::Cut,
            source.clone(),
            selection(),
            Some(target())
        )
        .unwrap());
    clipboard.cancel();
    assert!(clipboard.poll(&engine, true).is_none() && clipboard.busy());
    assert!(!clipboard
        .start_paths(
            &engine,
            Intent::Copy,
            source.clone(),
            selection(),
            Some(target())
        )
        .unwrap());
    release.send(()).unwrap();
    wait.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().status,
        Status::Cancelled
    );
    assert!(!clipboard.paths_available());
    engine
        .edit(
            serde_json::to_string(&motion_core::Command::SetVector {
                object: 7,
                property: motion_core::Property::Position,
                frame: 0,
                value: [5.; 3],
            })
            .unwrap(),
        )
        .unwrap();
    // Missing geometry is deliberately not resolved when its source is stale.
    let invalid = vec![PathSelection {
        target: PathTarget::Vector {
            object: 404,
            item: None,
        },
        path: 3,
        nodes: None,
    }];
    let stale = engine
        .call(|reply| Request::PathClipboardRead {
            source,
            selections: invalid,
            reply,
        })
        .unwrap();
    assert!(stale.superseded && stale.clipboard.is_none());
    assert_eq!(paths(&engine.state().unwrap(), 0).len(), 1);
}

#[test]
fn document_generation_and_missing_identity_reject_reads_and_commits_before_geometry_resolution() {
    let root = tempfile::tempdir().unwrap();
    let original = document(root.path(), 4, 2);
    let mut session = motion_host::session::Session::new(
        original.clone(),
        root.path().into(),
        motion_desktop_media::platform(),
    )
    .unwrap();
    let snapshot = session.snapshot().unwrap();
    for field in ["root", "composition", "document_generation", "revision"] {
        let mut incomplete = snapshot.clone();
        incomplete[field] = Value::Null;
        if field == "composition" {
            incomplete["project"]["composition_id"] = Value::Null;
        }
        let rejected =
            super::super::paths::read_paths(&session, PathSource::of(&incomplete), selection())
                .unwrap();
        assert!(
            rejected.superseded && rejected.clipboard.is_none(),
            "{field}"
        );
    }
    let source = PathSource::of(&snapshot);
    let frozen = super::super::paths::read_paths(&session, source.clone(), selection()).unwrap();
    session
        .replace_project(
            motion_core::Engine::new(original.clone()).unwrap(),
            root.path().into(),
        )
        .unwrap();
    assert_eq!(frozen.stamp.revision, session.engine.revision());
    assert_ne!(frozen.stamp.generation, session.document_generation);
    assert!(
        super::super::paths::read_paths(&session, source, selection())
            .unwrap()
            .superseded
    );
    let clipboard = frozen.clipboard.unwrap();
    for edit in [
        Edit::CutPaths {
            stamp: frozen.stamp.clone(),
            clipboard: clipboard.clone(),
            cancel: Arc::new(AtomicBool::new(false)),
        },
        Edit::PastePaths {
            stamp: frozen.stamp,
            clipboard,
            target: target(),
            at_frame: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
    ] {
        assert_eq!(
            commit(&mut session, edit).unwrap()["clipboard_status"],
            "superseded"
        );
        assert_eq!(*session.engine.project(), original);
        assert_eq!(session.engine.revision(), 0);
    }
}

#[test]
fn independent_path_keyframe_and_layer_caches_survive_a_rejected_capture() {
    let root = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    document(root.path(), 4, 2);
    let engine = super::engine(root.path());
    let mut clipboard = Clipboard::default();
    begin(&mut clipboard, &engine, Intent::Copy);
    let keys = vec![motion_core::KeyframeSelection {
        target: motion_core::KeyframeTarget::Vector {
            object: 7,
            item: None,
            parameter: "node:3:10".into(),
        },
        frames: vec![0, 1],
    }];
    assert!(!clipboard
        .start_keyframes(
            &engine,
            Intent::Copy,
            KeyframeSource::of(&engine.state().unwrap()),
            keys.clone(),
            vec![]
        )
        .unwrap());
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().domain,
        Domain::Paths
    );
    assert!(clipboard
        .start_keyframes(
            &engine,
            Intent::Copy,
            KeyframeSource::of(&engine.state().unwrap()),
            keys,
            vec![]
        )
        .unwrap());
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().domain,
        Domain::Keyframes
    );
    assert!(clipboard
        .start(&engine, Intent::Copy, vec![7], cache.path().into())
        .unwrap());
    assert_eq!(
        finish(&mut clipboard, &engine).unwrap().domain,
        Domain::Layers
    );
    let bad = vec![PathSelection {
        target: PathTarget::Vector {
            object: 404,
            item: None,
        },
        path: 3,
        nodes: None,
    }];
    assert!(clipboard
        .start_paths(
            &engine,
            Intent::Copy,
            PathSource::of(&engine.state().unwrap()),
            bad,
            Some(target())
        )
        .unwrap());
    assert!(finish(&mut clipboard, &engine)
        .err()
        .unwrap()
        .contains("phase=capture object=404"));
    assert!(
        clipboard.paths_available() && clipboard.keyframes_available() && clipboard.available()
    );
    begin(&mut clipboard, &engine, Intent::Paste { in_place: true });
    let pasted = finish(&mut clipboard, &engine).unwrap().state.unwrap();
    assert_eq!(paths(&pasted, 1)[0]["nodes"].as_array().unwrap().len(), 4);
    assert!(clipboard
        .start(
            &engine,
            Intent::Paste { in_place: true },
            vec![],
            cache.path().into()
        )
        .unwrap());
    let layers = finish(&mut clipboard, &engine).unwrap().state.unwrap();
    assert_eq!(layers["project"]["layers"].as_array().unwrap().len(), 3);
    assert!(clipboard.keyframes_available() && clipboard.paths_available());
}

#[test]
fn capture_and_commit_failures_reach_default_file_and_stderr_without_stdout_leaks() {
    const NAME: &str = "layer_clipboard::tests::path_bridge::capture_and_commit_failures_reach_default_file_and_stderr_without_stdout_leaks";
    const CHILD: &str = "MOTION_PATH_BRIDGE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        document(&root.join("project"), 4, 2);
        let engine = super::engine(&root.join("project"));
        let mut stale = engine.state().unwrap();
        stale["revision"] = (stale["revision"].as_u64().unwrap() + 1).into();
        let superseded = engine
            .call(|reply| Request::PathClipboardRead {
                source: PathSource::of(&stale),
                selections: selection(),
                reply,
            })
            .unwrap();
        assert!(superseded.superseded && superseded.clipboard.is_none());
        let mut cancelled = Clipboard::default();
        begin(&mut cancelled, &engine, Intent::Cut);
        ready(&mut cancelled, &engine);
        cancelled.cancel();
        assert_eq!(
            finish(&mut cancelled, &engine).unwrap().status,
            Status::Cancelled
        );
        crate::logging::flush();
        let normal = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
        assert!(!normal.lines().any(|l| l.contains(" ERROR ")), "{normal}");
        let bad = vec![PathSelection {
            target: PathTarget::Vector {
                object: 404,
                item: None,
            },
            path: 3,
            nodes: None,
        }];
        let error = match read(&engine, bad) {
            Err(e) => e,
            Ok(_) => panic!("missing path source accepted"),
        };
        assert!(error.contains("phase=capture object=404"), "{error}");
        let read = read(&engine, selection()).unwrap();
        let before = engine.state().unwrap();
        let error = engine
            .call(|reply| Request::ClipboardCommit {
                edit: Edit::PastePaths {
                    stamp: read.stamp,
                    clipboard: read.clipboard.unwrap(),
                    target: PathDestination::Vector {
                        object: 405,
                        item: None,
                    },
                    at_frame: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
                reply,
            })
            .unwrap_err();
        assert!(error.contains("phase=paste object=405"), "{error}");
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
        let source = PathSource::of(&engine.state().unwrap());
        let (entered, blocked) = std::sync::mpsc::channel();
        let (release, proceed) = std::sync::mpsc::channel();
        let wait = engine
            .request(|reply| Request::TestWait {
                entered,
                release: proceed,
                reply,
            })
            .unwrap();
        blocked.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(engine.in_flight().unwrap().0, "test_wait");
        let panic = engine
            .request(|reply| Request::TestPanic {
                during_prepare: false,
                reply,
            })
            .unwrap();
        let queued = engine
            .request(|reply| Request::PathClipboardRead {
                source,
                selections: selection(),
                reply,
            })
            .unwrap();
        release.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(20)).unwrap().unwrap();
        let primary = panic
            .recv_timeout(Duration::from_secs(20))
            .unwrap()
            .unwrap_err();
        assert!(primary.contains("injected worker panic"), "{primary}");
        let began = Instant::now();
        let secondary = loop {
            if let Some(result) = engine.poll_reply(&queued, "path_clipboard_read") {
                break match result {
                    Err(error) => error,
                    Ok(_) => panic!("path read after terminal worker panic succeeded"),
                };
            }
            assert!(began.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(
            secondary,
            format!("path_clipboard_read response disconnected: {primary}")
        );
        drop(engine);
        crate::logging::flush();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_PATH_BRIDGE_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(log.contains("logging initialized level=INFO"));
    for op in ["path_clipboard_read", "clipboard_commit"] {
        assert!(
            log.lines()
                .any(|l| l.contains("INFO") && l.contains("start #") && l.ends_with(op)),
            "{op}"
        );
    }
    assert!(!stderr.contains("start #"));
    for cause in [
        "phase=capture object=404",
        "phase=paste object=405",
        "injected worker panic",
    ] {
        assert!(log.contains(cause) && stderr.contains(cause), "{cause}");
        assert!(!stdout.contains(cause));
    }
}
