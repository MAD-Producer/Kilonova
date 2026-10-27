use super::*;
use crate::history_accounting::{reset_serialized_bytes, serialized_visit_bytes};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

// Per-thread instrumentation measures only the accounting operation; unrelated
// tests and the pre-existing deep snapshot clone do not enter these totals.
thread_local! { static ALLOCATIONS: Cell<(bool, usize, usize)> = const { Cell::new((false, 0, 0)) }; }
struct MeasuringAllocator;
fn allocated(bytes: usize) {
    let _ = ALLOCATIONS.try_with(|state| {
        let (enabled, current, peak) = state.get();
        if enabled {
            let current = current.saturating_add(bytes);
            state.set((true, current, peak.max(current)));
        }
    });
}
fn released(bytes: usize) {
    let _ = ALLOCATIONS.try_with(|state| {
        let (enabled, current, peak) = state.get();
        if enabled {
            state.set((true, current.saturating_sub(bytes), peak));
        }
    });
}
unsafe impl GlobalAlloc for MeasuringAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        released(layout.size());
        unsafe {
            System.dealloc(pointer, layout);
        }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            if size >= layout.size() {
                allocated(size - layout.size());
            } else {
                released(layout.size() - size);
            }
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: MeasuringAllocator = MeasuringAllocator;
fn peak_allocation<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ALLOCATIONS.with(|state| state.set((false, 0, 0)));
        }
    }
    ALLOCATIONS.with(|state| state.set((true, 0, 0)));
    let reset = Reset;
    let result = operation();
    let peak = ALLOCATIONS.with(|state| state.get().2);
    drop(reset);
    (result, peak)
}

fn exact(project: &Project) -> usize {
    serde_json::to_vec(project).unwrap().len()
}

#[test]
fn guide_metadata_accounting_matches_real_json_after_scoped_edits_and_history_moves() {
    let mut project = Project::new(640, 360, 30, 90).unwrap();
    project.compositions.push(crate::Composition {
        id: "comp-guide-history".into(), name: "Guide history".into(),
        width: project.width, height: project.height, fps: project.fps, frames: project.frames,
        work_area: project.work_area, guides: Vec::new(), background: project.background,
        camera: project.camera.clone(), layers: Vec::new(), expressions: Vec::new(),
    });
    let mut engine = Engine::new(project).unwrap();
    for composition in [crate::MAIN_COMPOSITION, "comp-guide-history"] {
        for count in [crate::MAX_GUIDES, 1, 0] {
            let guides = (0..count).map(|index| crate::Guide {
                id: index as u64 + 1, axis: crate::GuideAxis::X, position: index as f64 + 0.25,
            }).collect();
            engine.apply(Command::InComposition { composition: composition.into(),
                command: Box::new(Command::SetGuides { guides }) }).unwrap();
            assert_exact(&engine);
        }
    }
    while engine.undo().unwrap() { assert_exact(&engine); }
    while engine.redo().unwrap() { assert_exact(&engine); }
    assert_exact(&engine);
}
fn assert_exact(engine: &Engine) {
    if engine.gesture.is_none() {
        assert_eq!(engine.size.bytes(), exact(&engine.project));
    }
    for entry in engine.undo.iter().chain(&engine.redo) {
        assert_eq!(entry.size.bytes(), exact(&entry.project));
    }
    if let Some(gesture) = &engine.gesture {
        assert_eq!(gesture.before.size.bytes(), exact(&gesture.before.project));
    }
}
fn rename(name: &str) -> Command {
    Command::Rename {
        object: 1,
        name: name.into(),
    }
}

#[test]
fn work_area_history_counts_changed_header_without_reserializing_layer_payloads() {
    let mut project = Project::new(64, 64, 30, 36_000).unwrap();
    let mut layer = Layer::solid(1, "Unchanged animated payload", [64.; 2], [32., 32., 0.], [1.; 4]);
    for frame in 0..1000 {
        layer.transform.position.set_at(frame, [frame as f32, 32., 0.]).unwrap();
    }
    project.layers.push(layer);
    let mut engine = Engine::new(project).unwrap();
    for (in_frame, out_frame) in [(1, 2), (1000, 35_999), (0, 36_000)] {
        reset_serialized_bytes();
        engine.apply(Command::SetWorkArea { in_frame, out_frame }).unwrap();
        assert!(serialized_visit_bytes() < 4096, "range edits must reuse untouched animation sizes");
        assert_exact(&engine);
    }
    for _ in 0..3 {
        assert!(engine.undo().unwrap());
        assert_exact(&engine);
    }
    for _ in 0..3 {
        assert!(engine.redo().unwrap());
        assert_exact(&engine);
    }
}

#[test]
fn history_accounting_matches_json_through_payload_changes_and_undo_redo() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    assert_exact(&engine);
    for command in [
        rename("中\n\"quoted\"\\name"),
        Command::SetVector {
            object: 1,
            property: Property::Position,
            frame: 0,
            value: [10., 20., 30.],
        },
        Command::SetScalar {
            object: 1,
            property: Property::Opacity,
            frame: 3,
            value: 0.75,
        },
        Command::SeparateDimensions {
            object: 1,
            property: Property::Position,
        },
        Command::MergeDimensions {
            object: 1,
            property: Property::Position,
        },
        Command::SeparateDimensions {
            object: 1,
            property: Property::Position,
        },
        Command::SetComponent {
            object: 1,
            property: Property::Position,
            axis: crate::Axis::X,
            frame: 5,
            value: 123.,
        },
        Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        },
        Command::Reorder {
            object: 1,
            index: 2,
        },
        Command::Duplicate { object: 1 },
        Command::Delete { object: 4 },
        Command::RegisterAsset {
            asset: crate::Asset {
                id: 1,
                path: "assets/picture.png".into(),
                width: 64,
                height: 64,
            },
        },
    ] {
        engine.apply(command).unwrap();
        assert_exact(&engine);
    }
    reset_serialized_bytes();
    while engine.undo().unwrap() {
        assert_exact(&engine);
    }
    while engine.redo().unwrap() {
        assert_exact(&engine);
    }
    assert_eq!(
        serialized_visit_bytes(),
        0,
        "history navigation reserialized payloads"
    );
}

#[test]
fn history_accounting_optional_metadata_counts_json_not_external_media_payloads() {
    let mut project = Project::demo().unwrap();
    project.fonts.push(crate::FontAsset {
        id: "test-font".into(),
        path: "assets/fonts/test.ttf".into(),
        name: "Font 中文".into(),
        face_index: 0,
        license: "license\\n\"quoted\"\n".repeat(1024),
    });
    project.audio_assets.push(crate::AudioAsset {
        id: 50,
        path: "assets/sound.wav".into(),
        mime: "audio/wav".into(),
        bytes: 2_000_000_000,
        track: 0,
        sample_rate: 48_000,
        channels: 2,
        sample_frames: 48_000,
        duration_us: 1_000_000,
    });
    project.video_assets.push(crate::VideoAsset {
        id: 51,
        path: "assets/video.mp4".into(),
        mime: "video/avc".into(),
        bytes: 2_000_000_000,
        track: 0,
        width: 64,
        height: 64,
        rotation: 0,
        display_width: 64,
        display_height: 64,
        video_start_us: 0,
        video_end_us: 1_000_000,
        duration_us: 1_000_000,
        frame_count: 30,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    let (standalone, allocated) = peak_allocation(|| project.estimated_bytes());
    assert_eq!(standalone, exact(&project));
    assert_eq!(
        allocated, 0,
        "standalone size count allocated a JSON document"
    );
    let mut engine = Engine::new(project).unwrap();
    assert!(
        engine.size.bytes() < 100_000,
        "source payload bytes entered history accounting"
    );
    assert_exact(&engine);
    reset_serialized_bytes();
    engine.apply(rename("Metadata cache reused")).unwrap();
    assert!(
        serialized_visit_bytes() < 4_000,
        "unchanged metadata was reserialized"
    );
    assert_exact(&engine);
}

#[test]
fn history_accounting_serialization_failure_keeps_the_fail_closed_size() {
    struct Refused;
    impl serde::Serialize for Refused {
        fn serialize<S: serde::Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("injected serialization failure"))
        }
    }
    assert_eq!(motion_model::serialized_bytes(&Refused), usize::MAX);
}

#[test]
fn history_accounting_exact_budget_boundary_and_128_entry_limit_are_unchanged() {
    let project = Project::demo().unwrap();
    let initial = exact(&project);
    for (budget, depth) in [(initial - 1, 0), (initial, 1), (initial + 1, 1)] {
        let mut engine = Engine::new(project.clone()).unwrap();
        engine.history_budget = budget;
        engine.apply(rename("changed")).unwrap();
        assert_eq!(engine.undo.len(), depth);
        assert_exact(&engine);
    }
    let mut engine = Engine::new(project).unwrap();
    let mut expected = VecDeque::new();
    for index in 0..200 {
        let before = engine.project.clone();
        expected.push_back(before);
        if expected.len() > 128 {
            expected.pop_front();
        }
        engine.apply(rename(&format!("entry-{index:03}"))).unwrap();
        assert_exact(&engine);
    }
    assert_eq!(engine.undo.len(), 128);
    for project in expected.into_iter().rev() {
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project, project);
    }
    assert!(!engine.undo().unwrap());
}

#[test]
fn history_accounting_budget_eviction_matches_the_previous_json_policy() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    engine.history_budget = exact(&engine.project) * 4;
    let mut expected = VecDeque::new();
    for index in 0..160 {
        let before = engine.project.clone();
        expected.push_back((exact(&before), before));
        while expected.len() > 128
            || expected.iter().map(|entry| entry.0).sum::<usize>() > engine.history_budget
        {
            expected.pop_front();
        }
        engine.apply(rename(&format!("edit-{index:03}"))).unwrap();
        assert_eq!(engine.undo.len(), expected.len());
        assert!(
            engine
                .undo
                .iter()
                .map(|entry| entry.size.bytes())
                .sum::<usize>()
                <= engine.history_budget
        );
        assert_exact(&engine);
    }
    for (_, project) in expected.into_iter().rev() {
        assert!(engine.undo().unwrap());
        assert_eq!(engine.project, project);
    }
}

#[test]
fn history_accounting_gestures_defer_counting_and_cancel_reuses_the_baseline() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    let baseline = engine.project.clone();
    engine.begin_gesture().unwrap();
    reset_serialized_bytes();
    for index in 0..40 {
        engine.apply(rename(&format!("drag-{index:02}"))).unwrap();
    }
    assert_eq!(
        serialized_visit_bytes(),
        0,
        "mouse updates performed size walks"
    );
    engine.end_gesture(false).unwrap();
    assert_eq!(
        serialized_visit_bytes(),
        0,
        "gesture cancellation reserialized payloads"
    );
    assert_eq!(engine.project, baseline);
    assert_exact(&engine);
    engine.begin_gesture().unwrap();
    for index in 0..40 {
        engine.apply(rename(&format!("commit-{index:02}"))).unwrap();
    }
    assert_eq!(serialized_visit_bytes(), 0);
    engine.end_gesture(true).unwrap();
    assert!(serialized_visit_bytes() > 0);
    assert_eq!(engine.undo.len(), 1);
    assert_exact(&engine);
    reset_serialized_bytes();
    engine.undo().unwrap();
    engine.redo().unwrap();
    assert_eq!(serialized_visit_bytes(), 0);
    assert_exact(&engine);
}

#[test]
fn history_accounting_failed_batch_prepare_and_save_preserve_the_cache() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    let original = engine.project.clone();
    reset_serialized_bytes();
    assert!(engine
        .apply_batch(vec![rename("changed"), Command::Delete { object: 999 }])
        .is_err());
    assert_eq!(engine.project, original);
    assert_exact(&engine);
    assert!(engine
        .apply_batch_with_prepare(vec![rename("prepared")], |_| Err(Error::Invalid(
            "refused".into()
        )))
        .is_err());
    let directory = tempfile::tempdir().unwrap();
    let invalid_root = directory.path().join("not-a-directory");
    std::fs::write(&invalid_root, b"occupied").unwrap();
    assert!(engine
        .apply_batch_saved(vec![rename("save failed")], &invalid_root)
        .is_err());
    assert_eq!(engine.project, original);
    assert_eq!(serialized_visit_bytes(), 0);
    assert!(!engine.can_undo());
    assert_exact(&engine);
}

#[test]
fn history_accounting_composition_routing_and_activation_keep_exact_payload_sizes() {
    let mut project = Project::demo().unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.compositions.push(crate::Composition {
        id: "comp-child".into(),
        name: "Child\n\"quoted\"".into(),
        width: 64,
        height: 64,
        fps: 30,
        frames: 180,
        work_area: crate::WorkArea::full(180),
        guides: Vec::new(),
        background: [0.; 4],
        camera: crate::Camera::new(64, 64),
        layers: vec![Layer::solid(
            1,
            "Same local ID",
            [20.; 2],
            [32., 32., 0.],
            [1.; 4],
        )],
        expressions: Vec::new(),
    });
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply(Command::InComposition {
            composition: "comp-child".into(),
            command: Box::new(rename("Child changed")),
        })
        .unwrap();
    assert_exact(&engine);
    engine.activate_composition("comp-child").unwrap();
    assert_exact(&engine);
    engine.apply(rename("Active child changed")).unwrap();
    assert_exact(&engine);
    engine
        .activate_composition(crate::MAIN_COMPOSITION)
        .unwrap();
    assert_exact(&engine);
    while engine.undo().unwrap() {
        assert_exact(&engine);
    }
    while engine.redo().unwrap() {
        assert_exact(&engine);
    }
}

fn large_project() -> Project {
    let mut project = Project::new(64, 64, 30, 600).unwrap();
    for id in 1..=32 {
        let mut layer = Layer::solid(id, "Key tree", [20.; 2], [32., 32., 0.], [1.; 4]);
        for frame in 0..512 {
            layer
                .transform
                .position
                .upsert(frame, [frame as f32, 32., 0.], Ease::Linear)
                .unwrap();
        }
        project.layers.push(layer);
    }
    project
}

#[test]
fn history_accounting_single_layer_visits_do_not_scale_with_unmodified_key_trees() {
    let mut engine = Engine::new(large_project()).unwrap();
    let full_json = exact(&engine.project);
    reset_serialized_bytes();
    engine.apply(rename("One changed layer")).unwrap();
    let visited = serialized_visit_bytes();
    assert!(
        visited * 16 < full_json,
        "single-layer count walked too much: {visited}/{full_json}"
    );
    assert_exact(&engine);
    reset_serialized_bytes();
    engine.undo().unwrap();
    engine.redo().unwrap();
    assert_eq!(serialized_visit_bytes(), 0);
}

#[test]
fn history_accounting_default_32_mib_budget_trims_large_batch_edits_like_json_lengths() {
    let mut engine = Engine::new(large_project()).unwrap();
    let mut expected = VecDeque::new();
    for index in 0..80 {
        expected.push_back(exact(&engine.project));
        while expected.len() > 128 || expected.iter().sum::<usize>() > 32 * 1024 * 1024 {
            expected.pop_front();
        }
        engine.apply(rename(&format!("Large-{index:03}"))).unwrap();
        assert_eq!(engine.undo.len(), expected.len());
        assert!(
            engine
                .undo
                .iter()
                .map(|entry| entry.size.bytes())
                .sum::<usize>()
                <= 32 * 1024 * 1024
        );
        assert_eq!(engine.size.bytes(), exact(&engine.project));
    }
    assert!(
        engine.undo.len() < 80,
        "fixture must exercise byte eviction, not only entry count"
    );
}

#[test]
fn history_accounting_peak_allocation_is_independent_of_untouched_key_payloads() {
    let project = large_project();
    let size = crate::history_accounting::ProjectSize::new(&project);
    let mut edited = project.clone();
    edited.layers[0].name = "Measured changed layer".into();
    let mut changes = crate::history_accounting::Changes::default();
    changes.note(&rename("Measured changed layer"), crate::MAIN_COMPOSITION);
    let (legacy, legacy_peak) = peak_allocation(|| serde_json::to_vec(&edited).unwrap());
    let (cached, cached_peak) = peak_allocation(|| size.updated(&edited, &changes));
    assert_eq!(cached.bytes(), legacy.len());
    assert!(legacy_peak >= legacy.len());
    assert!(
        cached_peak * 32 < legacy.len(),
        "accounting allocated too much: cached peak={cached_peak}, JSON={}",
        legacy.len()
    );
}

#[test]
#[ignore = "manual performance measurement on a large document"]
fn history_accounting_large_document_measurement() {
    let project = large_project();
    let size = crate::history_accounting::ProjectSize::new(&project);
    let mut edited = project.clone();
    edited.layers[0].name = "Measured single layer".into();
    let mut changes = crate::history_accounting::Changes::default();
    changes.note(&rename("Measured single layer"), crate::MAIN_COMPOSITION);
    let began = std::time::Instant::now();
    let (legacy_bytes, legacy_peak) = peak_allocation(|| serde_json::to_vec(&edited).unwrap());
    let legacy_time = began.elapsed();
    let began = std::time::Instant::now();
    reset_serialized_bytes();
    let (cached, cached_peak) = peak_allocation(|| size.updated(&edited, &changes));
    let cached_time = began.elapsed();
    let cached_visits = serialized_visit_bytes();
    assert_eq!(cached.bytes(), legacy_bytes.len());
    let mut engine = Engine::new(project).unwrap();
    let began = std::time::Instant::now();
    engine.apply(rename("Measured single layer")).unwrap();
    let committed_time = began.elapsed();
    println!("legacy accounting: {:?}, JSON={} bytes, peak={} bytes; cached accounting: {:?}, peak={} bytes, serialized component bytes={}; whole cached edit: {:?}",
        legacy_time, legacy_bytes.len(), legacy_peak, cached_time, cached_peak, cached_visits, committed_time);
    assert_exact(&engine);
}

#[test]
fn history_evictions_count_real_unavailable_undo_entries_and_budget_pressure() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    assert_eq!(engine.history_diagnostics()["evictions"],0);
    for index in 0..200 { engine.apply(rename(&format!("history-{index:03}"))).unwrap(); }
    assert_eq!(engine.history_diagnostics()["evictions"],72);
    assert_eq!(engine.history_diagnostics()["undoEntries"],128);
    let mut undone = 0;
    while engine.undo().unwrap() { undone += 1; }
    assert_eq!(undone,128);
    assert_eq!(engine.project.layers[0].name,"history-071");
    assert_eq!(engine.history_diagnostics()["evictions"],72);
    assert_eq!(engine.history_diagnostics()["redoEntries"],128);
    while engine.redo().unwrap() {}
    assert_eq!(engine.history_diagnostics()["evictions"],72);

    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    engine.history_budget = exact(engine.project())-1;
    engine.apply(rename("budget-trimmed")).unwrap();
    assert_eq!(engine.history_diagnostics()["evictions"],1);
    assert_eq!(engine.history_diagnostics()["undoEntries"],0);
    assert!(!engine.undo().unwrap());
}

#[test]
fn active_gesture_age_distinguishes_history_blocking_without_changing_undo_policy() {
    let mut engine = Engine::new(Project::demo().unwrap()).unwrap();
    assert!(engine.history_diagnostics()["gestureElapsedMs"].is_null());
    engine.apply(rename("before gesture")).unwrap();
    assert!(engine.can_undo());
    engine.begin_gesture().unwrap();
    engine.gesture.as_mut().unwrap().started -= std::time::Duration::from_secs(3);
    let diagnostics = engine.history_diagnostics();
    assert_eq!(diagnostics["gestureActive"], true);
    assert!(diagnostics["gestureElapsedMs"].as_u64().unwrap() >= 3000);
    assert_eq!(diagnostics["undoEntries"], 1);
    assert!(!engine.can_undo());
    assert!(matches!(engine.begin_gesture(), Err(Error::Gesture)));
    engine.end_gesture(false).unwrap();
    assert!(engine.history_diagnostics()["gestureElapsedMs"].is_null());
    assert!(engine.can_undo());
    assert!(matches!(engine.end_gesture(false), Err(Error::Gesture)));
    assert!(engine.undo().unwrap());
}
