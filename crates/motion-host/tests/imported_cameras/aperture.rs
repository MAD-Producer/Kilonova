use super::*;
use motion_core::Track;

fn project(diameter: f32) -> Project {
    let mut p = Project::new(32, 32, 30, 10).unwrap();
    p.camera.created = true;
    p.camera.position.value = [16., 16., -100.];
    p.camera.target.value = [16., 16., 0.];
    p.camera.depth_of_field = serde_json::from_value(json!({
        "focus_distance": Track::constant(100.),
        "aperture_diameter": Track::constant(diameter),
        "max_radius_pixels": 8.,
    }))
    .unwrap();
    let mut card =
        motion_core::Layer::solid(1, "Owned card", [20.; 2], [16., 16., 0.], [1., 0., 0., 1.]);
    card.three_d = true;
    p.layers.push(card);
    p.validate().unwrap();
    p
}

#[test]
fn packed_aperture_refuses_before_writing_and_zero_diameter_recovers() {
    let mut builder = motion_render::effect_plan::PlanBuilder::new(
        motion_effects::Registry::new_with_builtins().unwrap(),
    )
    .unwrap();
    for diameter in [10., 0.] {
        let p = project(diameter);
        let mut scene = motion_render::Scene::new(&p).unwrap();
        scene
            .sample(&p, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let plan = builder.build(&scene, &[0], 32, 32, true).unwrap();
        let mut output = vec![0xa5; plan.buffer_bytes(&scene)];
        let before = output.clone();
        let result = plan.write(&scene, &mut output);
        if diameter > 0. {
            let cause = result
                .expect_err("portable plans cannot silently discard native aperture")
                .to_string();
            assert!(cause.contains("depth of field"), "{cause}");
            assert_eq!(output, before);
        } else {
            assert!(result.unwrap() > 0);
            assert_ne!(output, before);
        }
    }
}

#[test]
fn host_aperture_export_boundaries_refuse_and_disabled_aperture_recovers() {
    for diameter in [10., 0.] {
        let root = tempfile::tempdir().unwrap();
        let id = session::open(root.path().into(), project(diameter), Arc::new(CpuOnly)).unwrap();
        let mut parameters = vec![0xa5; 1 << 16];
        let mut vertices = vec![0x5a; 1 << 16];
        let a = parameters.clone();
        let b = vertices.clone();
        let mut bundle = Vec::new();
        let results = [
            motion_host::ops::export::render_plan_info(id).map(|_| ()),
            motion_host::ops::export::sample_render_plan_into(id, 0, &mut parameters).map(|_| ()),
            motion_host::ops::geometry::sample_geometry_into(
                id,
                0.,
                &mut parameters,
                &mut vertices,
            )
            .map(|_| ()),
            motion_host::ops::export::sample_into(id, 0, &mut parameters).map(|_| ()),
            motion_host::ops::composition::frame_bundle(
                id,
                motion_core::MAIN_COMPOSITION,
                0.,
                &mut bundle,
            )
            .map(|_| ()),
        ];
        session::close(id);
        for (index, result) in results.into_iter().enumerate() {
            if diameter > 0. {
                let cause = result
                    .expect_err("host cannot advertise a plane-only aperture export")
                    .to_string();
                assert!(cause.contains("depth of field"), "entry={index}: {cause}");
            } else {
                result.unwrap_or_else(|e| panic!("disabled entry={index}: {e}"));
            }
        }
        if diameter > 0. {
            assert_eq!(parameters, a);
            assert_eq!(vertices, b);
        }
    }
}

#[test]
fn nested_aperture_cannot_escape_through_a_pinhole_parent_bundle_or_geometry() {
    let mut authored = project(0.);
    authored.layers[0].three_d = false;
    let mut engine = motion_core::Engine::new(authored).unwrap();
    engine
        .apply_batch(
            motion_core::parse_commands(
                &json!({"op":"composition","composition":motion_core::MAIN_COMPOSITION,
        "action":{"kind":"precompose","objects":[1],"name":"Owned aperture child"}})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let mut p = engine.project().clone();
    p.compositions[0].camera = project(10.).camera;
    p.compositions[0].layers[0].three_d = true;
    p.validate().unwrap();
    let root = tempfile::tempdir().unwrap();
    let id = session::open(root.path().into(), p, Arc::new(CpuOnly)).unwrap();
    let mut bundle = Vec::new();
    let result = motion_host::ops::composition::frame_bundle(
        id,
        motion_core::MAIN_COMPOSITION,
        0.,
        &mut bundle,
    );
    let mut parameters = vec![0xa5; 1 << 16];
    let mut vertices = vec![0x5a; 1 << 16];
    let geometry =
        motion_host::ops::geometry::sample_geometry_into(id, 0., &mut parameters, &mut vertices);
    session::close(id);
    assert!(result.unwrap_err().to_string().contains("depth of field"));
    assert!(geometry.unwrap_err().to_string().contains("depth of field"));
    assert!(parameters.iter().all(|v| *v == 0xa5));
    assert!(vertices.iter().all(|v| *v == 0x5a));
}
