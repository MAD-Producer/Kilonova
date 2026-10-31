use motion_model::vector::{path_ops, SampledPath, SampledTrimPaths, TrimMode};
use motion_model::{serialized_bytes, Curve, CurveShape, CurveSpace, Ease, Keyframe, Track};
use std::{collections::BTreeMap, process::Command, sync::Mutex};

struct Capture(Mutex<Vec<(log::Level, String)>>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0
                .lock()
                .unwrap()
                .push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}
static LOG: Capture = Capture(Mutex::new(Vec::new()));

#[test]
fn default_level_observes_real_model_failures_and_preserves_results() {
    const CHILD: &str = "MOTION_MODEL_OBSERVATION_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "default_level_observes_real_model_failures_and_preserves_results",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    log::set_logger(&LOG).unwrap();
    log::set_max_level(log::LevelFilter::Info);

    let healthy = Track::constant(3.0_f32);
    for _ in 0..120 {
        assert_eq!(healthy.sample(5.0).unwrap(), 3.0);
        assert_eq!(healthy.sample(f64::NAN).unwrap(), 3.0); // Documented static fallback.
        assert_eq!(serialized_bytes(&[1, 2, 3]), 7);
    }
    assert!(LOG.0.lock().unwrap().is_empty());

    // Both finite endpoints and this overshooting curve pass model validation.
    // The resulting f64 interpolation exceeds the f32 storage range.
    let mut track = Track::constant(0.0_f32);
    track.keys = vec![
        Keyframe {
            auto_bezier: false,
            frame: 0,
            value: 0.0,
            ease: Ease::Linear,
            spatial: None,
            curve: Some(Curve {
                space: CurveSpace::Progress,
                shape: CurveShape::Quadratic {
                    control: [0.5, 8.0],
                    start: 0.0,
                    end: 1.0,
                },
            }),
        },
        Keyframe {
            auto_bezier: false,
            frame: 10,
            value: f32::MAX,
            ease: Ease::Linear,
            curve: None,
            spatial: None,
        },
    ];
    track.validate(11).unwrap();
    for i in 0..120 {
        assert!(track.sample(5.0).unwrap().is_infinite());
        // Other properties sampled in the same frame must not reset evidence.
        assert_eq!(healthy.sample(5.0).unwrap(), 3.0);
        let messages = LOG.0.lock().unwrap();
        assert_eq!(messages.len(), if i < 59 { 1 } else { 2 });
    }
    assert_eq!(track.sample(10.0).unwrap(), f32::MAX);
    assert!(track.sample(5.0).unwrap().is_infinite());
    assert_eq!(LOG.0.lock().unwrap().len(), 2); // Process-lifetime suppression.

    // A genuine serde_json restriction, not a mocked successful return.
    let invalid_map = BTreeMap::from([(vec![1_u8, 2], true)]);
    assert!(serde_json::to_vec(&invalid_map)
        .unwrap_err()
        .to_string()
        .contains("key must be a string"));
    for i in 0..120 {
        assert_eq!(serialized_bytes(&invalid_map), usize::MAX);
        assert_eq!(serialized_bytes(&[1, 2, 3]), 7);
        assert_eq!(LOG.0.lock().unwrap().len(), if i < 59 { 3 } else { 4 });
    }
    let trim = SampledTrimPaths {
        start: 40.0,
        end: 60.0,
        offset: 0.0,
        mode: TrimMode::Simultaneously,
    };
    let path = SampledPath {
        closed: false,
        nodes: vec![
            [f32::MAX, 0.0, 0.0, 0.0, f32::MAX, 0.0],
            [f32::MAX, 0.0, f32::MAX, 0.0, 0.0, 0.0],
        ],
    };
    // Public path operations also accept callers' SampledPath values directly.
    // Finite coordinates/tangents can produce an unrepresentable cut point.
    for i in 0..120 {
        let cut = path_ops::trim(std::slice::from_ref(&path), &trim).unwrap();
        assert_eq!(cut.len(), 1);
        assert!(cut[0].nodes.iter().flatten().any(|v| !v.is_finite()));
        let normal = SampledPath {
            closed: false,
            nodes: vec![[0.0; 6], [100.0, 0.0, 0.0, 0.0, 0.0, 0.0]],
        };
        let cut = path_ops::trim(&[normal], &trim).unwrap();
        assert!(cut[0].nodes.iter().flatten().all(|v| v.is_finite()));
        assert_eq!(LOG.0.lock().unwrap().len(), if i < 59 { 5 } else { 6 });
    }
    let messages = LOG.0.lock().unwrap();
    for (offset, site) in [
        (0, "track.sample"),
        (2, "serialized_bytes"),
        (4, "path_ops.cut"),
    ] {
        assert_eq!(messages[offset].0, log::Level::Warn);
        assert_eq!(messages[offset + 1].0, log::Level::Error);
        assert!(messages[offset]
            .1
            .contains(&format!("site={site} lifetime_count=1 ")));
        assert!(messages[offset + 1]
            .1
            .contains(&format!("site={site} lifetime_count=60 ")));
    }
    assert!(messages[0]
        .1
        .contains("frame=5 first_key=Some(0) last_key=Some(10) keys=2"));
    assert!(messages[2]
        .1
        .contains("fallback=usize::MAX written_bytes=1"));
    assert!(messages[2].1.contains("reason=key must be a string"));
    assert!(messages[4].1.contains("input_nodes=2 output_nodes=2"));
    assert!(messages[0].1.contains("reason=non_finite_sample"));
    assert!(messages[4].1.contains("reason=non_finite_cut"));
}
