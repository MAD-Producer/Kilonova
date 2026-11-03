use motion_core::*;
#[path = "../../motion-model/tests/support/tracking_plane.rs"]
mod fixture;
#[test]
fn plane_binding_reaches_scene_sampling_and_undo_without_baking() {
    let p = fixture::plane_fixture();
    let binding = p.layers[0].tracking_plane.clone().unwrap();
    let mut initial = p.clone();
    initial.layers[0].tracking_plane = None;
    let mut engine = Engine::new(initial.clone()).unwrap();
    engine
        .apply(Command::SetTrackingPlane {
            object: 7,
            binding: Some(binding.clone()),
        })
        .unwrap();
    let bound = engine.project().clone();
    let mut scene = motion_render::Scene::new(&bound).unwrap();
    scene
        .sample(&bound, 0.5, None, &ExpressionEvaluator)
        .unwrap();
    let fx = &scene.effects[0];
    let i = fx.param_ids.iter().position(|id| id == "tl").unwrap();
    assert_eq!(&fx.values[i][..2], &[5., 2.5]);
    scene
        .sample(&bound, 2., None, &ExpressionEvaluator)
        .unwrap();
    assert_eq!(&scene.effects[0].values[i][..2], &[20., 10.]);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &initial);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &bound);
    let mut bad = binding;
    bad.effect = 1000;
    assert!(engine
        .apply(Command::SetTrackingPlane {
            object: 7,
            binding: Some(bad)
        })
        .is_err());
    assert_eq!(engine.project(), &bound);
    assert!(bound.layers[0].effects[0].params["tl"]
        .track
        .keys
        .is_empty());
}

#[test]
fn catalogue_describes_required_nullable_plane_binding_and_strict_quad_shape() {
    let v = CommandRegistry::catalogue();
    let s = &v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "set_tracking_plane")
        .unwrap()["inputSchema"];
    assert!(s["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "binding"));
    let b = s["properties"]["binding"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["type"] == "object")
        .unwrap();
    assert_eq!(b["additionalProperties"], false);
    assert_eq!(b["properties"]["reference_quad"]["minItems"], 4);
    assert!(b["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "effect"));
    assert_eq!(
        CommandRegistry::parse(r#"{"op":"set_tracking_plane","object":7,"binding":null}"#)
            .unwrap()
            .len(),
        1
    );
    assert!(CommandRegistry::parse(r#"{"op":"set_tracking_plane","object":7}"#).is_err());
}
