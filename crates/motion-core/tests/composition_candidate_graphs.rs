use motion_core::{
    Camera, Command, Composition, CompositionAction, CompositionClip, Content, Engine, Layer,
    Project, MAIN_COMPOSITION,
};

fn reference(id: u64, target: &str) -> Layer {
    let mut layer = Layer::solid(id, "Reference", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Composition {
        clip: CompositionClip::new(target.into()),
    };
    layer
}

fn node(index: usize, targets: &[usize]) -> Composition {
    Composition {
        id: format!("comp-{index}"),
        name: format!("Composition {index}"),
        width: 64,
        height: 64,
        fps: 30,
        frames: 120,
        background: [0.; 4],
        camera: Camera::new(64, 64),
        layers: targets
            .iter()
            .enumerate()
            .map(|(id, target)| reference(id as u64 + 1, &format!("comp-{target}")))
            .collect(),
        expressions: vec![],
    }
}

fn assert_candidates_match_edits(project: &Project) {
    project.validate().unwrap();
    for source in project.composition_ids() {
        let candidates = project.composition_reference_candidates(&source).unwrap();
        for target in project.composition_ids() {
            let mut engine = Engine::new(project.clone()).unwrap();
            let before = engine.snapshot();
            let revision = engine.revision();
            let result = engine.apply(Command::InComposition {
                composition: source.clone(),
                command: Box::new(Command::Composition {
                    action: CompositionAction::Reference {
                        target: target.clone(),
                        at_frame: 0,
                    },
                }),
            });
            assert_eq!(
                candidates.contains(&target),
                result.is_ok(),
                "{source} -> {target}: {result:?}"
            );
            if result.is_err() {
                assert_eq!(engine.project(), &before);
                assert_eq!(engine.revision(), revision);
                assert!(!engine.can_undo());
            }
        }
    }
}

#[test]
fn shared_diamonds_and_repeated_references_do_not_depend_on_library_visit_order() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers = vec![reference(1, "comp-0"), reference(2, "comp-1")];
    project.compositions = vec![
        node(0, &[2, 2]),
        node(1, &[2, 3]),
        node(2, &[4]),
        node(3, &[]),
        node(4, &[]),
    ];
    assert!(project
        .composition_reference_candidates("comp-4")
        .unwrap()
        .contains(&"comp-3".into()));
    assert!(!project
        .composition_reference_candidates("comp-4")
        .unwrap()
        .contains(&"comp-0".into()));
    assert_candidates_match_edits(&project);
    project.compositions.reverse();
    assert_candidates_match_edits(&project);
}

#[test]
fn shared_source_expansion_counts_each_instance_and_preserves_the_last_slot() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers = vec![reference(1, "comp-0"), reference(2, "comp-0")];
    project.compositions = (0..8)
        .map(|index| node(index, &[index + 1, index + 1]))
        .collect();
    project.compositions.extend([node(8, &[]), node(9, &[])]);
    // Root expansion is 1+2+4+...+512 = 1023. One root leaf fits;
    // adding it to the shared bottom source would instantiate 512 new leaves.
    for reverse in [false, true] {
        if reverse {
            project.compositions.reverse();
        }
        project.validate().unwrap();
        assert!(project
            .composition_reference_candidates(MAIN_COMPOSITION)
            .unwrap()
            .contains(&"comp-9".into()));
        assert!(!project
            .composition_reference_candidates("comp-8")
            .unwrap()
            .contains(&"comp-9".into()));
        assert_candidates_match_edits(&project);
    }
}
