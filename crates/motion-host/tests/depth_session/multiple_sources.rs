use super::*;
use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};

struct Permitted {
    echo: Echo,
    permit: mpsc::Receiver<()>,
}
impl Backend for Permitted {
    fn infer(
        &mut self,
        input: &Input,
        cancellation: &Cancellation,
    ) -> motion_host::depth::Result<Estimate> {
        let result = self.echo.infer(input, cancellation)?;
        self.permit
            .recv_timeout(Duration::from_secs(5))
            .expect("test must release the controlled result");
        Ok(result)
    }
}
struct Close(i64);
impl Drop for Close {
    fn drop(&mut self) {
        motion_host::session::close(self.0);
    }
}
fn request(id: i64, asset: u64, seen: &mpsc::Receiver<Seen>) {
    motion_host::session::with_session(id, |s| {
        s.request_image_depth(asset, geometry(), model())?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        seen.recv_timeout(Duration::from_secs(5))
            .unwrap()
            .key
            .source
            .asset,
        asset
    );
}
fn release(id: i64, count: u64, permit: &mpsc::Sender<()>) {
    permit.send(()).unwrap();
    motion_host::session::with_session(id, |s| {
        wait_done(s, count);
        assert!(s.poll_depth()?.is_some());
        Ok(())
    })
    .unwrap();
}
fn capture(id: i64) -> (std::path::PathBuf, image::RgbaImage) {
    let result = motion_host::ops::export::capture(id).unwrap();
    let path = std::path::PathBuf::from(result["path"].as_str().unwrap());
    let pixels = image::open(&path).unwrap().to_rgba8();
    (path, pixels)
}
fn pending(id: i64) {
    let cause = motion_host::ops::export::capture(id)
        .unwrap_err()
        .to_string();
    assert!(cause.contains("pending"), "{cause}");
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn two_image_depth_results_render_together_and_pending_replacement_seek_and_cancel_cannot_reuse_stale_sources(
) {
    let root = tempfile::tempdir().unwrap();
    let original = session(root.path());
    let mut project = original.engine.project().clone();
    let mut green = image::RgbaImage::new(3, 3);
    for (x, y, pixel) in green.enumerate_pixels_mut() {
        *pixel = image::Rgba([100 - ((y * 3 + x + 1) * 10) as u8, 200, 3, 255]);
    }
    green.save(root.path().join("assets/green.png")).unwrap();
    project.assets.push(Asset {
        id: 8,
        path: "assets/green.png".into(),
        width: 3,
        height: 3,
    });
    for (object, asset) in [(1, 7), (2, 8)] {
        let mut layer =
            motion_core::Layer::solid(object, "Owned source", [80.; 2], [16., 16., 0.], [1.; 4]);
        layer.content = motion_core::Content::Image { asset };
        layer.three_d = true;
        project.layers.push(layer);
    }
    let mut blue = motion_core::Layer::solid(
        3,
        "Physical depth plane",
        [80.; 2],
        [16., 16., 0.],
        [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [16., 16., -120.])
        .unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let id = motion_host::session::open(root.path().into(), project, Arc::new(NoMedia)).unwrap();
    let _close = Close(id);
    let (echo, seen, _) = backend(false);
    let (permit, receiver) = mpsc::channel();
    motion_host::session::with_session(id, |s| {
        s.configure_depth(Permitted {
            echo,
            permit: receiver,
        })
    })
    .unwrap();
    // Ordinary commands author both independent sources. Each commit invalidates
    // old live results; saved calibration does not bypass current admission.
    for (count, object, asset) in [(1, 1, 7), (2, 2, 8)] {
        request(id, asset, &seen);
        release(id, count, &permit);
        motion_host::session::with_session(id, |s| {
            let command = s.depth_calibration_command(
                object,
                [
                    DepthAnchor {
                        value: 10.,
                        camera_z_meters: 8.,
                    },
                    DepthAnchor {
                        value: 90.,
                        camera_z_meters: 4.,
                    },
                ],
                CalibrationCurve::CameraZ,
                CalibrationExtrapolation::Reject,
                20.,
            )?;
            s.apply_command_batch(vec![command])?;
            let status = s.depth_status()?.unwrap();
            assert_eq!((status.retained_samples, status.retained_bytes), (0, 0));
            Ok(())
        })
        .unwrap();
    }
    request(id, 7, &seen);
    release(id, 3, &permit);
    pending(id);
    assert!(!root.path().join("exports/frame-000000.png").exists());
    request(id, 8, &seen);
    release(id, 4, &permit);
    motion_host::session::with_session(id, |s| {
        let status = s.depth_status()?.unwrap();
        assert_eq!(
            (
                status.retained_samples,
                status.retained_bytes,
                status.retention_rejections
            ),
            (2, 72, 0)
        );
        assert!(s.bound_depth_calibration(1)?.is_some());
        assert!(s.bound_depth_calibration(2)?.is_some());
        Ok(())
    })
    .unwrap();
    let (path, pixels) = capture(id);
    assert!(
        pixels.get_pixel(16, 10).0[1] > 180,
        "near green source must occlude the red source and blue plane"
    );
    let near = pixels.get_pixel(16, 22).0;
    assert!(
        near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255,
        "near red source must occlude the green source and blue plane: {near:?}"
    );
    let file = std::fs::read(&path).unwrap();
    // Superseding this exact source removes its old admission immediately;
    // the unrelated source remains retained but cannot complete the whole scene.
    request(id, 8, &seen);
    motion_host::session::with_session(id, |s| {
        let status = s.depth_status()?.unwrap();
        assert_eq!((status.retained_samples, status.retained_bytes), (1, 36));
        assert!(!status.installed);
        assert!(status.in_flight.is_some());
        Ok(())
    })
    .unwrap();
    pending(id);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        file,
        "pending replacement cannot overwrite the previous export"
    );
    release(id, 5, &permit);
    assert_eq!(capture(id).1, pixels);
    motion_host::session::with_session(id, |s| {
        s.cancel_depth()?;
        let status = s.depth_status()?.unwrap();
        assert_eq!((status.retained_samples, status.retained_bytes), (0, 0));
        Ok(())
    })
    .unwrap();
    pending(id);
    assert_eq!(std::fs::read(&path).unwrap(), file);
    // Reverse request order after cancellation; asset order must not decide depth.
    request(id, 8, &seen);
    release(id, 6, &permit);
    request(id, 7, &seen);
    release(id, 7, &permit);
    assert_eq!(capture(id).1, pixels);
    motion_host::session::with_session(id, |s| {
        s.frame = 1.;
        s.sample()?;
        let status = s.depth_status()?.unwrap();
        assert_eq!((status.retained_samples, status.retained_bytes, status.retained_image_bytes), (2, 72, 72));
        assert!(!status.installed, "clock changes still invalidate the last worker ticket");
        assert_eq!(status.counts.completed, 7, "clock changes do not rerun static inference");
        Ok(())
    })
    .unwrap();
    assert_eq!(capture(id).1, pixels);
    let retained_file = std::fs::read(root.path().join("exports/frame-000001.png")).unwrap();
    motion_host::session::with_session(id, |s| {
        s.cancel_depth()?;
        let status = s.depth_status()?.unwrap();
        assert_eq!((status.retained_samples, status.retained_bytes), (0, 0));
        Ok(())
    })
    .unwrap();
    pending(id);
    assert_eq!(std::fs::read(root.path().join("exports/frame-000001.png")).unwrap(), retained_file,
        "explicit cancellation cannot overwrite the accepted playback output");
    request(id, 7, &seen);
    release(id, 8, &permit);
    pending(id);
    request(id, 8, &seen);
    release(id, 9, &permit);
    assert_eq!(capture(id).1, pixels);
}
