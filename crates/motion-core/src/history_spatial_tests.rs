use super::*;
use motion_model::keyframe_clipboard::spatial::SpatialKeyEdit;

fn project() -> Project {
    let mut project = Project::new(640, 360, 30, 90).unwrap();
    let mut layer = Layer::solid(1, "entry-00", [32.; 2], [0.; 3], [1.; 4]);
    for (frame, x) in [(0, 0.), (12, 7.), (30, 40.)] {
        layer
            .transform
            .position
            .upsert(frame, [x, 0., 0.], Ease::Linear)
            .unwrap();
    }
    layer
        .transform
        .position
        .set_spatial(
            0,
            Some(crate::SpatialTangents {
                incoming: None,
                outgoing: Some([80., 180., 0.]),
            }),
        )
        .unwrap();
    layer.transform.position.enable_roving(&[12]).unwrap();
    project.layers.push(layer);
    project
}
fn rename(index: u32) -> Command {
    Command::Rename {
        object: 1,
        name: format!("entry-{index:02}"),
    }
}
fn time(object: u64, key: u64, time: f64) -> Command {
    Command::SpatialKeyframes {
        target: crate::KeyframeTarget::Property {
            object,
            property: Property::Position,
            axis: None,
        },
        edit: SpatialKeyEdit::Time { key, time },
    }
}
fn cache_bytes(project: &Project) -> usize {
    project.layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .cache_usage()
        .retained_bytes()
}
fn json_bytes(project: &Project) -> usize {
    serde_json::to_vec(project).unwrap().len()
}
fn usage(engine: &Engine) -> crate::history_accounting::HistoryUsage {
    crate::history_accounting::history_usage(
        engine
            .undo
            .iter()
            .chain(&engine.redo)
            .map(|entry| &entry.size),
    )
}

#[test]
fn spatial_history_cache_alone_changes_the_real_budget_boundary() {
    let project = project();
    let json = json_bytes(&project);
    let cache = cache_bytes(&project);
    assert!(
        cache > json,
        "curved derived geometry must exceed this small fixture's JSON"
    );
    for (budget, retained) in [(json, 0), (json + cache - 1, 0), (json + cache, 1)] {
        let mut engine = Engine::new(project.clone()).unwrap();
        engine.history_budget = budget;
        engine.apply(rename(1)).unwrap();
        assert_eq!(engine.undo.len(), retained, "budget={budget}");
        assert!(usage(&engine).total() <= budget);
        assert_eq!(engine.history_evictions, u64::from(retained == 0));
        assert_eq!(
            engine.history_diagnostics()["spatialCacheBytes"],
            if retained == 0 { 0 } else { cache }
        );
        assert_eq!(
            engine.history_diagnostics()["retainedBytes"],
            usage(&engine).total()
        );
        assert_eq!(engine.can_undo(), retained == 1);
    }
}

#[test]
fn shared_cache_is_counted_once_and_rebuilt_history_caches_remain_distinct() {
    let mut engine = Engine::new(project()).unwrap();
    let first = cache_bytes(engine.project());
    for index in 1..=4 {
        engine.apply(rename(index)).unwrap();
    }
    assert_eq!(
        usage(&engine).spatial_bytes,
        first,
        "renames share one Arc allocation"
    );
    assert_eq!(
        usage(&engine).model_bytes,
        engine
            .undo
            .iter()
            .map(|h| json_bytes(&h.project))
            .sum::<usize>()
    );
    engine.apply(time(1, 2, 8.25)).unwrap();
    let second = cache_bytes(engine.project());
    engine.apply(rename(5)).unwrap();
    assert_eq!(usage(&engine).spatial_bytes, first + second);
    let expected = usage(&engine).total();
    for _ in 0..3 {
        engine.undo().unwrap();
        assert_eq!(usage(&engine).total(), expected);
    }
    for _ in 0..3 {
        engine.redo().unwrap();
        assert_eq!(usage(&engine).total(), expected);
    }
    let diagnostics = engine.history_diagnostics();
    let before = engine.snapshot();
    let revision = engine.revision();
    assert!(engine
        .apply(time(1, 999, 9.))
        .unwrap_err()
        .to_string()
        .contains("identity is missing"));
    assert_eq!(engine.history_diagnostics(), diagnostics);
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), revision);
    engine.history_budget = 0;
    engine.trim();
    assert_eq!(usage(&engine).total(), 0);
    assert!(!engine.can_undo() && !engine.can_redo());
}

#[test]
fn branching_drops_redo_before_budget_trimming_for_commands_and_gestures() {
    for gesture in [false, true] {
        let mut engine = Engine::new(project()).unwrap();
        engine.apply(rename(1)).unwrap();
        engine.apply(rename(2)).unwrap();
        engine.undo().unwrap();
        let before = engine.snapshot();
        engine.history_budget = json_bytes(&before) + cache_bytes(&before);
        if gesture {
            engine.begin_gesture().unwrap();
        }
        engine.apply(rename(3)).unwrap();
        if gesture {
            engine.end_gesture(true).unwrap();
        }
        assert_eq!(engine.redo.len(), 0);
        assert_eq!(
            engine.undo.len(),
            1,
            "discarded redo evicted a usable undo, gesture={gesture}"
        );
        assert!(usage(&engine).total() <= engine.history_budget);
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project(), &before);
    }
}

#[test]
fn gestures_refresh_derived_cache_owners_only_on_commit_and_cancel_restores_identity() {
    let mut engine = Engine::new(project()).unwrap();
    let baseline = engine.snapshot();
    let original_cache = cache_bytes(&baseline);
    for commit in [false, true] {
        engine.begin_gesture().unwrap();
        crate::history_accounting::reset_serialized_bytes();
        for x in [5., 6., 7.] {
            engine
                .apply(Command::SetVector {
                    object: 1,
                    property: Property::Position,
                    frame: 0,
                    value: [x, 0., 0.],
                })
                .unwrap();
        }
        assert_eq!(crate::history_accounting::serialized_visit_bytes(), 0);
        engine.end_gesture(commit).unwrap();
        if commit {
            engine.apply(rename(1)).unwrap();
            assert_eq!(
                usage(&engine).spatial_bytes,
                original_cache + cache_bytes(engine.project())
            );
        } else {
            assert_eq!(engine.project(), &baseline);
            engine.apply(rename(0)).unwrap(); // ordinary no-op creates no history
            assert_eq!(usage(&engine).total(), 0);
        }
    }
}

#[test]
fn all_compositions_anchors_and_camera_owners_contribute_without_activation_changes() {
    let mut project = project();
    let source = project.layers[0].transform.position.clone();
    project.camera.created = true;
    project.camera.position = source.clone();
    project.camera.target = source.clone();
    let mut camera = Layer::camera(2, "camera", 640, 360);
    if let crate::Content::Camera { camera } = &mut camera.content {
        camera.position = source.clone();
        camera.target = source.clone();
    }
    // Camera layers own position/target in Camera and require a static
    // Transform. Exercise the supported anchor owner on the solid layer.
    let anchor = &mut project.layers[0].transform.anchor;
    for (frame, x) in [(0, 0.1), (12, 0.2), (30, 0.6)] {
        anchor.upsert(frame, [x, 0.5], Ease::Linear).unwrap();
    }
    anchor.enable_roving(&[12]).unwrap();
    let anchor_bytes = anchor
        .spatial_animation
        .as_ref()
        .unwrap()
        .cache_usage()
        .retained_bytes();
    project.layers.push(camera);
    let mut node = crate::Composition {
        id: "comp-spatial-budget".into(),
        name: "secondary".into(),
        width: 640,
        height: 360,
        fps: 30,
        frames: 90,
        work_area: project.work_area,
        guides: Vec::new(),
        background: project.background,
        camera: project.camera.clone(),
        layers: project.layers.clone(),
        expressions: Vec::new(),
    };
    // Same values but independently prepared allocation must count separately.
    node.camera.position = serde_json::from_slice(&serde_json::to_vec(&source).unwrap()).unwrap();
    let separate_bytes = node
        .camera
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .cache_usage()
        .retained_bytes();
    project.compositions.push(node);
    let mut engine = Engine::new(project).unwrap();
    engine.apply(rename(1)).unwrap();
    assert_eq!(
        usage(&engine).spatial_bytes,
        cache_bytes(engine.project()) + anchor_bytes + separate_bytes
    );
    let before = usage(&engine).total();
    engine.activate_composition("comp-spatial-budget").unwrap();
    assert_eq!(usage(&engine).total(), before);
    engine.apply(time(0, 2, 7.25)).unwrap();
    engine
        .apply(Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    let extra = engine
        .project()
        .camera
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .cache_usage()
        .retained_bytes();
    assert_eq!(
        usage(&engine).spatial_bytes,
        cache_bytes(engine.project()) + anchor_bytes + separate_bytes + extra
    );
}

#[test]
fn deleted_owners_keep_only_their_retained_undo_cache_until_that_entry_is_evicted() {
    let mut engine = Engine::new(project()).unwrap();
    let old_bytes = cache_bytes(engine.project());
    engine.apply(Command::Delete { object: 1 }).unwrap();
    assert!(engine.project().layers.is_empty());
    assert_eq!(usage(&engine).spatial_bytes, old_bytes);
    let empty_project = engine.snapshot();
    // Enough for only the cache-free pre-color snapshot. A stale cache in the
    // new ProjectSize would evict this valid undo as well as the deleted layer.
    engine.history_budget = json_bytes(&empty_project);
    engine
        .apply(Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    assert_eq!(engine.undo.len(), 1);
    assert_eq!(usage(&engine).spatial_bytes, 0);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &empty_project);
    assert!(!engine.undo().unwrap());
}

#[test]
fn semantic_noops_and_gestures_returning_to_origin_restore_the_cached_allocation_identity() {
    let mut engine = Engine::new(project()).unwrap();
    let original = engine.snapshot();
    let original_cache = original.layers[0]
        .transform
        .position
        .spatial_animation
        .as_ref()
        .unwrap()
        .cache_usage();
    let current_cache = |engine: &Engine| {
        engine.project().layers[0]
            .transform
            .position
            .spatial_animation
            .as_ref()
            .unwrap()
            .cache_usage()
    };
    engine.apply(time(1, 1, 0.)).unwrap();
    assert!(
        original_cache == current_cache(&engine),
        "equal rebuilt cache escaped a no-op command"
    );
    assert_eq!(engine.revision(), 0);
    assert_eq!(usage(&engine).total(), 0);
    for commit in [false, true] {
        engine.begin_gesture().unwrap();
        for x in [5., 0.] {
            engine
                .apply(Command::SetVector {
                    object: 1,
                    property: Property::Position,
                    frame: 0,
                    value: [x, 0., 0.],
                })
                .unwrap();
        }
        assert_eq!(engine.project(), &original);
        engine.end_gesture(commit).unwrap();
        assert!(
            original_cache == current_cache(&engine),
            "equal preview cache survived, commit={commit}"
        );
        assert_eq!(usage(&engine).total(), 0);
        assert!(!engine.can_undo() && !engine.can_redo());
    }
    engine.apply(rename(1)).unwrap();
    assert_eq!(
        usage(&engine).spatial_bytes,
        original_cache.retained_bytes()
    );
}
