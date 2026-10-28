use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn admitted_depth_keeps_exact_source_pixels_after_gpu_eviction_and_file_removal() {
    let root = tempfile::tempdir().unwrap();
    let original = calibrated_source_session(root.path());
    let mut project = original.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.; 2];
    project.camera.created = true;
    project.camera.position.set_at(0, [16., 16., -120.]).unwrap();
    project.camera.target.set_at(0, [16., 16., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let mut blue = motion_core::Layer::solid(
        2, "Immutable image depth oracle", [80.; 2], [16., 16., 0.], [0., 0., 1., 1.],
    );
    blue.three_d = true;
    project.layers.push(blue);
    drop(original);
    let id = motion_host::session::open(root.path().into(), project, Arc::new(NoMedia)).unwrap();
    struct Close(i64);
    impl Drop for Close {
        fn drop(&mut self) { motion_host::session::close(self.0); }
    }
    let _close = Close(id);
    let (worker, seen, _) = backend(false);
    motion_host::session::with_session(id, |s| {
        s.configure_depth(worker)?;
        s.request_image_depth(7, geometry(), model())?;
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id, |s| {
        wait_done(s, 1);
        s.poll_depth()?;
        let command = calibration_command(s);
        s.apply_command_batch(vec![command])?;
        s.request_image_depth(7, geometry(), model())?;
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id, |s| {
        wait_done(s, 2);
        s.poll_depth()?;
        Ok(())
    }).unwrap();
    let capture = || {
        let result = motion_host::ops::export::capture(id).unwrap();
        image::open(result["path"].as_str().unwrap()).unwrap().to_rgba8()
    };
    let expected = capture();
    assert_eq!(expected.get_pixel(16, 10).0, [0, 0, 255, 255]);
    let near = expected.get_pixel(16, 22).0;
    assert!(near[0] > 40 && near[1] < 10 && near[2] < 10 && near[3] == 255);
    // A GPU cache miss must not combine a previously admitted depth field with
    // newly decoded pixels, even when its registered asset metadata is unchanged.
    image::RgbaImage::from_pixel(3, 3, image::Rgba([0, 255, 0, 255]))
        .save(root.path().join("assets/input.png")).unwrap();
    motion_host::session::with_session(id, |s| {
        assert!(s.graphics.is_none(), "each headless capture creates a fresh renderer");
        Ok(())
    }).unwrap();
    assert_eq!(capture(), expected, "the paired source must survive GPU eviction and source replacement");
    std::fs::remove_file(root.path().join("assets/input.png")).unwrap();
    motion_host::session::with_session(id, |s| {
        assert!(s.graphics.is_none(), "each headless capture creates a fresh renderer");
        Ok(())
    }).unwrap();
    assert_eq!(capture(), expected, "admitted source ownership must not reopen the source file");
    motion_host::session::with_session(id, |s| {
        assert_eq!(s.depth_status()?.unwrap().counts.completed, 2, "rendering never reruns inference");
        s.cancel_depth()?;
        Ok(())
    }).unwrap();
    assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
}
