use motion_model::{composition::MAX_COMPOSITIONS, *};

fn project(total: usize) -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.layers.push(layer());
    project.compositions = (1..total)
        .map(|id| Composition {
            id: format!("comp-{id}"),
            name: format!("Library {id}"),
            width: 64,
            height: 64,
            fps: 30,
            frames: 120,
            background: [0.; 4],
            camera: Camera::new(64, 64),
            layers: if id == 1 { vec![layer()] } else { vec![] },
            expressions: vec![],
        })
        .collect();
    project.validate().unwrap();
    project
}

fn layer() -> Layer {
    Layer::solid(1, "Selected", [32.; 2], [32., 32., 0.], [1.; 4])
}

fn precompose(objects: Vec<u64>, range: &str) -> CompositionAction {
    CompositionAction::Precompose {
        objects,
        name: "Nested".into(),
        range: range.into(),
    }
}

fn composition_error(error: Error) -> composition::CompositionError {
    let Error::Composition(error) = error else {
        panic!("expected composition error: {error}")
    };
    error
}

#[test]
fn precompose_at_capacity_returns_resource_error_without_mutating_the_model() {
    for nested in [false, true] {
        let mut project = project(MAX_COMPOSITIONS);
        if nested {
            project.activate_composition("comp-1").unwrap();
        }
        let before = project.clone();
        let bytes = serde_json::to_vec(&project).unwrap();
        let error = composition_error(
            project
                .edit_composition(precompose(vec![1], "composition"))
                .unwrap_err(),
        );
        assert_eq!(error.code, "resource_limit");
        assert_eq!(error.composition, before.composition_id);
        assert!(project == before, "failed edit changed the model");
        assert_eq!(serde_json::to_vec(&project).unwrap(), bytes);
        project.validate().unwrap();
    }
}

#[test]
fn precompose_can_use_the_last_composition_slot_in_any_active_view() {
    for nested in [false, true] {
        let mut project = project(MAX_COMPOSITIONS - 1);
        if nested {
            project.activate_composition("comp-1").unwrap();
        }
        let selected = project.layers[0].clone();
        let result = project
            .edit_composition(precompose(vec![1], "composition"))
            .unwrap();
        assert_eq!(project.compositions.len() + 1, MAX_COMPOSITIONS);
        let child = project
            .composition(result["composition"].as_str().unwrap())
            .unwrap();
        assert_eq!(child.layers, [selected]);
        assert!(matches!(
            project.layers[0].content,
            Content::Composition { .. }
        ));
        project.validate().unwrap();
    }
}

#[test]
fn selection_and_range_errors_keep_their_precedence_at_capacity() {
    for (objects, range, code) in [
        (vec![], "composition", "invalid_selection"),
        (vec![999], "composition", "cross_composition_selection"),
        (vec![1], "selected", "unsupported_mode"),
    ] {
        let mut project = project(MAX_COMPOSITIONS);
        let before = project.clone();
        let error = composition_error(
            project
                .edit_composition(precompose(objects, range))
                .unwrap_err(),
        );
        assert_eq!(error.code, code);
        assert_eq!(project, before);
    }
}

#[test]
fn zero_audio_frame_rate_returns_a_contextual_error_even_without_audio_layers() {
    for nested in [false, true] {
        let mut project = project(2);
        if nested {
            project.activate_composition("comp-1").unwrap();
        }
        project.fps = 0;
        let before = serde_json::to_vec(&project).unwrap();
        let error = composition_error(project.audio_voices().unwrap_err());
        assert_eq!(error.code, "invalid_frame_rate");
        assert_eq!(error.composition, project.composition_id);
        assert_eq!(error.details["fps"], 0);
        assert!(error.message.contains("frame rate"));
        assert_eq!(serde_json::to_vec(&project).unwrap(), before);
    }
}
