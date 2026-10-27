use motion_model::{Camera, Composition, CompositionClip, Content, Error, Layer, Project};
use serde_json::{json, Value};

fn reference(id: u64, target: &str, size: [f32; 2]) -> Layer {
    let mut layer = Layer::solid(id, "Reference", size, [0.; 3], [1.; 4]);
    layer.content = Content::Composition {
        clip: CompositionClip::new(target.into()),
    };
    layer
}

fn node(
    id: &str,
    name: &str,
    size: [u32; 2],
    fps: u32,
    frames: u32,
    layers: Vec<Layer>,
) -> Composition {
    Composition {
        id: id.into(),
        name: name.into(),
        width: size[0],
        height: size[1],
        fps,
        frames,
        work_area: motion_model::WorkArea::full(frames),
        guides: Vec::new(),
        background: [0.; 4],
        camera: Camera::new(size[0], size[1]),
        layers,
        expressions: vec![],
    }
}

fn project() -> Project {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.name = "Root".into();
    project.layers = vec![
        reference(1, "comp-a", [128., 72.]),
        reference(2, "comp-a", [128., 72.]),
        Layer::solid(3, "Solid", [32.; 2], [0.; 3], [1.; 4]),
    ];
    project.compositions = vec![
        node("comp-b", "Empty", [16; 2], 60, 240, vec![]),
        node(
            "comp-a",
            "Sequence",
            [128, 72],
            24,
            100,
            vec![reference(9, "comp-b", [16.; 2])],
        ),
        node(
            "comp-c",
            "Unused",
            [32; 2],
            30,
            60,
            vec![Layer::solid(3, "Other", [32.; 2], [0.; 3], [1.; 4])],
        ),
    ];
    project.validate().unwrap();
    project
}

fn expected() -> Vec<Value> {
    vec![
        json!({"id":"comp-main","name":"Root","width":64,"height":64,"fps":30,"frames":120,
            "main":true,"references":[],"children":[{"object":1,"composition":"comp-a"},{"object":2,"composition":"comp-a"}],"next_layer_id":4}),
        json!({"id":"comp-b","name":"Empty","width":16,"height":16,"fps":60,"frames":240,
            "main":false,"references":[{"composition":"comp-a","object":9}],"children":[],"next_layer_id":1}),
        json!({"id":"comp-a","name":"Sequence","width":128,"height":72,"fps":24,"frames":100,
            "main":false,"references":[{"composition":"comp-main","object":1},{"composition":"comp-main","object":2}],
            "children":[{"object":9,"composition":"comp-b"}],"next_layer_id":10}),
        json!({"id":"comp-c","name":"Unused","width":32,"height":32,"fps":30,"frames":60,
            "main":false,"references":[],"children":[],"next_layer_id":4}),
    ]
}

#[test]
fn query_wire_values_keep_empty_unused_nested_and_repeated_references_after_activation() {
    let mut project = project();
    let list = expected();
    assert_eq!(project.composition_list(), list);
    for entry in &list {
        assert_eq!(
            project.composition_references(entry["id"].as_str().unwrap()),
            entry["references"].as_array().unwrap().clone()
        );
    }
    assert!(project.composition_references("unknown").is_empty());
    let before = serde_json::to_vec(&project).unwrap();
    for _ in 0..60 {
        assert_eq!(project.composition_list(), list);
    }
    assert_eq!(serde_json::to_vec(&project).unwrap(), before);

    project.activate_composition("comp-a").unwrap();
    project.validate().unwrap();
    assert_eq!(
        project.composition_list(),
        [
            list[2].clone(),
            list[1].clone(),
            list[0].clone(),
            list[3].clone()
        ]
    );
    for entry in &list {
        assert_eq!(
            project.composition_references(entry["id"].as_str().unwrap()),
            entry["references"].as_array().unwrap().clone()
        );
    }
    project.activate_composition("comp-main").unwrap();
    assert_eq!(serde_json::to_vec(&project).unwrap(), before);
}

#[test]
fn lookup_keeps_borrowed_body_identity_and_native_missing_composition_error() {
    let mut project = project();
    for active in ["comp-main", "comp-a", "comp-c"] {
        project.activate_composition(active).unwrap();
        let before = project.clone();
        assert!(std::ptr::eq(
            project.composition_view(active).unwrap().layers,
            project.layers.as_slice()
        ));
        for body in &project.compositions {
            let view = project.composition_view(&body.id).unwrap();
            assert_eq!(view.name, body.name);
            assert_eq!(
                [view.width, view.height, view.fps, view.frames],
                [body.width, body.height, body.fps, body.frames]
            );
            assert!(std::ptr::eq(view.layers, body.layers.as_slice()));
        }
        let Error::Composition(error) = project.composition_view("missing").err().unwrap() else {
            panic!("missing node lost native composition identity")
        };
        assert_eq!(error.code, "composition_missing");
        assert_eq!(error.composition, "missing");
        assert_eq!(project, before);
    }
}
