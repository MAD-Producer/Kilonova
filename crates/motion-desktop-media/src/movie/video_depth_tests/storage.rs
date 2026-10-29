//! Real-codec temporal ownership beyond the live FIFO, including independent
//! persistent restoration after every original owner and source has gone away.
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counted(Arc<AtomicUsize>);
impl Backend for Counted {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.0.fetch_add(1, Ordering::SeqCst);
        RedDepth.infer(input, cancel)
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn real_codec_150_sample_snapshot_survives_eviction_close_and_persistent_restore() {
    let original = tempfile::tempdir().unwrap();
    let queue = tempfile::tempdir().unwrap();
    let (project, pts, decoded) = source_frames(original.path(), 150);
    let calls = Arc::new(AtomicUsize::new(0));
    let id =
        motion_host::session::open(original.path().into(), project, crate::platform()).unwrap();
    struct Close(i64);
    impl Drop for Close {
        fn drop(&mut self) {
            motion_host::session::close(self.0);
        }
    }
    let _close = Close(id);
    motion_host::session::with_session(id, |s| {
        s.configure_depth(Counted(calls.clone()))?;
        for index in 0..150 {
            author(s, index, pts[index], &decoded[index]);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 150);
    let mut expected = Vec::new();
    for index in 0..150 {
        motion_host::session::with_session(id, |s| {
            let result = stage(s, index);
            assert_eq!(
                result.resource.descriptor().source.pts.ticks,
                pts[index] as i64
            );
            assert_eq!(
                result.resource.values(),
                decoded[index]
                    .chunks_exact(4)
                    .map(|pixel| f32::from(pixel[0]))
                    .collect::<Vec<_>>()
            );
            Ok(())
        })
        .unwrap();
        let capture = motion_host::ops::export::capture(id).unwrap();
        expected.push(
            image::open(capture["path"].as_str().unwrap())
                .unwrap()
                .into_rgba8()
                .into_raw(),
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 300);
    let (plan, png_plan) = motion_host::session::with_session(id, |s| {
        let status = s.depth_status()?.unwrap();
        assert_eq!(
            (
                status.retained_samples,
                status.stored_samples,
                status.stored_bytes
            ),
            (128, 150, 150 * 32 * 32 * 4)
        );
        s.frame = 0.;
        // The first interval has been evicted. Capturing plans must only retain
        // file metadata and must not read, decode, infer or re-admit that interval.
        let plan = FrozenMovie::capture_plan(s, &ExportControl::default()).unwrap();
        let png = frame::FrameFreezePlan::capture(
            s,
            Some(queue.path().join("first.png")),
            &ExportControl::default(),
        )
        .unwrap();
        assert_eq!(s.depth_status()?.unwrap().retained_samples, 128);
        Ok((plan, png))
    })
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 300);
    let frozen = plan.freeze(&ExportControl::default()).unwrap();
    let prepared = frozen
        .prepare_persistent(queue.path(), &ExportControl::default())
        .unwrap();
    let persisted = prepared.publish().unwrap();
    motion_host::session::close(id);
    drop(frozen);
    original.close().unwrap();
    png_plan.export(&ExportControl::default()).unwrap();
    assert_eq!(
        image::open(queue.path().join("first.png"))
            .unwrap()
            .into_rgba8()
            .into_raw(),
        expected[0]
    );
    let restored = FrozenMovie::restore_persistent(&persisted, &ExportControl::default()).unwrap();
    let output = queue.path().join("temporal-150.mp4");
    restored
        .export(&output, &ExportControl::default(), |_| {})
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 300);
    let probe = crate::av::probe(&output, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), 150);
    let mut reader =
        crate::av::Decoder::open(&output, probe.asset, probe.timestamps.clone()).unwrap();
    let mut differences = Vec::new();
    for (index, time) in probe.timestamps.iter().enumerate() {
        let decoded = reader.frame(*time, &|| Ok(())).unwrap();
        let pixels = decoded.rgba().unwrap();
        let far = &pixels[(10 * 32 + 16) * 4..][..4];
        let near = &pixels[(22 * 32 + 16) * 4..][..4];
        assert!(
            far[0] < 20 && far[1] < 20 && far[2] > 220,
            "{index}: far={far:?}"
        );
        assert!(
            near[0] > 30 + (index % 3) as u8 * 40 && near[1] < 20 && near[2] < 20,
            "{index}: near={near:?}"
        );
        let difference: u64 = pixels
            .iter()
            .zip(&expected[index])
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        assert!(
            difference < 32 * 32 * 4 * 8,
            "frame={index}: difference={difference}"
        );
        differences.push(difference);
    }
    drop(reader);
    if let Some(root) = std::env::var_os("MOTION_REAL_VIDEO_DEPTH_ARTIFACTS") {
        let root = PathBuf::from(root).join("temporal-150");
        fs::create_dir_all(&root).unwrap();
        fs::copy(&output, root.join("temporal-150.mp4")).unwrap();
        fs::copy(
            persisted.join("depth.snapshot"),
            root.join("depth.snapshot"),
        )
        .unwrap();
        for index in [0, 1, 2, 127, 128, 149] {
            image::save_buffer(
                root.join(format!("native-{index:03}.png")),
                &expected[index],
                32,
                32,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        fs::write(root.join("comparison.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "frames":150,"sourcePts":pts,"absoluteDifferences":differences,"backendCalls":calls.load(Ordering::SeqCst),
            "liveSamples":128,"storedSamples":150,"sessionAndOriginalGone":true,"persistentRestore":true,
            "precision":"Unorm8","scope":"Five-second beyond-cache ownership proof, not sustained duration or trained-model quality"
        })).unwrap()).unwrap();
    }
}
