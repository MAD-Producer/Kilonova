use super::*;
use motion_core::{AxisTracks, Ease, Track, Tween};

#[derive(Clone, Copy, Debug, PartialEq)]
struct IntegralVector([f32; 3]);
impl Tween for IntegralVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(std::array::from_fn(|i| {
            (self.0[i] * (1. - t) + other.0[i] * t).round()
        }))
    }
    fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
    fn bounded(self, bound: f32) -> bool {
        self.0.iter().all(|v| v.is_finite() && v.abs() <= bound)
    }
    fn add(self, other: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + other.0[i]))
    }
    fn components(self) -> Option<[f32; 3]> {
        Some(self.0)
    }
    fn from_components(v: [f32; 3]) -> Option<Self> {
        v.iter()
            .all(|n| n.is_finite() && n.fract() == 0.)
            .then_some(Self(v))
    }
}

#[test]
fn actual_sampling_refusals_preserve_native_causes_and_default_reply_sinks() {
    const CHILD: &str = "MOTION_ANIMATION_SAMPLING_LOG_CHILD";
    const NAME: &str = "engine::failure::model_edit_tests::animation_sampling::actual_sampling_refusals_preserve_native_causes_and_default_reply_sinks";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut causes = Vec::new();
        for (request, case) in (741..744).zip(0..3) {
            let error = if case == 0 {
                let mut track = Track::constant(IntegralVector([0.; 3]));
                track
                    .upsert(0, IntegralVector([0.; 3]), Ease::Linear)
                    .unwrap();
                track
                    .upsert(10, IntegralVector([10.; 3]), Ease::Linear)
                    .unwrap();
                track.separate().unwrap();
                track.validate(20).unwrap();
                let before = track.clone();
                let error = track.sample(5.5).unwrap_err();
                assert_eq!(track, before);
                assert_eq!(track.sample(5.).unwrap(), IntegralVector([5.; 3]));
                assert!(error.to_string().contains("phase=reconstruct frame=5.5"));
                error
            } else {
                let mut track = Track::constant([1., 2., 3.]);
                track.separate().unwrap();
                if case == 1 {
                    track.keys.push(motion_core::Keyframe {
                        auto_bezier: false,
                        frame: 0,
                        value: [4.; 3],
                        ease: Ease::Linear,
                        curve: None,
                        spatial: None,
                    });
                } else {
                    track.axes.as_mut().unwrap().y.axes = Some(Box::new(AxisTracks {
                        x: Track::constant(1.),
                        y: Track::constant(2.),
                        z: Track::constant(3.),
                    }));
                }
                let before = track.clone();
                let error = track.sample(5.5).unwrap_err();
                assert_eq!(track, before);
                assert!(error.to_string().contains("phase=storage frame=5.5"));
                error
            };
            assert!(matches!(&error, motion_core::Error::Invalid(_)));
            let original = error.to_string();
            let native = motion_host::Error::from(error);
            // The existing transparent host variant owns the model error itself;
            // source() forwards the inner source, which Invalid intentionally lacks.
            let motion_host::Error::Model(source) = &native else {
                panic!("expected native model variant, got {native:?}");
            };
            assert!(matches!(source, motion_core::Error::Invalid(_)));
            assert_eq!(source.to_string(), original);
            let (reply, receive) = channel::<Result<(), String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_animation_sampling",
                started: Instant::now(),
            }));
            respond(reply, Err(native));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        let mut healthy = Track::constant([1., 2., 3.]);
        healthy.separate().unwrap();
        assert_eq!(healthy.sample(5.5).unwrap(), [1., 2., 3.]);
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 3);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (741..744).zip(causes) {
        let marker = format!("error #{request} test_animation_sampling ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1, "{text}");
            assert!(lines[0].contains(&cause), "{}", lines[0]);
        }
        assert!(!stdout.contains(&cause), "{stdout}");
    }
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            3,
            "{text}"
        );
    }
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "test_animation_sampling",
        "logging initialized",
        "INFO",
        "ERROR",
    ] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
