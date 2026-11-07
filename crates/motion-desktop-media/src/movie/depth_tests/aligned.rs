//! Real PNG/queued MP4 consumers of an independently reconstructed transition.
use super::*;
use motion_host::depth::{
    alignment::Limits,
    transition::{Activity, Policy},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct TransitionDepth(Arc<AtomicUsize>);
impl Backend for TransitionDepth {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let raw = OwnedDepth.infer(input, cancel)?;
        if input.key().model.processing != "settled" {
            return Ok(raw);
        }
        Ok(Estimate {
            resource: DepthResource::new(
                raw.resource.descriptor().clone(),
                raw.resource
                    .values()
                    .iter()
                    .map(|v| (*v - 3.) / 2.)
                    .collect(),
                None,
                None,
            )?,
            provider: "owned_settled_output_cpu".into(),
            costs: Costs::default(),
        })
    }
}

#[test]
#[cfg(all(feature = "ffmpeg", feature = "gpu-tests"))]
fn aligned_transition_png_and_durable_mp4_keep_raw_metric_occlusion_without_live_owners_or_inference(
) {
    let original = tempfile::tempdir().unwrap();
    let outputs = tempfile::tempdir().unwrap();
    let precision = motion_effects::image_contract::ChannelPrecision::Unorm8;
    let initial = session(original.path(), precision);
    let project = initial.engine.project().clone();
    drop(initial);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut session =
        Session::new(project.clone(), original.path().into(), crate::platform()).unwrap();
    session
        .configure_depth(TransitionDepth(calls.clone()))
        .unwrap();
    infer(&mut session);
    let raw_png = outputs.path().join("raw-metric.png");
    frame::FrameFreezePlan::capture(&session, Some(raw_png.clone()), &ExportControl::default())
        .unwrap()
        .export(&ExportControl::default())
        .unwrap();
    let expected = image::open(&raw_png).unwrap().into_rgba8().into_raw();
    occlusion(&expected, true);
    let mut settled = project.layers[0]
        .depth_calibration
        .as_ref()
        .unwrap()
        .model()
        .clone();
    settled.processing = "settled".into();
    session
        .begin_image_depth_transition(
            settled,
            Policy {
                idle_delay: Duration::from_millis(100),
                minimum_interactive_residence: Duration::from_millis(300),
            },
            Limits {
                minimum_fit_pairs: 3,
                minimum_validation_pairs: 3,
                minimum_pair_fraction: 1.,
                minimum_confidence: None,
                minimum_reference_stddev: 1e-8,
                scale_bounds: [0.1, 10.],
                maximum_normalized_rms: 1e-8,
                maximum_normalized_error: 1e-8,
            },
        )
        .unwrap();
    let ready = |session: &mut Session| {
        let until = Instant::now() + Duration::from_secs(5);
        while !session.poll_image_depth_transition().unwrap() {
            assert!(
                Instant::now() < until,
                "controlled output transition did not finish"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    assert!(session
        .request_image_depth_transition(Activity::Interactive, Duration::ZERO)
        .unwrap()
        .is_some());
    ready(&mut session);
    assert!(session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(100))
        .unwrap()
        .is_none());
    assert!(session
        .request_image_depth_transition(Activity::Idle, Duration::from_millis(400))
        .unwrap()
        .is_some());
    ready(&mut session);
    let png = outputs.path().join("aligned.png");
    let plan =
        frame::FrameFreezePlan::capture(&session, Some(png.clone()), &ExportControl::default())
            .unwrap();
    let frozen = FrozenMovie::from_session(&session).unwrap();
    let persistent = frozen
        .prepare_persistent(outputs.path(), &ExportControl::default())
        .unwrap()
        .publish()
        .unwrap();
    drop(frozen);
    drop(session);
    original.close().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    plan.export(&ExportControl::default()).unwrap();
    let actual = image::open(&png).unwrap().into_rgba8().into_raw();
    occlusion(&actual, true);
    assert_eq!(
        actual, expected,
        "aligned PNG must match the independent raw metric oracle"
    );
    let restored = FrozenMovie::restore_persistent(&persistent, &ExportControl::default()).unwrap();
    let movie = outputs.path().join("aligned.mp4");
    assert!(
        restored
            .export(&movie, &ExportControl::default(), |_| {})
            .unwrap()
            .bytes
            > 0
    );
    let probe = crate::av::probe(&movie, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), 31);
    let mut decoder =
        crate::av::Decoder::open(&movie, probe.asset, probe.timestamps.clone()).unwrap();
    let mut last = Vec::new();
    let mut maximum = 0;
    for pts in probe.timestamps {
        let decoded = decoder.frame(pts, &|| Ok(())).unwrap();
        let pixels = decoded.rgba().unwrap();
        occlusion(&pixels, false);
        let difference: u64 = pixels
            .iter()
            .zip(&expected)
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(
            difference < 32 * 32 * 4 * 8,
            "encoded mean absolute error exceeds eight: {difference}"
        );
        maximum = maximum.max(difference);
        last = pixels.into_owned();
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "export cannot infer or retry after the depth owner exits"
    );
    retain(
        "aligned-png",
        &png,
        &expected,
        &actual,
        serde_json::json!({"exactNativePixels":true,"sessionClosed":true,"originalRemoved":true,"inferences":3}),
    );
    retain(
        "aligned-durable-mp4",
        &movie,
        &expected,
        &last,
        serde_json::json!({"decodedFrames":31,"maxAbsoluteDifferenceSum":maximum,"restoredFromDisk":true,"inferences":3}),
    );
}
