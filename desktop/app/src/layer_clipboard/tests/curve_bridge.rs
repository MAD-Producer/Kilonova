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
            .start_curves(&engine, intent, KeyframeSource::of(&state), selection())
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
            selection()
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
                edit: Edit::PasteEasing {
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
