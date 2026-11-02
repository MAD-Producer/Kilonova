use super::*;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};
use motion_core::{Ease, Engine, KeyframeTarget, Layer, Project};
use serde_json::json;
fn fixture(reverse: bool) -> Engine {
    let mut p = Project::new(640, 360, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "Clock", [64.; 2], [0.; 3], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: 2,
        out_frame: 110,
        offset_frame: 2,
    });
    for frame in [3, 13] {
        layer
            .transform
            .position
            .upsert(frame, [frame as f32, 20., 0.], Ease::Linear)
            .unwrap();
    }
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            if reverse { 40. } else { 0. },
            0.,
        )
        .unwrap(),
        remap: None,
    });
    p.layers.push(layer);
    Engine::new(p).unwrap()
}
fn state(engine: &Engine) -> Value {
    json!({"root":"mapped-graph","composition":motion_core::MAIN_COMPOSITION,"revision":engine.revision(),"document_generation":7,"project":engine.project(),"timeline_layers":engine.project().timeline_layers(0.).unwrap()})
}
fn target() -> KeyframeTarget {
    KeyframeTarget::Property {
        object: 7,
        property: Property::Position,
        axis: None,
    }
}
#[test]
fn value_and_physical_speed_use_the_same_clock_and_fit_keeps_fractional_identity() {
    for reverse in [false, true] {
        let engine = fixture(reverse);
        let s = state(&engine);
        let mut cache = Cache::default();
        let samples = cache.load(&s, &target()).unwrap();
        let clock = cache.clock;
        let time = engine.project().layers[0]
            .property_composition_frame(8.)
            .unwrap();
        assert_eq!(
            samples
                .composition_value(time, 2, clock, 0, false, 30.)
                .unwrap(),
            8.
        );
        assert_eq!(
            samples
                .composition_value(time, 2, clock, 0, true, 30.)
                .unwrap(),
            if reverse { -20. } else { 20. }
        );
        let again = cache.load(&s, &target()).unwrap();
        assert!(Arc::ptr_eq(&samples, &again));
        assert_eq!(cache.clock, clock);
        let mut panels = Panels::default();
        panels
            .selected_keys
            .apply_addressed(
                &s,
                [super::super::key_selection::LocalKey {
                    target: target(),
                    frame: 3,
                }
                .into()],
                super::super::key_selection::Mode::Replace,
            )
            .unwrap();
        let source = &s["timeline_layers"][0]["properties"]["position"];
        let (all, chosen) = fit_intervals(&panels, &s, &target(), source, true);
        let c = engine.project().layers[0]
            .property_composition_frame(3.)
            .unwrap();
        assert_eq!(chosen, Some([c, c]));
        let bounds = samples
            .fit_bounds(all, 2., clock, &[0], false, 30., 1., [500., 400.], 0.35)
            .unwrap();
        assert!(bounds.x.low < all[0] && bounds.x.high > all[1]);
        assert!(bounds.y.low <= 3. && bounds.y.high >= 13.);
    }
}
#[test]
fn reversed_hold_knots_keep_order_and_match_the_existing_model_on_both_sides() {
    let clock = LayerClock::new(
        TimeStretch::new(3, 2).unwrap(),
        TimeDirection::Reverse,
        40.,
        0.,
    )
    .unwrap();
    let mut track = Track::constant(0f32);
    track.upsert(0, 0., Ease::Hold).unwrap();
    track.upsert(10, 10., Ease::Linear).unwrap();
    track.validate_local().unwrap();
    let boundary = clock.projected_composition_frame(10., 2).unwrap();
    let samples = Samples::Scalar(track.clone());
    let knots: Vec<_> = mapped_knots(&track, 20., 45., 2., clock).unwrap().collect();
    assert!(knots.windows(2).all(|p| p[0] < p[1]));
    assert!(
        knots.contains(&boundary)
            && knots.contains(&boundary.next_up())
            && knots.contains(&boundary.next_down())
    );
    assert_eq!(
        samples
            .composition_value(boundary, 2, clock, 0, false, 30.)
            .unwrap(),
        10.
    );
    // Track::sample currently narrows interval progress to f32. Preserve its
    // actual edge semantics in the graph; changing pixel semantics belongs to
    // the separately recorded model precision work, not this UI projection.
    for time in [
        boundary.next_down(),
        boundary,
        boundary.next_up(),
        boundary + 0.00001,
    ] {
        assert_eq!(
            samples
                .composition_value(time, 2, clock, 0, false, 30.)
                .unwrap(),
            track.sample(clock.local_frame(time, 2).unwrap()).unwrap()
        );
    }
    assert_eq!(
        samples
            .composition_value(boundary.next_up(), 2, clock, 0, false, 30.)
            .unwrap(),
        10.,
        "keep the existing rounded edge visible in the regression contract"
    );
    assert_eq!(
        samples
            .composition_value(boundary + 0.00001, 2, clock, 0, false, 30.)
            .unwrap(),
        0.
    );
    let bounds = samples
        .fit_bounds(
            [20., 45.],
            2.,
            clock,
            &[0],
            false,
            30.,
            1.,
            [500., 400.],
            0.35,
        )
        .unwrap();
    assert!(bounds.y.low <= 0. && bounds.y.high >= 10.);
}
#[test]
fn malformed_clock_uses_existing_bounded_graph_fault_and_recovers_at_default_level() {
    const NAME:&str="panels::timeline_graph::time_tests::malformed_clock_uses_existing_bounded_graph_fault_and_recovers_at_default_level";
    const CHILD: &str = "MOTION_TIMED_GRAPH_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logging = crate::logging::init(root.clone()).unwrap();
        let good = state(&fixture(false));
        let mut bad = good.clone();
        bad["project"]["layers"][0]["time_mapping"]["clock"]["stretch"]["denominator"] = json!(0);
        bad["revision"] = json!(1);
        let mut cache = Cache::default();
        for _ in 0..120 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        let mut restored = good.clone();
        restored["revision"] = json!(2);
        assert!(cache.load(&restored, &target()).is_some());
        bad["revision"] = json!(3);
        for _ in 0..120 {
            assert!(cache.load(&bad, &target()).is_none());
        }
        crate::logging::flush();
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        let lines: Vec<_> = file
            .lines()
            .filter(|l| l.contains("timeline graph failed"))
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.contains("ERROR")
            && l.contains("object=7")
            && l.contains("phase=decode")
            && l.contains("duration ratio")));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_TIMED_GRAPH_LOG_ARTIFACTS")
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
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .matches("timeline graph failed")
            .count(),
        2
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("timeline graph failed"));
}
