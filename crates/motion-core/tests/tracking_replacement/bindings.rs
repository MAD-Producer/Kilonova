use super::*;
#[path = "../../../motion-model/tests/support/tracking_plane.rs"]
mod plane_fixture;

fn nested_points_and_planes() -> Project {
    let mut project = plane_fixture::plane_fixture();
    let mut data = (*project.tracking_assets[0].data).clone();
    let mut point = data.tracks[0].clone();
    point.id = "point".into();
    point.kind = tracking::asset::Kind::Point;
    point.reference_index = None;
    for (i, sample) in point.samples.iter_mut().enumerate() {
        sample.observation = Some(Observation::Point([20. + i as f64 * 10., 30.]));
    }
    data.tracks.push(point);
    project.tracking_assets[0].data = SharedData::new(data).unwrap();
    let mut layer = Layer::solid(8, "Point correction", [1.; 2], [100., 80., 0.], [0.; 4]);
    layer.content = Content::Null;
    layer.tracking_point = Some(tracking::point::PointBinding {
        asset: 2,
        track: "point".into(),
        anchor_local_frame: 0,
        source_anchor_pts: -4000,
        speed_numerator: 1,
        speed_denominator: 1,
        reference_pixels: [20., 30.],
        crop: [0., 0., 320., 240.],
        rotation: tracking::plate::Rotation::None,
        interpolation: tracking::point::Interpolation::Linear,
        max_gap_ticks: 4000,
        minimum_confidence: 0.8,
        missing: tracking::point::Missing::Error,
    });
    project.layers.push(layer);
    project.compositions.push(Composition {
        id: "comp-nested".into(),
        name: "Independent nested corrections".into(),
        width: project.width,
        height: project.height,
        fps: project.fps,
        frames: project.frames,
        work_area: project.work_area,
        guides: Vec::new(),
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: Vec::new(),
    });
    project
        .edit_composition(CompositionAction::Reference {
            target: "comp-nested".into(),
            at_frame: 0,
        })
        .unwrap();
    project.activate_composition("comp-nested").unwrap();
    project.layers[1].transform.position.value[0] = 200.;
    project.validate().unwrap();
    project
}

fn next(project: &Project, detach: bool) -> Replacement {
    let mut video = project.video_assets[0].clone();
    video.path = "assets/new-pair.mp4".into();
    let mut solve = project.tracking_assets[0].clone();
    solve.path = "assets/tracking/new-pair.mstrack".into();
    solve.sha256 = "11".repeat(32);
    solve.source_sha256 = "09".repeat(32);
    let mut data = (*solve.data).clone();
    data.source_sha256 = [9; 32];
    for track in &mut data.tracks {
        track.id.push_str("-new");
    }
    solve.data = SharedData::new(data).unwrap();
    let mut bindings = Vec::new();
    for id in project.composition_ids() {
        let view = project.composition_view(&id).unwrap();
        let mut plane = view.layers[0].tracking_plane.clone().unwrap();
        plane.track.push_str("-new");
        let mut point = view.layers[1].tracking_point.clone().unwrap();
        point.track.push_str("-new");
        bindings.push(BindingUpdate::Plane {
            composition: id.clone(),
            object: 7,
            binding: if detach { None } else { Some(plane) },
        });
        bindings.push(BindingUpdate::Point {
            composition: id,
            object: 8,
            binding: if detach { None } else { Some(point) },
        });
    }
    Replacement {
        video,
        audio: None,
        tracking: vec![solve],
        bindings,
    }
}

#[test]
fn nested_point_and_plane_rebinding_restores_active_view_and_authored_corrections() {
    let before = nested_points_and_planes();
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply(Command::ReplaceTrackedVideo {
            replacement: Box::new(next(&before, false)),
        })
        .unwrap();
    let after = engine.snapshot();
    assert_eq!(after.composition_id, "comp-nested");
    assert_eq!(engine.revision(), 1);
    assert_eq!(
        after.tracking_assets[0].data.tracks[0].samples,
        before.tracking_assets[0].data.tracks[0].samples
    );
    for id in before.composition_ids() {
        let old = before.composition_view(&id).unwrap();
        let new = after.composition(&id).unwrap();
        assert_eq!(new.layers[0].effects, old.layers[0].effects);
        assert_eq!(new.layers[1].transform, old.layers[1].transform);
        assert_eq!(
            new.layers[0].tracking_plane.as_ref().unwrap().track,
            "feature-new"
        );
        assert_eq!(
            new.layers[1].tracking_point.as_ref().unwrap().track,
            "point-new"
        );
        // A 1-frame step is 2000 original ticks: point moves 10 source pixels,
        // or 20 target pixels, on top of each composition's independent offset.
        let x = new.layers[1].sampled_position(&new, 1.).unwrap()[0];
        assert_eq!(x, if id == "comp-nested" { 220. } else { 120. });
        assert_eq!(
            new.layers[0]
                .tracking_plane
                .as_ref()
                .unwrap()
                .displacement(&new, &new.layers[0], 1.)
                .unwrap(),
            old.layers[0]
                .tracking_plane
                .as_ref()
                .unwrap()
                .displacement(&before.composition(&id).unwrap(), &old.layers[0], 1.)
                .unwrap()
        );
        assert_eq!(
            new.layers[0]
                .tracking_plane
                .as_ref()
                .unwrap()
                .displacement(&new, &new.layers[0], 1.)
                .unwrap(),
            // Corner-pin offsets use this layer's 320x240 local coordinates,
            // while point offsets above use the 640x480 composition axes.
            [[10., 5.]; 4]
        );
    }
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
}

#[test]
fn explicit_point_plane_and_camera_detach_preserves_authored_values_and_is_undoable() {
    let before = nested_points_and_planes();
    let wire = serde_json::to_string(&Command::ReplaceTrackedVideo {
        replacement: Box::new(next(&before, true)),
    })
    .unwrap();
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply_batch(CommandRegistry::parse(&wire).unwrap())
        .unwrap();
    for id in before.composition_ids() {
        let new = engine.project().composition(&id).unwrap();
        let old = before.composition_view(&id).unwrap();
        assert!(new.layers[0].tracking_plane.is_none());
        assert!(new.layers[1].tracking_point.is_none());
        assert_eq!(new.layers[0].effects, old.layers[0].effects);
        assert_eq!(new.layers[1].transform, old.layers[1].transform);
    }
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    let before = fixture::camera_fixture();
    let mut paired = replacement(&before);
    paired.bindings = vec![BindingUpdate::Camera {
        composition: before.composition_id.clone(),
        object: 0,
        binding: None,
    }];
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply(Command::ReplaceTrackedVideo {
            replacement: Box::new(paired),
        })
        .unwrap();
    assert!(engine.project().camera.tracking.is_none());
    assert_eq!(engine.project().camera.position, before.camera.position);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}

#[test]
fn locked_nested_target_or_missing_composition_rolls_back_earlier_binding_changes() {
    for missing in [false, true] {
        let mut before = nested_points_and_planes();
        if !missing {
            before.layers[1].locked = true;
        }
        let mut paired = next(&before, false);
        if missing {
            paired.bindings.push(BindingUpdate::Point {
                composition: "absent".into(),
                object: 8,
                binding: None,
            });
        }
        let mut engine = Engine::new(before.clone()).unwrap();
        let error = engine
            .apply(Command::ReplaceTrackedVideo {
                replacement: Box::new(paired),
            })
            .unwrap_err();
        if missing {
            assert!(matches!(&error, Error::Composition(_)), "{error}");
        } else {
            assert!(
                error.to_string().contains("point layer is locked"),
                "{error}"
            );
        }
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.revision(), 0);
        assert!(!engine.undo().unwrap());
        assert!(!engine.redo().unwrap());
    }
}
