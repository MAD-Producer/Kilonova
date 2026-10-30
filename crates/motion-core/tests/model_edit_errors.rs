use motion_core::{
    masks::{self, LayerMask, MaskAction},
    vector::{PathNode, VectorPath},
    Command, Engine, Error, Layer, Project, Track,
};
use serde_json::{json, Value};

fn mask(id: u64) -> LayerMask {
    LayerMask::new(
        id,
        VectorPath {
            tangents: Default::default(), id: 1,
            closed: true,
            nodes: [[0., 0.], [16., 0.], [16., 16.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    )
}

fn missing_actions() -> Vec<Value> {
    vec![
        json!({"kind":"replace","mask":mask(9)}),
        json!({"kind":"remove","mask":9}),
        json!({"kind":"options","mask":9,"enabled":false}),
        json!({"kind":"path","mask":9,"path":mask(9).path}),
        json!({"kind":"set","mask":9,"property":"opacity","frame":0,"value":[25]}),
        json!({"kind":"animate","mask":9,"property":"opacity","frame":0,"enabled":true}),
        json!({"kind":"delete_key","mask":9,"property":"opacity","frame":0}),
        json!({"kind":"move_key","mask":9,"property":"opacity","from":0,"to":10}),
        json!({"kind":"copy_key","mask":9,"property":"opacity","from":0,"to":10}),
        json!({"kind":"curve","mask":9,"property":"opacity","frame":0,"easing":{"ease":"linear"}}),
    ]
}

fn project() -> Project {
    let mut project = Project::new(32, 32, 30, 90).unwrap();
    let mut layer = Layer::solid(1, "Source", [32.; 2], [16., 16., 0.], [1.; 4]);
    layer.masks = vec![mask(1), mask(2)];
    project.layers.push(layer);
    project
}

#[test]
fn every_missing_mask_edit_returns_the_cause_without_touching_tracks_or_history() {
    let mut engine = Engine::new(project()).unwrap();
    // Keep existing keys and a redo branch: a failed batch must preserve both.
    let command = |action| {
        serde_json::from_value::<Command>(json!({"op":"mask","object":1,"action":action})).unwrap()
    };
    engine
        .apply(command(
            json!({"kind":"animate","mask":1,"property":"opacity","frame":0,"enabled":true}),
        ))
        .unwrap();
    engine
        .apply(command(
            json!({"kind":"set","mask":1,"property":"opacity","frame":10,"value":[50]}),
        ))
        .unwrap();
    let keyed = engine.snapshot();
    engine.undo().unwrap();
    let before = engine.snapshot();
    for action in missing_actions() {
        let mut masks = before.layers[0].masks.clone();
        let error = masks::edit(
            &mut masks,
            serde_json::from_value(action.clone()).unwrap(),
            0,
        )
        .unwrap_err();
        assert!(
            matches!(&error, Error::Invalid(message) if message == "mask does not exist: mask=9")
        );
        assert_eq!(
            masks, before.layers[0].masks,
            "direct edit changed tracks: {action}"
        );
        let error = engine
            .apply_batch(vec![
                command(json!({"kind":"options","mask":1,"inverted":true})),
                command(action.clone()),
            ])
            .unwrap_err();
        assert!(
            matches!(&error, Error::Invalid(message) if message == "mask does not exist: mask=9")
        );
        assert_eq!(
            engine.snapshot(),
            before,
            "failed batch changed project: {action}"
        );
    }
    engine.redo().unwrap();
    assert_eq!(engine.snapshot(), keyed, "errors destroyed the redo branch");
}

#[test]
fn mask_reorder_rejects_bad_membership_before_mutation_and_preserves_animation() {
    let mut masks = project().layers.remove(0).masks;
    masks[0].opacity.set_animated(0, true).unwrap();
    masks[0].opacity.set_at(10, 50.).unwrap();
    let before = masks.clone();
    for ids in [vec![], vec![1], vec![1, 1], vec![1, 9], vec![2, 1, 9]] {
        let error = masks::edit(&mut masks, MaskAction::Reorder { masks: ids }, 0).unwrap_err();
        assert!(
            matches!(error, Error::Invalid(message) if message == "mask order must include each existing ID once")
        );
        assert_eq!(masks, before);
    }
    masks::edit(&mut masks, MaskAction::Reorder { masks: vec![2, 1] }, 0).unwrap();
    assert_eq!(masks, vec![before[1].clone(), before[0].clone()]);
    masks::edit(&mut masks, MaskAction::Reorder { masks: vec![1, 2] }, 0).unwrap();
    assert_eq!(masks, before);
}
