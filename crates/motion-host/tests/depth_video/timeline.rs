//! Actual native pixels across indexed source intervals, including a controlled
//! pending worker. No second decoder supplies colors after depth admission.
use super::*;

#[test]
fn indexed_spill_keeps_150_samples_after_the_128_sample_live_cache_evicts_the_first() {
    let mut f = fixture(Mode::ManyFrames);
    for frame in 0..150 {
        f.session.frame = f64::from(frame);
        request(&mut f.session).unwrap();
        done(&mut f.session, frame as u64 + 1);
        f.session.poll_depth().unwrap().unwrap();
        let command = author_indexed_sample(&mut f.session, f64::from((10 + frame * 40) % 256));
        f.session.apply_command_batch(vec![command]).unwrap();
        f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    // Establish all admissions at one final document revision. Authoring a new
    // calibration intentionally invalidates previous revision ownership.
    for frame in 0..150 {
        f.session.frame = f64::from(frame);
        request(&mut f.session).unwrap();
        done(&mut f.session, frame as u64 + 151);
        let result = f.session.poll_depth().unwrap().unwrap();
        assert_eq!(result.resource.descriptor().source.pts.ticks, 10_000 + i64::from(frame) * 40_000);
        f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let before = f.session.depth_status().unwrap().unwrap();
    assert_eq!((before.retained_samples, before.stored_samples), (128, 150));
    assert_eq!((before.stored_bytes, before.counts.restored), (150 * 24, 0));
    // Frame zero is no longer in the live FIFO. A new worker admission must
    // recover its exact scalar/RGB pair without invoking inference again.
    f.session.frame = 0.;
    request(&mut f.session).unwrap();
    done(&mut f.session, 301);
    let restored = f.session.poll_depth().unwrap().unwrap();
    assert_eq!(restored.resource.values(), &[10., 11., 12., 13., 14., 15.]);
    assert!(f.session.bound_depth_calibration(1).unwrap().is_some());
    assert!(f.seen.try_recv().is_err());
    let after = f.session.depth_status().unwrap().unwrap();
    assert_eq!((after.retained_samples, after.stored_samples, after.stored_bytes, after.counts.restored), (128, 150, 3600, 1));
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 301);
}

#[test]
fn indexed_spill_restores_verified_source_without_running_backend_again_and_resets_with_document() {
    let mut authored = fixture(Mode::Rgba);
    for (index, frame, low) in [(1, 0., 10.), (2, 1., 40.)] {
        authored.session.frame = frame;
        request(&mut authored.session).unwrap();
        done(&mut authored.session, index);
        authored.session.poll_depth().unwrap().unwrap();
        let command = author_indexed_sample(&mut authored.session, low);
        authored.session.apply_command_batch(vec![command]).unwrap();
    }
    let mut f = fixture(Mode::Rgba);
    f.session.replace_project(motion_core::Engine::new(authored.session.engine.snapshot()).unwrap(), f.dir.path().into()).unwrap();
    for (count, frame) in [(1, 0.), (2, 1.)] {
        f.session.frame = frame;
        request(&mut f.session).unwrap();
        done(&mut f.session, count);
        f.session.poll_depth().unwrap().unwrap();
        f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.stored_samples, status.stored_bytes, status.storage_rejections), (2, 48, 0));
    // An explicit request evicts this interval's live result. It must decode
    // the original again but restore its stored scalar planes without inference.
    f.session.frame = 0.;
    request(&mut f.session).unwrap();
    done(&mut f.session, 3);
    let restored = f.session.poll_depth().unwrap().unwrap();
    assert_eq!(restored.resource.descriptor().source.pts.ticks, 10_000);
    assert!(f.seen.try_recv().is_err(), "restore must not run the backend");
    assert_eq!(f.platform.opens.load(Ordering::SeqCst), 3);
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.stored_samples, status.stored_bytes, status.counts.restored), (2, 48, 1));
    assert!(f.session.bound_depth_calibration(1).unwrap().is_some());
    let project = f.session.engine.snapshot();
    f.session.replace_project(motion_core::Engine::new(project).unwrap(), f.dir.path().into()).unwrap();
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.stored_samples, status.stored_bytes), (0, 0));
    request(&mut f.session).unwrap();
    done(&mut f.session, 4);
    f.session.poll_depth().unwrap().unwrap();
    f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(f.session.depth_status().unwrap().unwrap().counts.restored, 1);
    f.session.cancel_depth().unwrap();
    assert_eq!(f.session.depth_status().unwrap().unwrap().stored_samples, 0);
}

#[test]
fn indexed_spill_io_failure_is_reported_at_default_info_without_publishing_a_success() {
    let mut authored = fixture(Mode::Rgba);
    request(&mut authored.session).unwrap();
    done(&mut authored.session, 1);
    authored.session.poll_depth().unwrap().unwrap();
    let command = author_indexed_sample(&mut authored.session, 10.);
    authored.session.apply_command_batch(vec![command]).unwrap();
    let mut f = fixture(Mode::BlockBackend);
    f.session.replace_project(motion_core::Engine::new(authored.session.engine.snapshot()).unwrap(), f.dir.path().into()).unwrap();
    let request_id = request(&mut f.session).unwrap();
    f.seen.recv_timeout(Duration::from_secs(5)).unwrap();
    // Decoding has completed and the backend is controlled. Removing the test
    // root now forces a real tempfile creation failure in the storage phase.
    std::fs::remove_dir_all(f.dir.path()).unwrap();
    f.release.send(()).unwrap();
    done(&mut f.session, 1);
    assert!(f.session.poll_depth().is_err());
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!((status.counts.failed, status.stored_samples, status.retained_samples), (1, 0, 0));
    let lines = LOG.0.lock().unwrap().clone();
    assert!(lines.iter().any(|line| line.contains(&format!("start #{request_id} depth_spill object=1 asset=74"))), "{lines:?}");
    assert!(lines.iter().any(|line| line.contains(&format!("request={request_id} operation=depth_infer asset=74"))
        && line.contains("stage=archive_write") && line.contains("cause=")), "{lines:?}");
}

struct FourthGate {
    echo: Echo,
    calls: usize,
    entered: mpsc::Sender<()>,
    release: Option<mpsc::Receiver<()>>,
}
impl Backend for FourthGate {
    fn infer(
        &mut self,
        input: &Input,
        cancel: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        self.calls += 1;
        if self.calls == 4 {
            self.entered.send(()).unwrap();
            self.release
                .take()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
        self.echo.infer(input, cancel)
    }
}

#[test]
fn indexed_pending_result_cannot_cross_into_another_authored_source_interval() {
    let mut authored = fixture(Mode::Rgba);
    for (index, frame, low) in [(1, 0., 10.), (2, 1., 40.)] {
        authored.session.frame = frame;
        request(&mut authored.session).unwrap();
        done(&mut authored.session, index);
        authored.session.poll_depth().unwrap().unwrap();
        let command = author_indexed_sample(&mut authored.session, low);
        authored.session.apply_command_batch(vec![command]).unwrap();
    }
    let mut f = fixture(Mode::BlockBackend);
    f.session
        .replace_project(
            motion_core::Engine::new(authored.session.engine.snapshot()).unwrap(),
            f.dir.path().into(),
        )
        .unwrap();
    let old_request = request(&mut f.session).unwrap();
    assert_eq!(
        f.seen
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .key
            .source
            .pts
            .ticks,
        10_000
    );
    f.session.frame = 1.;
    assert!(f.session.poll_depth().unwrap().is_none());
    let new_request = request(&mut f.session).unwrap();
    assert!(new_request > old_request);
    f.release.send(()).unwrap();
    assert_eq!(
        f.seen
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .key
            .source
            .pts
            .ticks,
        40_000
    );
    done(&mut f.session, 2);
    assert_eq!(
        f.session
            .poll_depth()
            .unwrap()
            .unwrap()
            .resource
            .descriptor()
            .source
            .pts
            .ticks,
        40_000
    );
    assert!(f.session.bound_depth_calibration(1).unwrap().is_some());
    let status = f.session.depth_status().unwrap().unwrap();
    assert_eq!(
        (
            status.counts.completed,
            status.counts.canceled,
            status.retained_samples
        ),
        (1, 1, 1)
    );
    assert_eq!(status.request, Some(new_request));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn indexed_native_playback_stages_once_reuses_vfr_samples_and_keeps_color_depth_pairs_after_source_removal(
) {
    use motion_effects::image_contract::ChannelPrecision::{Float16, Float32, Unorm16, Unorm8};
    for precision in [Unorm8, Unorm16, Float16, Float32] {
        let f = fixture(Mode::Rgba);
        let mut project = f.session.engine.snapshot();
        project.working_precision = precision;
        project.layers[0].three_d = true;
        project.layers[0].size = [80.; 2];
        project.layers[0]
            .transform
            .position
            .set_at(0, [16., 16., 0.])
            .unwrap();
        project.camera.created = true;
        project
            .camera
            .position
            .set_at(0, [16., 16., -120.])
            .unwrap();
        project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
        project.camera.fov.set_at(0, 60.).unwrap();
        let mut plane = Layer::solid(
            2,
            "Independent temporal occluder",
            [80.; 2],
            [16., 16., 0.],
            [0., 0., 1., 1.],
        );
        plane.three_d = true;
        project.layers.push(plane);
        let id =
            motion_host::session::open(f.dir.path().into(), project, f.platform.clone()).unwrap();
        struct Close(i64);
        impl Drop for Close {
            fn drop(&mut self) {
                motion_host::session::close(self.0);
            }
        }
        let _close = Close(id);
        let (sender, seen) = mpsc::channel();
        let (entered, waiting) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        motion_host::session::with_session(id, |s| {
            s.configure_depth(FourthGate {
                echo: Echo {
                    seen: sender,
                    release: None,
                },
                calls: 0,
                entered,
                release: Some(blocked),
            })?;
            for (index, frame, low) in [(1, 0., 10.), (2, 1., 40.), (3, 3., 110.)] {
                s.frame = frame;
                request(s).unwrap();
                done(s, index);
                s.poll_depth()?.unwrap();
                let command = author_indexed_sample(s, low);
                s.apply_command_batch(vec![command])?;
            }
            assert!(motion_host::depth_snapshot::DepthSnapshot::capture(s)
                .unwrap_err()
                .to_string()
                .contains("indexed video depth export"));
            s.frame = 0.;
            Ok(())
        })
        .unwrap();
        for pts in [10_000, 40_000, 110_000] {
            assert_eq!(
                seen.recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .key
                    .source
                    .pts
                    .ticks,
                pts
            );
        }
        // This render request stages automatically from the explicit saved
        // track. Repeated polling inside the same VFR interval cannot supersede
        // the blocked ticket or flood the worker with another decode.
        assert!(motion_host::ops::export::capture(id)
            .unwrap_err()
            .to_string()
            .contains("pending"));
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        for frame in [0.1, 0.5, 0.] {
            motion_host::session::with_session(id, |s| {
                s.frame = frame;
                Ok(())
            })
            .unwrap();
            assert!(motion_host::ops::export::capture(id)
                .unwrap_err()
                .to_string()
                .contains("pending"));
            motion_host::session::with_session(id, |s| {
                let status = s.depth_status()?.unwrap();
                assert_eq!(status.counts.submitted, 4);
                assert_eq!(status.counts.canceled, 0);
                assert_eq!(status.in_flight.unwrap().phase, "backend");
                Ok(())
            })
            .unwrap();
        }
        release.send(()).unwrap();
        assert_eq!(
            seen.recv_timeout(Duration::from_secs(5))
                .unwrap()
                .key
                .source
                .pts
                .ticks,
            10_000
        );
        motion_host::session::with_session(id, |s| {
            done(s, 4);
            s.poll_depth()?.unwrap();
            Ok(())
        })
        .unwrap();
        let mut accepted = Vec::new();
        for (frame, count, low) in [(0., 4, 10u8), (1., 5, 40), (3., 6, 110)] {
            motion_host::session::with_session(id, |s| {
                s.frame = frame;
                Ok(())
            })
            .unwrap();
            if count != 4 {
                assert!(motion_host::ops::export::capture(id)
                    .unwrap_err()
                    .to_string()
                    .contains("pending"));
                motion_host::session::with_session(id, |s| {
                    done(s, count);
                    s.poll_depth()?.unwrap();
                    Ok(())
                })
                .unwrap();
            }
            let capture = motion_host::ops::export::capture(id).unwrap();
            let pixels = image::open(capture["path"].as_str().unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(pixels.dimensions(), (32, 32));
            assert_eq!(
                pixels.get_pixel(16, 10).0,
                [0, 0, 255, 255],
                "far geometry must remain behind the independent plane"
            );
            let near = pixels.get_pixel(16, 22).0;
            assert!(
                near[0] >= low
                    && near[0] <= low + 5
                    && near[1] <= 3
                    && near[2] <= 4
                    && near[3] == 255,
                "frame={frame} must pair its own original color with depth: {near:?}"
            );
            if let Some(root) = std::env::var_os("MOTION_VIDEO_DEPTH_TIMELINE_ARTIFACTS") {
                let root = std::path::PathBuf::from(root);
                std::fs::create_dir_all(&root).unwrap();
                pixels
                    .save(root.join(format!("{precision:?}-frame-{frame}.png")))
                    .unwrap();
            }
            accepted.push(pixels);
        }
        assert_ne!(accepted[0], accepted[1]);
        assert_ne!(accepted[1], accepted[2]);
        assert_eq!(f.platform.opens.load(Ordering::SeqCst), 6);
        std::fs::remove_file(f.dir.path().join("assets/source.mp4")).unwrap();
        for (frame, index) in [(0., 0), (0.5, 0), (1., 1), (2., 1), (3., 2), (0., 0)] {
            motion_host::session::with_session(id, |s| {
                s.frame = frame;
                Ok(())
            })
            .unwrap();
            let capture = motion_host::ops::export::capture(id).unwrap();
            assert_eq!(
                image::open(capture["path"].as_str().unwrap())
                    .unwrap()
                    .to_rgba8(),
                accepted[index],
                "frame={frame}"
            );
        }
        assert_eq!(
            f.platform.opens.load(Ordering::SeqCst),
            6,
            "cached playback must not reopen a source decoder"
        );
        motion_host::session::with_session(id, |s| {
            let status = s.depth_status()?.unwrap();
            assert_eq!(
                (status.retained_samples, status.retained_image_bytes),
                (3, 72)
            );
            let mut value = serde_json::to_value(s.engine.snapshot()).unwrap();
            value["layers"][0]["video_depth_calibration"]
                .as_array_mut()
                .unwrap()
                .remove(1);
            let project: Project = serde_json::from_value(value).unwrap();
            s.replace_project(motion_core::Engine::new(project)?, f.dir.path().into())?;
            s.frame = 1.;
            Ok(())
        })
        .unwrap();
        let previous = std::fs::read(f.dir.path().join("exports/frame-000001.png")).unwrap();
        let error = motion_host::ops::export::capture(id).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no indexed calibration interval"),
            "{error}"
        );
        assert_eq!(
            std::fs::read(f.dir.path().join("exports/frame-000001.png")).unwrap(),
            previous,
            "an uncovered source time must refuse output without overwriting accepted pixels"
        );
    }
}
