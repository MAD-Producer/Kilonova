use super::*;
use motion_core::{Ease, Engine, KeyframeTarget};

fn target() -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    }
}
fn fixture() -> Engine {
    let mut project = super::super::curve_comparison::tests::fixture().snapshot();
    let track = &mut project.layers[0].transform.position;
    track.keys.clear();
    for (time, x) in [(0, 0.), (4, 1.), (10, 4.), (20, 8.)] {
        track.upsert(time, [x, 10., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4]).unwrap();
    Engine::new(project).unwrap()
}
fn state(engine: &Engine) -> Value {
    super::super::curve_comparison::tests::state(engine)
}

#[test]
fn cached_graph_reads_fractional_spatial_times_and_real_velocity_without_repreparing() {
    let engine = fixture();
    let original = engine.snapshot();
    let snapshot = state(&engine);
    let mut cache = Cache::default();
    let first = cache.load(&snapshot, &target()).unwrap();
    let Samples::Vector(track) = &*first else {
        panic!("vector track");
    };
    let usage = track.spatial_animation.as_ref().unwrap().cache_usage();
    for _ in 0..90 {
        let again = cache.load(&snapshot, &target()).unwrap();
        assert!(Arc::ptr_eq(&first, &again));
        let Samples::Vector(track) = &*again else {
            panic!("vector track");
        };
        assert!(usage == track.spatial_animation.as_ref().unwrap().cache_usage());
        assert!((again.value(5., 0).unwrap() - 2.).abs() < 1e-6);
        assert!((again.velocity(5., 0, 30.).unwrap() - 12.).abs() < 1e-6);
    }
    let data =
        super::super::curve_comparison::data::load(&snapshot, &target(), Locale::Zh).unwrap();
    assert!(data.frames.is_empty(), "no rounded shadow keys");
    assert_eq!(
        data.spatial_frames,
        [(1, 15.), (2, 17.5), (3, 25.), (4, 35.)]
    );
    assert_eq!(data.sample(20., 0, 30., false).unwrap(), 2.);
    assert_eq!(data.sample(20., 0, 30., true).unwrap(), 12.);
    assert_eq!(
        data::sample(&snapshot, 7, Property::Position, None, 20.).unwrap(),
        serde_json::json!([2., 10., 0.])
    );
    assert_eq!(engine.project(), &original);
}

#[test]
fn graph_cache_invalidates_document_revision_and_channel_with_identical_object_ids() {
    let snapshot = state(&fixture());
    let mut cache = Cache::default();
    let mut previous = cache.load(&snapshot, &target()).unwrap();
    for field in ["revision", "document_generation", "root", "composition"] {
        let mut changed = snapshot.clone();
        changed[field] = if field == "root" || field == "composition" {
            serde_json::json!("other")
        } else {
            serde_json::json!(9)
        };
        let current = cache.load(&changed, &target()).unwrap();
        assert!(!Arc::ptr_eq(&previous, &current), "{field} must invalidate");
        previous = current;
    }
    let opacity = KeyframeTarget::Property {
        object: 7,
        property: Property::Opacity,
        axis: None,
    };
    let other = cache.load(&snapshot, &opacity).unwrap();
    assert!(matches!(&*other, Samples::Scalar(_)));
    assert!(!Arc::ptr_eq(&previous, &other));
    for field in ["revision", "document_generation", "root", "composition"] {
        let mut incomplete = snapshot.clone();
        incomplete.as_object_mut().unwrap().remove(field);
        // The project carries a valid composition fallback.
        if field == "composition" {
            incomplete["project"]
                .as_object_mut()
                .unwrap()
                .remove("composition_id");
        }
        let a = cache.load(&incomplete, &target()).unwrap();
        let b = cache.load(&incomplete, &target()).unwrap();
        assert!(
            !Arc::ptr_eq(&a, &b),
            "missing {field} cannot authorize reuse"
        );
    }
}

#[test]
fn spatial_fit_selection_and_knots_keep_fractional_and_coincident_identities() {
    let snapshot = state(&fixture());
    let mut panels = Panels::default();
    panels
        .selected_keys
        .apply_addressed(
            &snapshot,
            [super::super::key_selection::SpatialKey {
                target: target(),
                id: 2,
            }
            .into()],
            super::super::key_selection::Mode::Replace,
        )
        .unwrap();
    let source = &snapshot["timeline_layers"][0]["properties"]["position"];
    assert_eq!(
        fit_intervals(&panels, &snapshot, &target(), source, 15),
        ([15., 35.], Some([17.5, 17.5]))
    );
    let mut project = fixture().snapshot();
    let track = &mut project.layers[0].transform.position;
    track.keys.clear();
    track.spatial_animation = None;
    for (time, x) in [(0, 0.), (4, 1.), (10, 1.), (20, 8.)] {
        track.upsert(time, [x, 0., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4, 10]).unwrap();
    let knots = super::super::graph_sampling::knots(track, 0., 40., 15.).collect::<Vec<_>>();
    assert!(knots.windows(2).all(|p| p[0] < p[1]));
    assert!(knots.contains(&17.5));
    assert!(knots.contains(&17.5f64.next_down()));
    let data = super::super::curve_comparison::data::load(
        &state(&Engine::new(project).unwrap()),
        &target(),
        Locale::En,
    )
    .unwrap();
    assert_eq!(data.spatial_frames[1], (2, 17.5));
    assert_eq!(data.spatial_frames[2], (3, 17.5));
}

#[test]
fn malformed_spatial_graph_is_logged_once_per_revision_then_recovers_at_default_level() {
    const NAME:&str="panels::timeline_graph::spatial_tests::malformed_spatial_graph_is_logged_once_per_revision_then_recovers_at_default_level";
    const CHILD: &str = "MOTION_SPATIAL_GRAPH_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let good = state(&fixture());
        let mut bad = good.clone();
        bad["revision"] = 1.into();
        bad["project"]["layers"][0]["transform"]["position"]["spatial_animation"]["keys"][1]
            .as_object_mut()
            .unwrap()
            .remove("fixed");
        let mut cache = Cache::default();
        for _ in 0..90 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        let mut restored = good.clone();
        restored["revision"] = 2.into();
        assert!(cache.load(&restored, &target()).is_some());
        bad["revision"] = 3.into();
        for _ in 0..90 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        assert_eq!(good["project"], restored["project"]);
        crate::logging::flush();
        let log = root.join("motion-studio.log");
        assert_eq!(
            std::fs::read_to_string(&log)
                .unwrap()
                .lines()
                .filter(|l| l.contains("ERROR") && l.contains("timeline graph failed"))
                .count(),
            2
        );
        // Missing revisions must neither reuse decoded tracks nor flood errors.
        bad.as_object_mut().unwrap().remove("revision");
        restored.as_object_mut().unwrap().remove("revision");
        for _ in 0..90 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        let a = cache.load(&restored, &target()).unwrap();
        let b = cache.load(&restored, &target()).unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
        for _ in 0..90 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_SPATIAL_GRAPH_ERROR_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, &root)
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
    let errors = file
        .lines()
        .filter(|l| l.contains("ERROR") && l.contains("timeline graph failed"))
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 4);
    assert_eq!(
        errors
            .iter()
            .filter(|l| l.contains("revision=None"))
            .count(),
        2
    );
    for error in errors {
        assert!(
            error.contains("object=7") && error.contains("phase=decode") && error.contains("fixed"),
            "{error}"
        );
    }
    assert!(String::from_utf8_lossy(&output.stderr).contains("timeline graph failed"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("timeline graph failed"));
}
