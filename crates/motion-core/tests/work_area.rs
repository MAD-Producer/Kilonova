use motion_core::{
    Command, CompositionAction, CompositionSettings, Engine, Layer, Project, WorkArea,
};

fn area(in_frame: u32, out_frame: u32) -> WorkArea {
    WorkArea {
        in_frame,
        out_frame,
    }
}

fn set(in_frame: u32, out_frame: u32) -> Command {
    Command::SetWorkArea {
        in_frame,
        out_frame,
    }
}

fn settings(frames: u32, fps: u32, timing: &str) -> Command {
    Command::Composition {
        action: CompositionAction::Settings {
            settings: CompositionSettings {
                name: "Work area".into(),
                width: 64,
                height: 64,
                fps,
                frames,
                timing: timing.into(),
                shorten: "trim".into(),
            },
        },
    }
}

#[test]
fn work_area_round_trips_history_and_does_not_trim_layers_or_keys() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    assert_eq!(project.work_area, WorkArea::full(120));
    let mut layer = Layer::solid(1, "Untrimmed", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.transform.position.set_at(2, [20., 32., 0.]).unwrap();
    layer
        .transform
        .position
        .set_at(110, [50., 32., 0.])
        .unwrap();
    project.layers.push(layer);
    let original = project.clone();
    let mut engine = Engine::new(project).unwrap();
    engine.apply(set(30, 75)).unwrap();
    assert_eq!(engine.project().work_area, area(30, 75));
    assert_eq!(engine.project().layers, original.layers);
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    assert_eq!(
        motion_core::storage::load(directory.path()).unwrap(),
        engine.snapshot()
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), original);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project().work_area, area(30, 75));
    assert_eq!(engine.project().layers, original.layers);
}

#[test]
fn invalid_ranges_and_batches_fail_atomically_with_original_bounds() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    engine.apply(set(20, 70)).unwrap();
    for (start, end) in [
        (40, 40),
        (41, 40),
        (0, 121),
        (120, 121),
        (u32::MAX, u32::MAX),
    ] {
        let before = engine.snapshot();
        let revision = engine.revision();
        let error = engine
            .apply_batch(vec![set(10, 60), set(start, end)])
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("invalid work area"), "{message}");
        assert!(message.contains(&format!("in_frame={start}")), "{message}");
        assert!(message.contains(&format!("out_frame={end}")), "{message}");
        assert_eq!(engine.snapshot(), before);
        assert_eq!(engine.revision(), revision);
    }
    engine.apply(set(119, 120)).unwrap();
    assert_eq!(engine.project().work_area, area(119, 120));
}

#[test]
fn cancelled_and_committed_gestures_preserve_single_history_entry() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    let original = engine.snapshot();
    engine.begin_gesture().unwrap();
    engine.apply(set(10, 100)).unwrap();
    engine.apply(set(15, 95)).unwrap();
    engine.end_gesture(false).unwrap();
    assert_eq!(engine.snapshot(), original);
    assert!(!engine.undo().unwrap());
    engine.begin_gesture().unwrap();
    engine.apply(set(10, 100)).unwrap();
    engine.apply(set(15, 95)).unwrap();
    engine.end_gesture(true).unwrap();
    assert!(engine.undo().unwrap());
    assert_eq!(engine.snapshot(), original);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project().work_area, area(15, 95));
}

#[test]
fn composition_switches_and_scoped_commands_keep_independent_ranges() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    engine.apply(set(20, 60)).unwrap();
    engine
        .apply(Command::Composition {
            action: CompositionAction::Create {
                settings: CompositionSettings {
                    name: "Child".into(),
                    width: 64,
                    height: 64,
                    fps: 60,
                    frames: 200,
                    timing: "preserve_frames".into(),
                    shorten: "reject".into(),
                },
            },
        })
        .unwrap();
    let child = engine.project().compositions[0].id.clone();
    assert_eq!(
        engine.project().composition_view(&child).unwrap().work_area,
        WorkArea::full(200)
    );
    engine
        .apply(Command::InComposition {
            composition: child.clone(),
            command: Box::new(set(100, 170)),
        })
        .unwrap();
    assert_eq!(engine.project().work_area, area(20, 60));
    engine.activate_composition(&child).unwrap();
    assert_eq!(engine.project().work_area, area(100, 170));
    assert_eq!(
        engine
            .project()
            .composition_frame_view(&child)
            .unwrap()
            .work_area,
        area(100, 170)
    );
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), engine.project()).unwrap();
    let reopened = motion_core::storage::load(directory.path()).unwrap();
    assert_eq!(reopened.work_area, area(20, 60));
    assert_eq!(
        reopened.composition(&child).unwrap().work_area,
        area(100, 170)
    );
}

#[test]
fn duration_and_fps_changes_preserve_policy_and_a_nonempty_range() {
    let mut engine = Engine::new(Project::new(64, 64, 30, 120).unwrap()).unwrap();
    engine.apply(settings(240, 30, "preserve_frames")).unwrap();
    assert_eq!(engine.project().work_area, WorkArea::full(240));
    engine.apply(set(30, 90)).unwrap();
    engine.apply(settings(240, 60, "preserve_seconds")).unwrap();
    assert_eq!(engine.project().work_area, area(60, 180));
    engine.apply(settings(240, 30, "preserve_frames")).unwrap();
    assert_eq!(engine.project().work_area, area(60, 180));
    engine.apply(settings(20, 30, "preserve_frames")).unwrap();
    assert_eq!(engine.project().work_area, area(19, 20));
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project().work_area, area(60, 180));
}

#[test]
fn required_serialized_range_rejects_missing_invalid_and_unknown_fields() {
    let project = Project::new(64, 64, 30, 120).unwrap();
    let original = serde_json::to_value(&project).unwrap();
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("work_area");
    assert!(serde_json::from_value::<Project>(missing)
        .unwrap_err()
        .to_string()
        .contains("work_area"));
    let mut invalid = project.clone();
    invalid.work_area = area(30, 30);
    assert!(invalid
        .validate()
        .unwrap_err()
        .to_string()
        .contains("invalid work area"));
    let mut unknown = original;
    unknown["work_area"]["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Project>(unknown)
        .unwrap_err()
        .to_string()
        .contains("unknown field"));
    let command =
        motion_core::parse_commands(r#"{"op":"set_work_area","in_frame":1,"out_frame":2}"#)
            .unwrap();
    let mut engine = Engine::new(project).unwrap();
    engine.apply_batch(command).unwrap();
    assert_eq!(engine.project().work_area, area(1, 2));
}

#[test]
fn audio_sample_edges_are_absolute_and_contiguous_at_fractional_rates() {
    for fps in [24, 30, 59, 60, 144, 240] {
        let whole = area(7, 111).sample_bounds(fps, 48_000).unwrap();
        let left = area(7, 53).sample_bounds(fps, 48_000).unwrap();
        let right = area(53, 111).sample_bounds(fps, 48_000).unwrap();
        assert_eq!(left.start, whole.start);
        assert_eq!(left.end, right.start);
        assert_eq!(right.end, whole.end);
        assert_eq!(whole.start, 7 * 48_000 / u64::from(fps));
        assert_eq!(whole.end, 111 * 48_000 / u64::from(fps));
    }
    assert!(area(0, 1).sample_bounds(0, 48_000).is_err());
    assert!(area(0, 1).sample_bounds(30, 0).is_err());
    assert!(area(3, 3).sample_bounds(30, 48_000).is_err());
}

#[test]
fn invalid_owner_clock_returns_an_error_without_panicking_or_mutating_settings() {
    let mut project = Project::new(64, 64, 30, 120).unwrap();
    project.fps = 0;
    let original = project.clone();
    let Command::Composition { action } = settings(120, 30, "preserve_frames") else {
        unreachable!()
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        project.edit_composition(action)
    }));
    let error = result.expect("invalid clock must not panic").unwrap_err();
    assert!(error.to_string().contains("invalid work area resize clock"));
    assert_eq!(project, original);
}
