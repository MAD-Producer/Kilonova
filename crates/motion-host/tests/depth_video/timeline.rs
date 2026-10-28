//! Actual native pixels across indexed source intervals, including a controlled
//! pending worker. No second decoder supplies colors after depth admission.
use super::*;

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
