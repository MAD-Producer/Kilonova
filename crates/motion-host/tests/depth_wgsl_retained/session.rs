//! Actual backend-to-Session-to-frozen-render control, without qualification claims.
use super::*;
use motion_core::{Asset, Command, Content, Layer, Project};
use motion_host::{Platform, Session, depth_snapshot::DepthSnapshot};
use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};

struct ImagesOnly;
impl Platform for ImagesOnly {
    fn name(&self) -> &'static str {
        "owned-retained-depth-session-control"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        Err("image-only control has no video probe".into())
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn motion_host::VideoDecoder>> {
        Err("image-only control has no video decoder".into())
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        Err("image-only control has no audio decoder".into())
    }
    fn media_capabilities(
        &self,
        _: Option<&motion_host::VideoQuery>,
    ) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        Err("image-only control has no native window surface".into())
    }
}

fn installed(session: &mut Session) -> Arc<Estimate> {
    wait(|| {
        let status = session.depth_status().unwrap().unwrap();
        assert_eq!(status.counts.failed, 0, "{status:?}");
        session.poll_depth().unwrap()
    })
}

#[test]
#[ignore = "requires explicit installed model, pinned GPU worker, Python, real RGB fixture and renderer"]
fn actual_retained_session_installs_and_freezes_depth_after_worker_exit() {
    session_and_frozen_render(false);
}

#[test]
#[ignore = "requires explicit installed model, pinned GPU worker, Python, real RGB fixture and renderer"]
fn actual_host_remap_session_installs_and_freezes_depth_after_worker_exit() {
    session_and_frozen_render(true);
}

fn session_and_frozen_render(rust_remap: bool) {
    let path = std::env::var_os("MOTION_DEPTH_RETAINED_TEST_FIXTURE")
        .expect("explicit real fixture required");
    let fixture: Fixture = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let out = fixture.evidence.parent().unwrap();
    let _logger = logging::init(out.join("logs")).unwrap();
    assert!(fixture.scratch.is_absolute() && !fixture.scratch.exists());
    fs::create_dir(&fixture.scratch).unwrap();
    let root = out.join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let sample = input(&fixture, 0, 0);
    let image = image::RgbImage::from_raw(640, 480, sample.rgb().to_vec()).unwrap();
    image.save(root.join("assets/source.png")).unwrap();
    assert_eq!(
        image::open(root.join("assets/source.png"))
            .unwrap()
            .to_rgb8(),
        image
    );

    let mut project = Project::new(96, 72, 30, 60).unwrap();
    project.assets.push(Asset {
        id: 75,
        path: "assets/source.png".into(),
        width: 640,
        height: 480,
    });
    let mut photo = Layer::solid(1, "Owned depth source", [80., 60.], [48., 36., 0.], [1.; 4]);
    photo.content = Content::Image { asset: 75 };
    photo.three_d = true;
    let mut plane = Layer::solid(
        2,
        "Authored metric occlusion control",
        [80., 60.],
        [48., 36., 0.],
        [0., 0., 1., 1.],
    );
    plane.three_d = true;
    project.layers.extend([photo, plane]);
    project.camera.created = true;
    project
        .camera
        .position
        .set_at(0, [48., 36., -120.])
        .unwrap();
    project.camera.target.set_at(0, [48., 36., 0.]).unwrap();
    project.camera.fov.set_at(0, 60.).unwrap();
    let config = Config {
        python: fixture.python.clone(),
        worker: fixture.worker.clone(),
        program: fixture.program.clone(),
        scratch: fixture.scratch.clone(),
        hardware: Hardware::Vulkan,
    };
    let backend = if rust_remap {
        Wgsl::new_retained_rust_remap(config)
    } else {
        Wgsl::new_retained(config)
    }
    .unwrap();
    let mut session = Session::new(project, root.clone(), Arc::new(ImagesOnly)).unwrap();
    session
        .configure_depth_cached_in(
            backend,
            out.join("spill"),
            motion_host::depth::cache::Config {
                directory: out.join("cache"),
                byte_limit: 16 << 20,
                entry_limit: 8,
            },
        )
        .unwrap();
    assert!(
        session.depth_qualified_bindings().unwrap().is_none(),
        "this is explicit unqualified integration"
    );
    session
        .request_image_depth(
            75,
            sample.key().geometry.clone(),
            sample.key().model.clone(),
        )
        .unwrap();
    let initial = installed(&mut session);
    let output_sha = sha(&initial
        .resource
        .values()
        .iter()
        .flat_map(|n| n.to_le_bytes())
        .collect::<Vec<_>>());
    assert_eq!(output_sha, fixture.expected_output_sha256[0]);
    // Cache reuse requires the same source/document revision. Authoring the
    // calibration below deliberately changes that revision and must miss.
    session
        .request_image_depth(
            75,
            sample.key().geometry.clone(),
            sample.key().model.clone(),
        )
        .unwrap();
    let cached = installed(&mut session);
    assert_eq!(cached.costs.inference, Duration::ZERO);
    assert_eq!(
        sha(&cached
            .resource
            .values()
            .iter()
            .flat_map(|n| n.to_le_bytes())
            .collect::<Vec<_>>()),
        output_sha
    );
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.cache_misses, 1);
    assert_eq!(status.counts.cache_hits, 1);
    let service = service_root(&fixture.scratch).unwrap();
    let native = wait(|| receipt(&service.join("service/receipt.json")));
    assert_eq!(native["context_preparations"], 1);
    assert_eq!(native["completed_inputs"], 1);
    let low = initial
        .resource
        .values()
        .iter()
        .copied()
        .reduce(f32::min)
        .unwrap();
    let high = initial
        .resource
        .values()
        .iter()
        .copied()
        .reduce(f32::max)
        .unwrap();
    assert!(low.is_finite() && high.is_finite() && high > low);
    // These authored distances exercise metric composition; they are not a
    // measurement of this scene's real-world distances or model accuracy.
    let command = session
        .depth_calibration_command(
            1,
            [
                DepthAnchor {
                    value: f64::from(low),
                    camera_z_meters: 8.,
                },
                DepthAnchor {
                    value: f64::from(high),
                    camera_z_meters: 4.,
                },
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.,
        )
        .unwrap();
    assert!(matches!(command, Command::DepthCalibration { .. }));
    session.apply_command_batch(vec![command]).unwrap();
    let pending = DepthSnapshot::capture(&session).unwrap_err().to_string();
    assert!(pending.contains("pending"), "{pending}");
    session
        .request_image_depth(
            75,
            sample.key().geometry.clone(),
            sample.key().model.clone(),
        )
        .unwrap();
    let current = installed(&mut session);
    assert_eq!(
        sha(&current
            .resource
            .values()
            .iter()
            .flat_map(|n| n.to_le_bytes())
            .collect::<Vec<_>>()),
        output_sha
    );
    assert!(session.bound_depth_calibration(1).unwrap().is_some());
    let revised = session.depth_status().unwrap().unwrap();
    assert_eq!(
        revised.counts.cache_misses, 2,
        "calibration advances the source revision"
    );
    assert_eq!(revised.counts.cache_hits, 1);
    let revised_native = wait(|| receipt(&service.join("service/receipt.json")));
    assert_eq!(revised_native["context_preparations"], 1);
    assert_eq!(revised_native["completed_inputs"], 2);
    let snapshot = DepthSnapshot::capture(&session).unwrap();
    assert_eq!(snapshot.layer_count(), 1);
    let project = session.engine.project().clone();
    let mut encoded = Vec::new();
    snapshot.write_to(&mut encoded, &|| Ok(())).unwrap();
    fs::write(out.join("frozen.depth"), &encoded).unwrap();
    let restored = DepthSnapshot::read_from(&mut &encoded[..], &|| Ok(())).unwrap();
    restored.validate_project(&project).unwrap();
    drop(current);
    drop(cached);
    drop(initial);
    drop(session);
    assert!(
        !service.exists(),
        "Session close must reap the owned worker and remove its workspace"
    );

    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    renderer.configure_assets(&project, &root).unwrap();
    let target = renderer
        .sdr_capture_target(96, 72, motion_render::OutputTransform::SDR_CLIP)
        .unwrap();
    let mut scene = motion_render::Scene::new(&project).unwrap();
    let verified = restored.verify_sources(&root, &|| Ok(())).unwrap();
    let mut frames = Vec::new();
    for frame in [0., 1., 30.] {
        scene
            .sample(&project, frame, None, &motion_core::ExpressionEvaluator)
            .unwrap();
        verified.attach(&mut scene).unwrap();
        assert!(
            renderer
                .prepare_scene_assets(
                    &scene,
                    motion_render::image_resources::Resolution::Full,
                    false
                )
                .unwrap()
        );
        let pixels = renderer.capture_sdr(&scene, &target).unwrap().0.pixels;
        image::RgbaImage::from_raw(96, 72, pixels.clone())
            .unwrap()
            .save(out.join(format!("frozen-{frame:02}.png")))
            .unwrap();
        frames.push(pixels);
    }
    assert!(
        frames.windows(2).all(|pair| pair[0] == pair[1]),
        "frozen scalar ownership must survive seek and worker exit"
    );
    let blue = frames[0]
        .chunks_exact(4)
        .filter(|p| *p == [0, 0, 255, 255])
        .count();
    assert!(
        blue > 16 && blue < 96 * 72 - 16,
        "composition must contain both the metric plane and the image: {blue}"
    );
    let mut flat = project.clone();
    flat.layers[0].depth_calibration = None;
    let mut flat_scene = motion_render::Scene::new(&flat).unwrap();
    flat_scene
        .sample(&flat, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    assert!(
        renderer
            .prepare_scene_assets(
                &flat_scene,
                motion_render::image_resources::Resolution::Full,
                false
            )
            .unwrap()
    );
    let flat_pixels = renderer.capture_sdr(&flat_scene, &target).unwrap().0.pixels;
    let different = flat_pixels
        .chunks_exact(4)
        .zip(frames[0].chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        different > 16,
        "actual calibrated depth must change visible composition: {different}"
    );

    let bytes = fs::read(root.join("assets/source.png")).unwrap();
    image::RgbImage::from_pixel(640, 480, image::Rgb([201, 2, 3]))
        .save(root.join("assets/source.png"))
        .unwrap();
    let changed = restored
        .verify_sources(&root, &|| Ok(()))
        .err()
        .unwrap()
        .to_string();
    assert!(
        changed.contains("source_id=75") && changed.contains("fingerprint changed"),
        "{changed}"
    );
    fs::write(root.join("assets/source.png"), bytes).unwrap();
    restored.verify_sources(&root, &|| Ok(())).unwrap();
    fs::write(&fixture.evidence,serde_json::to_vec_pretty(&json!({
        "status":"actual_retained_session_frozen_render_passed","qualified":false,
        "rust_remap":rust_remap,
        "limits":["Authored calibration distances are not real-world accuracy evidence","No automatic tier policy or native access-violation fix"],
        "output_sha256":output_sha,"native":native,"cache_hits":status.counts.cache_hits,"cache_misses":status.counts.cache_misses,
        "after_calibration_native":revised_native,"after_calibration_cache_misses":revised.counts.cache_misses,
        "snapshot_layers":restored.layer_count(),"frozen_sha256":sha(&encoded),"render_sha256":sha(&frames[0]),
        "frame_count":frames.len(),"blue_pixels":blue,"pixels_changed_from_flat":different,
        "original_changed_source_cause":changed,"owned_worker_workspace_removed":!service.exists()
    })).unwrap()).unwrap();
}
