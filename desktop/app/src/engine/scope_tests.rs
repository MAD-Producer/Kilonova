use super::*;
use crate::node_editor::NodeEditor;
use std::time::Duration;

pub(super) fn exercise(engine: &Engine, directory: &std::path::Path) {
    let mut editor = NodeEditor::default();
    let snapshot = editor.snapshot();
    engine.node_expect(snapshot.preview_stamp()).unwrap();
    assert!(engine
        .node_install(snapshot.clone(), snapshot.compile().unwrap())
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap());
    let before = engine
        .node_capture(0., directory.join("before.png"))
        .unwrap();

    // The first render compiles a real built-in effect, interleaved with node
    // requests from another client. Both GPU scope paths run on the owner worker.
    engine
        .edit(
            serde_json::json!({"op":"add_shape","id":1,"name":"Main",
        "shape":"rectangle","size":[20,20],"position":[32,32,0]})
            .to_string(),
        )
        .unwrap();
    let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| {
            package["manifest"]["effects"]
                .as_array()
                .unwrap()
                .iter()
                .any(|effect| effect["id"] == "invert")
        })
        .unwrap();
    engine
        .plugin(
            serde_json::json!({"op":"add","object":1,"plugin":package["manifest"]["id"],
        "version":package["manifest"]["version"],"hash":package["hash"],"effect":"invert"})
            .to_string(),
        )
        .unwrap();
    std::thread::scope(|scope| {
        let frames = scope.spawn(|| {
            let mut presented = 0;
            for frame in 0..16 {
                // Hidden swapchains can temporarily return no drawable frame.
                // Validation errors must still be Err, never a skipped frame.
                presented += usize::from(engine.render(frame as f64).unwrap());
            }
            assert!(presented > 0);
        });
        let compiler = scope.spawn(|| {
            for _ in 0..8 {
                let mut invalid = snapshot.compile().unwrap();
                invalid.shader.wgsl = "this is not valid WGSL".into();
                let error = engine
                    .node_install(snapshot.clone(), invalid)
                    .unwrap()
                    .recv_timeout(Duration::from_secs(30))
                    .unwrap()
                    .unwrap_err();
                assert!(
                    error.contains(&format!("node graph {}", snapshot.document.id)),
                    "{error}"
                );
                assert!(error.contains("N1 compiled graph"), "{error}");
            }
        });
        frames.join().unwrap();
        compiler.join().unwrap();
    });
    let after = engine
        .node_capture(0., directory.join("after.png"))
        .unwrap();
    assert_eq!(before["fingerprint"], after["fingerprint"]);
    assert_eq!(
        engine.state().unwrap()["desktop"]["nodePreview"]["installs"],
        1
    );

    let mut stale = snapshot.clone();
    stale.preview_revision += 1;
    let mut malformed = snapshot.compile().unwrap();
    malformed.shader.wgsl = "invalid stale shader".into();
    // Stale work is rejected before allocating or validating GPU objects.
    assert!(!engine
        .node_install(stale, malformed)
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap());
    editor.set_parameter_default("color", 0, 0.2).unwrap();
    let latest = editor.snapshot();
    engine.node_expect(latest.preview_stamp()).unwrap();
    assert!(engine
        .node_install(latest.clone(), latest.compile().unwrap())
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap());
    let changed = engine
        .node_capture(0., directory.join("changed.png"))
        .unwrap();
    assert_ne!(before["fingerprint"], changed["fingerprint"]);
    assert_eq!(
        engine.state().unwrap()["desktop"]["nodePreview"]["installs"],
        2
    );
    engine.render(21.).unwrap();
}
