use super::*;
use motion_core::{AxisTracks, Ease, Keyframe, Track, Tween};

#[derive(Clone, Copy, PartialEq)]
struct PartialVector([f32; 3]);
impl Tween for PartialVector {
    fn mix(self, other: Self, t: f32) -> Self {
        Self(self.0.mix(other.0, t))
    }
    fn finite(self) -> bool {
        self.0.finite()
    }
    fn bounded(self, bound: f32) -> bool {
        self.0.bounded(bound)
    }
    fn add(self, other: Self) -> Self {
        Self(self.0.add(other.0))
    }
    fn components(self) -> Option<[f32; 3]> {
        (self.0[0] >= 0.).then_some(self.0)
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        Some(Self(value))
    }
}

fn axes() -> Box<AxisTracks> {
    Box::new(AxisTracks {
        x: Track::constant(1.),
        y: Track::constant(2.),
        z: Track::constant(3.),
    })
}

#[test]
fn actual_dimension_conversion_failures_reach_native_reply_and_default_error_sinks() {
    const CHILD: &str = "MOTION_TRACK_DIMENSIONS_LOG_CHILD";
    const NAME: &str = "engine::failure::model_edit_tests::track_dimensions::actual_dimension_conversion_failures_reach_native_reply_and_default_error_sinks";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut causes = Vec::new();
        for (request, case) in (731..737).zip(0..6) {
            let error = if case == 5 {
                let mut track = Track::constant(PartialVector([1., 2., 3.]));
                track.keys.push(Keyframe {
                    frame: 7,
                    value: PartialVector([-1., 2., 3.]),
                    ease: Ease::Linear,
                    curve: None,
                    spatial: None,
                });
                let error = track.separate().unwrap_err();
                assert!(error.to_string().contains("key index=0 frame=7"));
                assert_eq!(track.value.0, [1., 2., 3.]);
                assert_eq!(track.keys.len(), 1);
                assert_eq!(track.keys[0].value.0, [-1., 2., 3.]);
                assert!(track.axes.is_none());
                error
            } else if case == 0 {
                let mut track = Track::constant(1.);
                track.axes = Some(axes());
                let before = format!("{track:?}");
                let error = track.separate().unwrap_err();
                assert_eq!(format!("{track:?}"), before);
                error
            } else {
                let mut track = Track::constant([1., 2., 3.]);
                if case == 1 {
                    track.keys = [20, -10]
                        .map(|frame| Keyframe {
                            frame,
                            value: [4., 5., 6.],
                            ease: Ease::InOut,
                            curve: None,
                            spatial: None,
                        })
                        .into();
                } else {
                    track.axes = Some(axes());
                    track.axes.as_mut().unwrap().y.axes = Some(axes());
                }
                let before = format!("{track:?}");
                let error = match case {
                    3 => track.upsert(8, [4., 5., 6.], Ease::Out).unwrap_err(),
                    4 => track.set_at(8, [4., 5., 6.]).unwrap_err(),
                    _ => track.separate().unwrap_err(),
                };
                assert_eq!(format!("{track:?}"), before);
                error
            };
            assert!(matches!(&error, motion_core::Error::Invalid(_)));
            let original = error.to_string();
            let native = motion_host::Error::from(error);
            assert!(matches!(
                &native,
                motion_host::Error::Model(motion_core::Error::Invalid(_))
            ));
            let (reply, receive) = channel::<Result<(), String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_dimension_conversion",
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
        assert_eq!(healthy.sample(0.), [1., 2., 3.]);
        healthy.merge_dimensions().unwrap();
        assert!(healthy.axes.is_none());
        assert_eq!(healthy.value, [1., 2., 3.]);
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
    assert_eq!(causes.len(), 6);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (731..737).zip(causes) {
        let marker = format!("error #{request} test_dimension_conversion ");
        for text in [&file, &stderr] {
            let lines: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(lines.len(), 1, "{text}");
            assert!(lines[0].contains(&cause));
        }
        assert!(!stdout.contains(&cause));
    }
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            6,
            "{text}"
        );
    }
    assert!(!stderr.contains("logging initialized"));
    for marker in [
        "test_dimension_conversion",
        "logging initialized",
        "INFO",
        "ERROR",
    ] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
