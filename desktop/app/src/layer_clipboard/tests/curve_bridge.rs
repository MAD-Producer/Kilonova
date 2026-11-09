use super::*;
use crate::engine::Command as Request;
use crate::layer_clipboard::controller::{Clipboard, Domain, Intent, Status};
use motion_core::{CurveClipboard, Ease, Easing, KeyframeSelection, KeyframeTarget, Property};

fn selection() -> Vec<KeyframeSelection> {
    vec![KeyframeSelection {
        target: KeyframeTarget::Property {
            object: 7,
            property: Property::Opacity,
            axis: None,
        },
        frames: vec![0],
    }]
}
fn setup(root: &Path) -> (crate::engine::Engine, serde_json::Value) {
    let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
    let mut layer = motion_core::Layer::solid(7, "Curve destination", [32.; 2], [0.; 3], [1.; 4]);
    layer
        .transform
        .opacity
        .upsert(0, 0.2, Ease::Linear)
        .unwrap();
    layer
        .transform
        .opacity
        .upsert(30, 0.8, Ease::Linear)
        .unwrap();
    project.layers.push(layer);
    motion_core::storage::save(root, &project).unwrap();
    let engine = engine(root);
    let state = engine.state().unwrap();
    (engine, state)
}
fn finish(clipboard: &mut Clipboard, engine: &crate::engine::Engine) -> controller::Completion {
    let began = Instant::now();
    loop {
        if let Some(result) = clipboard.poll(engine, true) {
            return result.unwrap();
        }
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "curve clipboard stuck at {:?}",
            clipboard.phase()
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn curve_clipboard_cancel_and_superseded_source_never_open_the_system_clipboard() {
    let root = tempfile::tempdir().unwrap();
    let (engine, state) = setup(root.path());
    for intent in [Intent::Copy, Intent::Paste { in_place: true }] {
        let mut clipboard = Clipboard::default();
        assert!(clipboard
            .start_curves(&engine, intent, KeyframeSource::of(&state), selection(), vec![])
            .unwrap());
        clipboard.cancel();
        let result = finish(&mut clipboard, &engine);
        assert_eq!(result.domain, Domain::Curves);
        assert_eq!(result.status, Status::Cancelled);
        assert_eq!(engine.state().unwrap()["revision"], state["revision"]);
    }
    let mut stale = state.clone();
    stale["document_generation"] = 999999.into();
    let mut clipboard = Clipboard::default();
    assert!(clipboard
        .start_curves(
            &engine,
            Intent::Copy,
            KeyframeSource::of(&stale),
            selection(), vec![]
        )
        .unwrap());
    assert_eq!(finish(&mut clipboard, &engine).status, Status::Superseded);
    assert_eq!(engine.state().unwrap()["revision"], state["revision"]);
}

#[test]
fn curve_clipboard_engine_capture_and_commit_keep_stamp_cancellation_and_single_undo_contract() {
    let root = tempfile::tempdir().unwrap();
    let (engine, state) = setup(root.path());
    let read = || {
        let source = KeyframeSource::of(&engine.state().unwrap());
        engine
            .call(|reply| Request::CurveClipboardRead {
                source,
                selection: Some(selection()[0].clone()),
                local_selection: None,
                reply,
            })
            .unwrap()
    };
    let captured = read();
    assert!(!captured.superseded);
    assert_eq!(captured.clipboard.unwrap().easing.ease, Ease::Linear);
    let old_stamp = captured.stamp;
    let payload = CurveClipboard::new(Easing {
        ease: Ease::Out,
        curve: None,
    })
    .unwrap();
    let paste = |stamp, cancel| {
        engine
            .call(|reply| Request::ClipboardCommit {
                edit: Edit::PasteEasing { local_selections: vec![],
                    stamp,
                    cancel,
                    clipboard: payload.clone(),
                    selections: selection(),
                },
                reply,
            })
            .unwrap()
    };
    let cancelled = paste(old_stamp.clone(), Arc::new(AtomicBool::new(true)));
    assert_eq!(cancelled["clipboard_status"], "superseded");
    assert_eq!(cancelled["revision"], serde_json::Value::Null);
    let applied = paste(old_stamp.clone(), Arc::new(AtomicBool::new(false)));
    assert_eq!(applied["clipboard_status"], "applied");
    assert_eq!(
        applied["project"]["layers"][0]["transform"]["opacity"]["keys"][0]["ease"],
        "out"
    );
    let after = engine.state().unwrap();
    assert_eq!(
        paste(old_stamp, Arc::new(AtomicBool::new(false)))["clipboard_status"],
        "superseded"
    );
    assert_eq!(engine.state().unwrap()["revision"], after["revision"]);
    let undo = engine.history(0).unwrap();
    assert_eq!(undo["project"], state["project"]);
    assert_eq!(undo["canUndo"], false);
}

#[test]
fn local_curve_worker_keeps_fractional_source_stamp_atomic_rejection_and_one_undo() {
    use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};
    for reverse in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let (original, state) = setup(root.path());
        original.request_shutdown().unwrap();
        let mut project:motion_core::Project=serde_json::from_value(state["project"].clone()).unwrap();
        let layer=&mut project.layers[0];
        layer.time_mapping=Some(LayerTimeMapping {
            clock:LayerClock::new(TimeStretch::new(3,2).unwrap(),
                if reverse {TimeDirection::Reverse} else {TimeDirection::Forward},0.,if reverse {33.} else {0.}).unwrap(),remap:None,
        });
        layer.transform.opacity.keys[0].frame=-3;
        layer.transform.opacity.keys[1].frame=3;
        let mut end=layer.transform.opacity.keys[1].clone();end.frame=9;layer.transform.opacity.keys.push(end);
        motion_core::storage::save(root.path(),&project).unwrap();
        let engine=engine(root.path());let before=engine.state().unwrap();
        let local=motion_core::LocalKeyframeSelection {target:selection()[0].target.clone(),frames:vec![3]};
        let read=engine.call(|reply|Request::CurveClipboardRead {
            source:KeyframeSource::of(&before),selection:None,local_selection:Some(local.clone()),reply,
        }).unwrap();
        assert_eq!(read.clipboard.unwrap().easing.ease,Ease::Linear);
        let paste=|stamp,cancel,local_selections|engine.call(|reply|Request::ClipboardCommit {
            edit:Edit::PasteEasing {stamp,cancel,clipboard:CurveClipboard::new(Easing {ease:Ease::Out,curve:None}).unwrap(),selections:vec![],local_selections},reply,
        });
        let cancelled=paste(read.stamp.clone(),Arc::new(AtomicBool::new(true)),vec![local.clone()]).unwrap();
        assert_eq!(cancelled["clipboard_status"],"superseded");
        let mut missing=local.clone();missing.frames=vec![999];
        let cause=paste(read.stamp.clone(),Arc::new(AtomicBool::new(false)),vec![missing]).unwrap_err();
        assert!(cause.to_string().contains("does not exist"),"{cause}");
        assert_eq!(engine.state().unwrap()["project"],before["project"]);
        let after=paste(read.stamp.clone(),Arc::new(AtomicBool::new(false)),vec![local.clone()]).unwrap();
        assert_eq!(after["project"]["layers"][0]["transform"]["opacity"]["keys"][1]["ease"],"out");
        assert_eq!(after["project"]["layers"][0]["transform"]["opacity"]["keys"][1]["frame"],3);
        assert_eq!(paste(read.stamp,Arc::new(AtomicBool::new(false)),vec![local]).unwrap()["clipboard_status"],"superseded");
        assert_eq!(engine.history(0).unwrap()["project"],before["project"]);
        assert!(!engine.state().unwrap()["canUndo"].as_bool().unwrap());
        assert_eq!(engine.history(1).unwrap()["project"],after["project"]);
    }
}
