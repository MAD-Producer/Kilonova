use motion_core::{lens::Lens, Command, EditResult, Engine, Project, Property};

fn engine() -> (Engine, u64) {
    let mut engine = Engine::new(Project::new(1920, 1080, 30, 300).unwrap()).unwrap();
    let result = engine.apply_batch(vec![Command::CreateCamera]).unwrap();
    let [EditResult::CreateCamera { object }] = result.as_slice() else {
        panic!("{result:?}")
    };
    (engine, *object)
}
fn edit(object: u64, frame: u32, film: f32, focal: f32) -> Command {
    Command::CameraLens {
        object,
        frame,
        film_width_mm: film,
        focal_length_mm: focal,
    }
}

#[test]
fn pinhole_conversions_follow_vertical_fov_and_horizontal_gate_without_drift() {
    for (width, height) in [(1920, 1080), (1080, 1920), (640, 640)] {
        for film in [0.1, 24., 36., 70., 1000.] {
            for fov in [10., 45., 90., 120.] {
                let lens = Lens::from_fov(film, fov, width, height).unwrap();
                let mut focal = lens.focal_length_mm;
                for _ in 0..100 {
                    let converted = Lens::from_focal_length(film, focal, width, height).unwrap();
                    assert!((converted.vertical_fov_degrees - fov).abs() < 1e-10);
                    assert!((converted.zoom_px - lens.zoom_px).abs() < 1e-9);
                    focal = converted.focal_length_mm;
                }
                assert!((focal - lens.focal_length_mm).abs() < 1e-9);
            }
        }
    }
    let lens = Lens::from_fov(36., 90., 1920, 1080).unwrap();
    assert!((lens.zoom_px - 540.).abs() < 1e-9);
    assert!((lens.focal_length_mm - 10.125).abs() < 1e-9);
}

#[test]
fn lens_edit_uses_existing_fov_animation_and_one_undo_with_storage_round_trip() {
    let (mut engine, object) = engine();
    engine
        .apply(Command::Animate {
            object,
            property: Property::Fov,
            axis: None,
            frame: 0,
            enabled: true,
        })
        .unwrap();
    let before = engine.snapshot();
    engine.apply(edit(object, 30, 24., 50.)).unwrap();
    let after = engine.snapshot();
    assert_eq!(after.version, motion_model::CURRENT_PROJECT_VERSION);
    let camera = after.camera_for(object).unwrap();
    let expected = Lens::from_focal_length(24., 50., 1920, 1080).unwrap();
    assert_eq!(camera.film_width_mm, 24.);
    assert!((f64::from(camera.fov.sample(30.).unwrap()) - expected.vertical_fov_degrees).abs() < 1e-5);
    assert_eq!(camera.fov.keys.len(), 2);
    assert_eq!(
        camera.fov.sample(0.).unwrap(),
        before.camera_for(object).unwrap().fov.sample(0.).unwrap()
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    engine.redo().unwrap();
    assert_eq!(engine.project(), &after);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(motion_core::storage::load(directory.path()).unwrap(), after);
    let mut old = after.clone();
    old.version = 21;
    assert!(old.validate().is_err());
    old.version = motion_model::CURRENT_PROJECT_VERSION + 1;
    assert!(old.validate().is_err());
}

#[test]
fn failed_lens_edits_and_locked_targets_are_atomic() {
    let (mut engine, object) = engine();
    for command in [
        edit(object, 0, 0., 50.),
        edit(object, 0, f32::NAN, 50.),
        edit(object, 0, 36., 0.),
        edit(object, 0, 36., f32::INFINITY),
        edit(object, 0, 36., 0.1),
        edit(999, 0, 36., 50.),
        edit(0, 0, 36., 50.),
    ] {
        let before = engine.snapshot();
        let revision = engine.revision();
        assert!(engine.apply(command).is_err());
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.revision(), revision);
    }
    engine
        .apply(Command::Flags {
            object,
            visible: true,
            locked: true,
        })
        .unwrap();
    let before = engine.snapshot();
    assert!(engine.apply(edit(object, 0, 24., 50.)).is_err());
    assert_eq!(engine.project(), &before);
}

#[test]
fn default_gate_is_explicit_and_edits_keep_the_current_format() {
    let (mut engine, object) = engine();
    let before = engine.snapshot();
    let serialized = serde_json::to_value(&before).unwrap();
    assert_eq!(serialized["camera"]["film_width_mm"], 36.);
    let camera = &serialized["layers"][0]["content"]["camera"];
    assert_eq!(camera["film_width_mm"], 36.);
    engine.apply(edit(object, 0, 36., 50.)).unwrap();
    assert_eq!(engine.project().version, before.version);
    assert_eq!(
        serde_json::from_value::<Project>(serialized).unwrap(),
        before
    );
}

#[test]
fn numeric_f32_boundary_values_can_round_trip_through_the_command() {
    let (mut engine, object) = engine();
    for film in [0.1_f32, 24., 36., 1000.] {
        for fov in [10., 120.] {
            let lens = Lens::from_fov(f64::from(film), fov, 1920, 1080).unwrap();
            engine
                .apply(edit(object, 0, film, lens.focal_length_mm as f32))
                .unwrap();
            assert!(
                (f64::from(engine.project().camera_for(object).unwrap().fov.value) - fov).abs()
                    < 1e-4
            );
        }
    }
}

#[test]
fn lens_edit_uses_local_clip_time_and_legacy_camera_and_matches_render_projection() {
    let (mut engine, object) = engine();
    engine
        .apply(Command::Animate {
            object,
            property: Property::Fov,
            axis: None,
            frame: 0,
            enabled: true,
        })
        .unwrap();
    engine
        .apply(Command::MoveLayerClip {
            object,
            in_frame: 30,
        })
        .unwrap();
    engine.apply(edit(object, 60, 24., 50.)).unwrap();
    let camera = engine.project().camera_for(object).unwrap();
    assert_eq!(camera.fov.keys.last().unwrap().frame, 30);
    let mut scene = motion_render::Scene::new(engine.project()).unwrap();
    scene
        .sample(
            engine.project(),
            60.,
            None,
            &motion_core::ExpressionEvaluator,
        )
        .unwrap();
    let matrix = scene.camera.view_projection;
    let mut reference = engine.snapshot();
    reference.camera_for_mut(object).unwrap().film_width_mm = 36.;
    let mut reference_scene = motion_render::Scene::new(&reference).unwrap();
    reference_scene
        .sample(&reference, 60., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert_eq!(matrix, reference_scene.camera.view_projection);
    let mut legacy = Engine::new(Project::demo().unwrap()).unwrap();
    legacy.apply(edit(0, 0, 24., 50.)).unwrap();
    assert_eq!(legacy.project().camera.film_width_mm, 24.);
    assert_eq!(legacy.project().version, motion_model::CURRENT_PROJECT_VERSION);
}

#[test]
fn scoped_lens_edits_use_target_dimensions_and_survive_package_export() {
    let (mut engine, _) = engine();
    let result=engine.apply_batch(motion_core::parse_commands(r#"{"op":"composition","action":{"kind":"create","settings":{"name":"Portrait","width":1080,"height":1920,"fps":30,"frames":300}}}"#).unwrap()).unwrap();
    let EditResult::Composition { result } = &result[0] else {
        panic!("{result:?}")
    };
    let id = result["composition"].as_str().unwrap().to_owned();
    let result = engine
        .apply_batch(vec![Command::InComposition {
            composition: id.clone(),
            command: Box::new(Command::CreateCamera),
        }])
        .unwrap();
    let [EditResult::CreateCamera { object }] = result.as_slice() else {
        panic!("{result:?}")
    };
    let object = *object;
    engine
        .apply(Command::InComposition {
            composition: id.clone(),
            command: Box::new(edit(object, 0, 24., 50.)),
        })
        .unwrap();
    let project = engine.snapshot();
    let child = project.composition(&id).unwrap();
    let expected = Lens::from_focal_length(24., 50., 1080, 1920).unwrap();
    assert!(
        (f64::from(child.camera_for(object).unwrap().fov.value) - expected.vertical_fov_degrees)
            .abs()
            < 1e-5
    );
    assert_eq!(project.version, motion_model::CURRENT_PROJECT_VERSION);
    project.validate().unwrap();
    let root = tempfile::tempdir().unwrap();
    motion_core::storage::save(root.path(), &project).unwrap();
    let package = root.path().join("lens.msproj");
    motion_core::storage::export_package(root.path(), &project, &package).unwrap();
    assert_eq!(
        motion_core::storage::import_package(&package, &root.path().join("import")).unwrap(),
        project
    );
}
