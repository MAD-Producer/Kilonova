#![cfg(feature = "ai-depth")]
#[path = "depth_session/multiple_sources.rs"]
mod multiple_sources;

use motion_core::{Asset, Command, Engine, Project};
use motion_host::{
    depth::{Backend, Cancellation, Costs, Estimate, Input, Key},
    Platform, Session,
};
use motion_model::depth::{
    DepthDescriptor, DepthGeometry, DepthMeasure, DepthModel, DepthResource, NearDirection,
};
use std::{
    path::Path,
    sync::{mpsc, Arc, Mutex, Once},
    time::{Duration, Instant},
};

struct LogCapture(Mutex<Vec<(log::Level, String)>>);
static LOG: LogCapture = LogCapture(Mutex::new(Vec::new()));
impl log::Log for LogCapture {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let mut records = self.0.lock().unwrap();
            if records.len() == 128 {
                records.remove(0);
            }
            records.push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}
fn init_log() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        log::set_logger(&LOG).unwrap();
        log::set_max_level(log::LevelFilter::Info);
    });
}

struct NoMedia;
impl Platform for NoMedia {
    fn name(&self) -> &'static str {
        "depth-session-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("no video probing")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn motion_host::VideoDecoder>> {
        panic!("no video decoding")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("no audio decoding")
    }
    fn media_capabilities(
        &self,
        _: Option<&motion_host::VideoQuery>,
    ) -> motion_media::Result<serde_json::Value> {
        Ok(serde_json::json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<motion_host::SurfaceTarget> {
        panic!("no GPU surface")
    }
}

struct Seen {
    key: Key,
    size: [u32; 2],
    rgb: Vec<u8>,
    thread: std::thread::ThreadId,
}
struct Echo {
    entered: mpsc::Sender<Seen>,
    release: Option<mpsc::Receiver<()>>,
}
impl Backend for Echo {
    fn infer(&mut self, input: &Input, _: &Cancellation) -> motion_host::depth::Result<Estimate> {
        self.entered
            .send(Seen {
                key: input.key().clone(),
                size: input.size(),
                rgb: input.rgb().to_vec(),
                thread: std::thread::current().id(),
            })
            .unwrap();
        if let Some(release) = self.release.take() {
            release.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        // Deliberately ignores cancellation to prove the installation guard.
        Ok(Estimate {
            resource: DepthResource::new(
                DepthDescriptor {
                    size: input.size(),
                    source: input.key().source.clone(),
                    geometry: input.key().geometry.clone(),
                    model: input.key().model.clone(),
                    measure: DepthMeasure::Relative {
                        near: NearDirection::Increasing,
                    },
                    scale: 1.,
                    offset: 0.,
                    valid_range: None,
                },
                input
                    .rgb()
                    .chunks_exact(3)
                    .map(|rgb| f32::from(rgb[0]))
                    .collect(),
                None,
                None,
            )?,
            provider: "controlled_cpu".into(),
            costs: Costs::default(),
        })
    }
}
fn backend(block: bool) -> (Echo, mpsc::Receiver<Seen>, mpsc::Sender<()>) {
    let (entered, seen) = mpsc::channel();
    let (release, receiver) = mpsc::channel();
    (
        Echo {
            entered,
            release: block.then_some(receiver),
        },
        seen,
        release,
    )
}
fn model() -> DepthModel {
    DepthModel {
        id: "owned-depth-test".into(),
        revision: "r1".into(),
        sha256: [7; 32],
        processing: "rgb-test-v1".into(),
    }
}
fn geometry() -> DepthGeometry {
    DepthGeometry {
        source_size: [3, 3],
        crop: [0, 0, 3, 3],
        quarter_turns: 0,
        mirror_x: false,
    }
}
fn session(root: &Path) -> Session {
    std::fs::create_dir_all(root.join("assets")).unwrap();
    let mut image = image::RgbaImage::new(3, 3);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([((y * 3 + x + 1) * 10) as u8, 2, 3, 255]);
    }
    image.save(root.join("assets/input.png")).unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    project.assets.push(Asset {
        id: 7,
        path: "assets/input.png".into(),
        width: 3,
        height: 3,
    });
    Session::new(project, root.into(), Arc::new(NoMedia)).unwrap()
}
fn wait_done(session: &mut Session, count: u64) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let status = session.depth_status().unwrap().unwrap();
        if status.counts.completed + status.counts.failed + status.counts.canceled >= count {
            return;
        }
        assert!(Instant::now() < until, "depth completion timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn calibrated_source_session(root: &Path) -> Session {
    let mut session = session(root);
    let mut layer = motion_core::Layer::solid(1, "Owned source", [3., 3.], [16., 16., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 7 };
    session
        .apply_command_batch(vec![Command::Add { layer }])
        .unwrap();
    session
}

fn calibration_command(session: &mut Session) -> Command {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    session
        .depth_calibration_command(
            1,
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
        )
        .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_png_capture_consumes_current_calibrated_geometry_and_refuses_stale_frames() {
    let root = tempfile::tempdir().unwrap();
    let original = calibrated_source_session(root.path());
    let mut project = original.engine.project().clone();
    project.layers[0].three_d = true;
    project.layers[0].size = [80.;2];
    project.camera.created = true;
    project.camera.position.set_at(0,[16.,16.,-120.]).unwrap();
    project.camera.target.set_at(0,[16.,16.,0.]).unwrap();
    project.camera.fov.set_at(0,60.).unwrap();
    let mut blue = motion_core::Layer::solid(2,"Depth occlusion oracle",[80.;2],[16.,16.,0.],[0.,0.,1.,1.]);
    blue.three_d = true;
    project.layers.push(blue);
    let id = motion_host::session::open(root.path().into(),project,Arc::new(NoMedia)).unwrap();
    struct Close(i64);
    impl Drop for Close { fn drop(&mut self) { motion_host::session::close(self.0); } }
    let _close = Close(id);
    let (worker,seen,_release) = backend(false);
    motion_host::session::with_session(id,|s| {
        s.configure_depth(worker)?;
        s.request_image_depth(7,geometry(),model())?;
        Ok(())
    }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id,|s| {
        wait_done(s,1);
        s.poll_depth()?;
        s.observing = true;
        let command = calibration_command(s);
        let Command::DepthCalibration { calibration:Some(calibration), .. } = &command else { panic!("calibration command required") };
        assert!(calibration.reference().is_some(),"3D authoring must capture the actual camera and plane");
        let reference = calibration.reference().unwrap();
        assert_eq!(reference.camera.eye,glam::Vec3::new(0.,0.,120.));
        assert_ne!(reference.camera.eye,s.scene.camera.eye,"the free observation camera must not become the saved source camera");
        s.apply_command_batch(vec![command])?;
        s.observing = false;
        Ok(())
    }).unwrap();
    assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
    assert!(!root.path().join("exports/frame-000000.png").exists(),"pending depth must not create a misleading flat export");
    motion_host::session::with_session(id,|s| { s.request_image_depth(7,geometry(),model())?; Ok(()) }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id,|s| { wait_done(s,2); s.poll_depth()?; Ok(()) }).unwrap();
    let capture = motion_host::ops::export::capture(id).unwrap();
    let pixels = image::open(capture["path"].as_str().unwrap()).unwrap().to_rgba8();
    assert_eq!(pixels.dimensions(),(32,32));
    assert_eq!(pixels.get_pixel(16,10).0,[0,0,255,255],"far calibrated photo must be occluded by actual geometry");
    let near = pixels.get_pixel(16,22).0;
    assert!(near[0]>40 && near[2]<10 && near[3]==255,"near calibrated photo must occlude the plane: {near:?}");
    motion_host::session::with_session(id,|s| { s.frame=1.; Ok(()) }).unwrap();
    assert!(motion_host::ops::export::capture(id).unwrap_err().to_string().contains("pending"));
    assert!(!root.path().join("exports/frame-000001.png").exists());
    motion_host::session::with_session(id,|s| { s.request_image_depth(7,geometry(),model())?; Ok(()) }).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    motion_host::session::with_session(id,|s| { wait_done(s,3); s.poll_depth()?; Ok(()) }).unwrap();
    let next = motion_host::ops::export::capture(id).unwrap();
    assert_eq!(image::open(next["path"].as_str().unwrap()).unwrap().to_rgba8(),pixels);
    let mut parameters = [0xa5;1024];
    let mut vertices = [0xa5;1024];
    let error = motion_host::ops::geometry::sample_geometry_into(id,1.,&mut parameters,&mut vertices).unwrap_err().to_string();
    assert!(error.contains("native calibrated depth"),"{error}");
    assert!(parameters.iter().chain(&vertices).all(|byte|*byte==0xa5));
    assert!(motion_host::ops::export::sample_into(id,1,&mut parameters).unwrap_err().to_string().contains("native calibrated depth"));
    assert!(parameters.iter().all(|byte|*byte==0xa5));
}

#[test]
fn actual_calibration_authoring_reopens_and_rebinds_without_reusing_live_revisions() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = calibrated_source_session(directory.path());
    let initial = session.engine.project().clone();
    let (engine, seen, _) = backend(false);
    session.configure_depth(engine).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 1);
    let previous = session.poll_depth().unwrap().unwrap();
    let command = calibration_command(&mut session);
    session.apply_command_batch(vec![command]).unwrap();
    let saved = session.engine.project().clone();
    assert!(saved.layers[0].depth_calibration.is_some());
    assert!(session.bound_depth_calibration(1).unwrap().is_none());
    assert!(session.depth_input_fingerprint().unwrap().is_none());
    session.engine.undo().unwrap();
    session.sample().unwrap();
    assert_eq!(session.engine.project(), &initial);
    session.engine.redo().unwrap();
    session.sample().unwrap();
    assert_eq!(session.engine.project(), &saved);
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 2);
    let fresh = session.poll_depth().unwrap().unwrap();
    let bound = session.bound_depth_calibration(1).unwrap().unwrap();
    assert_eq!(
        bound.descriptor().source.revision,
        session.engine.revision()
    );
    assert_ne!(
        bound.descriptor().source.revision,
        previous.resource.descriptor().source.revision
    );
    assert!(bound.bind(&previous.resource).is_err());
    assert_eq!(
        bound
            .bind(&fresh.resource)
            .unwrap()
            .camera_z_meters(0)
            .unwrap(),
        Some(8.)
    );
    assert_eq!(
        bound
            .bind(&fresh.resource)
            .unwrap()
            .camera_z_meters(8)
            .unwrap(),
        Some(4.)
    );
    motion_core::storage::save(directory.path(), session.engine.project()).unwrap();
    let mut reopened = Session::new(
        motion_core::storage::load(directory.path()).unwrap(),
        directory.path().into(),
        Arc::new(NoMedia),
    )
    .unwrap();
    assert_eq!(reopened.engine.project(), &saved);
    assert!(reopened.bound_depth_calibration(1).unwrap().is_none());
    let (engine, reopened_seen, _) = backend(false);
    reopened.configure_depth(engine).unwrap();
    reopened
        .request_image_depth(7, geometry(), model())
        .unwrap();
    reopened_seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut reopened, 1);
    let result = reopened.poll_depth().unwrap().unwrap();
    let binding = reopened.bound_depth_calibration(1).unwrap().unwrap();
    assert_eq!(
        binding.descriptor().source.revision,
        reopened.engine.revision()
    );
    assert_ne!(
        binding.descriptor().source.revision,
        bound.descriptor().source.revision
    );
    assert_eq!(
        binding
            .bind(&result.resource)
            .unwrap()
            .camera_z_meters(4)
            .unwrap(),
        Some(6.)
    );
    assert_eq!(reopened.engine.project(), &saved);
    assert!(reopened.graphics.is_none());
}

#[test]
fn saved_calibration_refuses_changed_pixels_and_processing_with_explicit_reauthoring_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = calibrated_source_session(directory.path());
    let (engine, seen, _) = backend(false);
    session.configure_depth(engine).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 1);
    let original = calibration_command(&mut session);
    session.apply_command_batch(vec![original.clone()]).unwrap();
    let before = session.engine.project().clone();
    let revision = session.engine.revision();
    let history = session.engine.history_diagnostics();
    let path = directory.path().join("assets/input.png");
    let mut pixels = image::open(&path).unwrap().to_rgba8();
    // The backend reads only R: changing G proves inference values alone cannot
    // identify the exact RGB that the worker actually prepared.
    pixels.get_pixel_mut(0, 0).0[1] += 1;
    pixels.save(path).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 2);
    let result = session.poll_depth().unwrap().unwrap();
    assert_eq!(
        result.resource.values(),
        &[10., 20., 30., 40., 50., 60., 70., 80., 90.]
    );
    let error = session.bound_depth_calibration(1).unwrap_err().to_string();
    assert!(
        error.contains("source=7") && error.contains("phase=rebind"),
        "{error}"
    );
    let error = session
        .apply_command_batch(vec![
            Command::Rename {
                object: 1,
                name: "Must roll back".into(),
            },
            original,
        ])
        .unwrap_err()
        .to_string();
    assert!(error.contains("actual source pixels"), "{error}");
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.history_diagnostics(), history);
    let replacement = calibration_command(&mut session);
    session.apply_command_batch(vec![replacement]).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 3);
    assert!(session.bound_depth_calibration(1).unwrap().is_some());
    let mut processing = model();
    processing.processing = "another-rgb-processing".into();
    session
        .request_image_depth(7, geometry(), processing)
        .unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 4);
    assert!(session.bound_depth_calibration(1).is_err());
    session.cancel_depth().unwrap();
    assert!(session.bound_depth_calibration(1).unwrap().is_none());
    session
        .apply_command_batch(vec![Command::DepthCalibration {
            object: 1,
            calibration: None,
        }])
        .unwrap();
    assert!(session.engine.project().layers[0]
        .depth_calibration
        .is_none());
}

#[test]
fn calibration_authoring_rejects_pending_canceled_and_changed_candidate_sources_atomically() {
    use motion_model::depth::{CalibrationCurve, CalibrationExtrapolation, DepthAnchor};
    let directory = tempfile::tempdir().unwrap();
    let mut session = calibrated_source_session(directory.path());
    let before = session.engine.project().clone();
    let (engine, seen, release) = backend(true);
    session.configure_depth(engine).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(session
        .depth_calibration_command(
            1,
            [
                DepthAnchor {
                    value: 10.,
                    camera_z_meters: 8.
                },
                DepthAnchor {
                    value: 90.,
                    camera_z_meters: 4.
                }
            ],
            CalibrationCurve::CameraZ,
            CalibrationExtrapolation::Reject,
            20.
        )
        .is_err());
    release.send(()).unwrap();
    wait_done(&mut session, 1);
    let command = calibration_command(&mut session);
    let revision = session.engine.revision();
    let history = session.engine.history_diagnostics();
    // Metadata changes after capture must be rejected against the transaction's
    // final candidate, rather than validated only before entering the editor.
    let mut asset = before.assets.iter().find(|a| a.id == 7).unwrap().clone();
    asset.path = "assets/another.png".into();
    let error = session
        .apply_command_batch(vec![
            Command::MediaAsset {
                action: motion_core::media_assets::AssetAction::Replace {
                    source: motion_core::media_assets::MediaAsset::Image(asset),
                },
            },
            command.clone(),
        ])
        .unwrap_err()
        .to_string();
    assert!(error.contains("current original source"), "{error}");
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.revision(), revision);
    assert_eq!(session.engine.history_diagnostics(), history);
    session.frame = 1.;
    session.sample().unwrap();
    assert!(session.apply_command_batch(vec![command.clone()]).is_err());
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.revision(), revision);
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 2);
    session.poll_depth().unwrap().unwrap();
    session.cancel_depth().unwrap();
    assert!(session.apply_command_batch(vec![command]).is_err());
    assert_eq!(session.engine.project(), &before);
    assert_eq!(session.engine.history_diagnostics(), history);
    // Removing persisted authoring data is ordinary history and needs no result.
    session
        .apply_command_batch(vec![Command::DepthCalibration {
            object: 1,
            calibration: None,
        }])
        .unwrap();
}

#[test]
fn installed_input_identity_survives_reopen_and_detects_changed_pixels_at_same_asset_key() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    assert!(session.depth_input_fingerprint().unwrap().is_none());
    let project = session.engine.project().clone();
    motion_core::storage::save(directory.path(), &project).unwrap();
    let (engine, seen, _) = backend(false);
    session.configure_depth(engine).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut session, 1);
    session.poll_depth().unwrap().unwrap();
    let original = session.depth_input_fingerprint().unwrap().unwrap();
    let serialized = serde_json::to_vec(&original).unwrap();
    let decoded: motion_model::depth::PreparedDepthInput =
        serde_json::from_slice(&serialized).unwrap();
    assert_eq!(decoded, original);
    let mut reopened = Session::new(
        motion_core::storage::load(directory.path()).unwrap(),
        directory.path().into(),
        Arc::new(NoMedia),
    )
    .unwrap();
    let (engine, seen, _) = backend(false);
    reopened.configure_depth(engine).unwrap();
    reopened
        .request_image_depth(7, geometry(), model())
        .unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut reopened, 1);
    reopened.poll_depth().unwrap().unwrap();
    assert_eq!(reopened.depth_input_fingerprint().unwrap(), Some(original));
    let path = directory.path().join("assets/input.png");
    let mut image = image::open(&path).unwrap().to_rgba8();
    image.get_pixel_mut(0, 0).0[0] += 1;
    image.save(path).unwrap();
    let revision = reopened.engine.revision();
    reopened
        .request_image_depth(7, geometry(), model())
        .unwrap();
    assert!(reopened.depth_input_fingerprint().unwrap().is_none());
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    wait_done(&mut reopened, 2);
    reopened.poll_depth().unwrap().unwrap();
    assert_ne!(reopened.depth_input_fingerprint().unwrap(), Some(original));
    assert_eq!(reopened.engine.revision(), revision);
    assert_eq!(reopened.engine.project(), &project);
    reopened.frame = 1.;
    assert!(reopened.depth_input_fingerprint().unwrap().is_none());
    assert!(!reopened.depth_status().unwrap().unwrap().installed);
}

#[test]
fn default_session_has_no_depth_worker_and_does_not_change_project() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let before = session.engine.snapshot();
    assert!(session.depth_status().unwrap().is_none());
    assert!(session.poll_depth().unwrap().is_none());
    session.cancel_depth().unwrap();
    assert!(session.request_image_depth(7, geometry(), model()).is_err());
    assert_eq!(before, session.engine.snapshot());
    assert!(session.graphics.is_none());
}

#[test]
fn actual_managed_png_is_cropped_rotated_mirrored_off_owner_thread() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let before = session.engine.snapshot();
    let (backend, seen, _) = backend(false);
    session.configure_depth(backend).unwrap();
    let mut geometry = geometry();
    geometry.crop = [1, 0, 2, 3];
    geometry.quarter_turns = 1;
    geometry.mirror_x = true;
    let ticket = session
        .request_image_depth(7, geometry.clone(), model())
        .unwrap();
    let seen = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(seen.thread, session.owner);
    assert_eq!(seen.size, [3, 2]);
    assert_eq!(
        seen.rgb,
        vec![20, 2, 3, 50, 2, 3, 80, 2, 3, 30, 2, 3, 60, 2, 3, 90, 2, 3]
    );
    assert_eq!(seen.key.source.asset, 7);
    assert_eq!(seen.key.source.revision, session.engine.revision());
    assert_eq!(
        (seen.key.source.pts.ticks, seen.key.source.pts.timescale),
        (0, 1)
    );
    wait_done(&mut session, 1);
    let result = session.poll_depth().unwrap().unwrap();
    assert_eq!(result.resource.values(), &[20., 50., 80., 30., 60., 90.]);
    assert_eq!(result.resource.descriptor().geometry, geometry);
    assert!(matches!(
        result.resource.descriptor().measure,
        DepthMeasure::Relative { .. }
    ));
    assert!(Arc::ptr_eq(
        &result,
        &session.poll_depth().unwrap().unwrap()
    ));
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.request, Some(ticket));
    assert!(status.installed);
    assert_eq!(status.counts.completed, 1);
    assert_eq!(session.engine.snapshot(), before);
    assert_eq!(session.frame, 0.);
    assert!(session.last_error.is_none());
}

#[test]
fn seek_cancels_blocked_inference_and_same_static_key_cannot_install_old_ticket() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let (backend, seen, release) = backend(true);
    session.configure_depth(backend).unwrap();
    let first = session.request_image_depth(7, geometry(), model()).unwrap();
    let first_key = seen.recv_timeout(Duration::from_secs(5)).unwrap().key;
    assert_eq!(
        session
            .depth_status()
            .unwrap()
            .unwrap()
            .in_flight
            .unwrap()
            .request,
        first
    );
    session.frame = 1.;
    session.sample().unwrap();
    assert!(session.poll_depth().unwrap().is_none());
    let next = session.request_image_depth(7, geometry(), model()).unwrap();
    assert!(next > first);
    assert_eq!(session.depth_status().unwrap().unwrap().queue_depth, 1);
    release.send(()).unwrap();
    assert!(first_key.same_sample(&seen.recv_timeout(Duration::from_secs(5)).unwrap().key));
    wait_done(&mut session, 2);
    assert!(session.poll_depth().unwrap().is_some());
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.canceled, 1);
    assert_eq!(status.counts.completed, 1);
    assert_eq!(status.request, Some(next));
}

#[test]
fn polling_independently_invalidates_completed_results_before_and_after_install() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let (backend, _seen, _) = backend(false);
    session.configure_depth(backend).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    // No sample()/caller-supplied key at this boundary.
    session.frame = 1.;
    assert!(session.poll_depth().unwrap().is_none());
    session.frame = 0.;
    assert!(session.poll_depth().unwrap().is_none());
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 2);
    assert!(session.poll_depth().unwrap().is_some());
    session
        .engine
        .apply(Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    assert!(session.poll_depth().unwrap().is_none());
    assert!(!session.depth_status().unwrap().unwrap().installed);
}

#[test]
fn relink_and_undo_invalidate_results_and_decode_current_registered_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let (backend, seen, _) = backend(false);
    session.configure_depth(backend).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    let old_key = seen.recv_timeout(Duration::from_secs(5)).unwrap().key;
    wait_done(&mut session, 1);
    assert!(session.poll_depth().unwrap().is_some());
    image::RgbaImage::from_pixel(3, 3, image::Rgba([201, 202, 203, 255]))
        .save(directory.path().join("assets/relinked.png"))
        .unwrap();
    session
        .apply_command_batch(vec![Command::MediaAsset {
            action: motion_core::media_assets::AssetAction::Replace {
                source: motion_core::media_assets::MediaAsset::Image(Asset {
                    id: 7,
                    path: "assets/relinked.png".into(),
                    width: 3,
                    height: 3,
                }),
            },
        }])
        .unwrap();
    assert!(session.poll_depth().unwrap().is_none());
    session.request_image_depth(7, geometry(), model()).unwrap();
    let new = seen.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!old_key.same_sample(&new.key));
    assert_eq!(new.rgb, vec![201, 202, 203].repeat(9));
    wait_done(&mut session, 2);
    assert_eq!(
        session.poll_depth().unwrap().unwrap().resource.values(),
        &[201.; 9]
    );
    assert!(session.engine.undo().unwrap());
    session.sample().unwrap();
    assert!(session.poll_depth().unwrap().is_none());
}

#[test]
fn replacing_same_document_path_and_revision_still_discards_old_completion() {
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let (backend, seen, release) = backend(true);
    session.configure_depth(backend).unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    seen.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = session.document_generation;
    let revision = session.engine.revision();
    let replacement = Engine::new(session.engine.snapshot()).unwrap();
    session
        .replace_project(replacement, directory.path().into())
        .unwrap();
    assert_eq!(session.engine.revision(), revision);
    assert_ne!(session.document_generation, generation);
    release.send(()).unwrap();
    wait_done(&mut session, 1);
    assert!(session.poll_depth().unwrap().is_none());
    assert_eq!(session.depth_status().unwrap().unwrap().counts.canceled, 1);
}

#[test]
fn actual_decode_errors_are_typed_observed_and_do_not_enter_backend_or_retry() {
    init_log();
    LOG.0.lock().unwrap().clear();
    let directory = tempfile::tempdir().unwrap();
    let mut session = session(directory.path());
    let (backend, seen, _) = backend(false);
    session.configure_depth(backend).unwrap();
    std::fs::write(directory.path().join("assets/input.png"), b"not a PNG").unwrap();
    session.request_image_depth(7, geometry(), model()).unwrap();
    wait_done(&mut session, 1);
    assert!(matches!(
        session.poll_depth(),
        Err(motion_host::Error::Depth(motion_host::depth::Error::Image(
            _
        )))
    ));
    assert!(session.poll_depth().unwrap().is_none());
    let status = session.depth_status().unwrap().unwrap();
    assert_eq!(status.counts.failed, 1);
    assert_eq!(status.counts.submitted, 1);
    assert!(status.in_flight.is_none());
    assert!(matches!(seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
    assert!(session.last_error.is_none());
    let records = LOG.0.lock().unwrap();
    let start = records
        .iter()
        .position(|(level, line)| {
            *level == log::Level::Info && line.contains("start #1 depth_infer asset=7")
        })
        .unwrap();
    let failure = records
        .iter()
        .position(|(level, line)| {
            *level == log::Level::Error
                && line.contains("request=1 operation=depth_infer asset=7")
                && line.contains("phase=prepare_or_backend")
                && line.contains("cause=")
        })
        .unwrap();
    assert!(start < failure);
}

#[test]
fn alpha_high_bit_depth_metadata_and_budget_failures_are_explicit() {
    for case in ["alpha", "unorm16", "metadata", "budget"] {
        let directory = tempfile::tempdir().unwrap();
        let mut session = session(directory.path());
        let (backend, seen, _) = backend(false);
        session.configure_depth(backend).unwrap();
        let expected = match case {
            "alpha" => {
                image::RgbaImage::from_pixel(3, 3, image::Rgba([100, 20, 30, 128]))
                    .save(directory.path().join("assets/input.png"))
                    .unwrap();
                "alpha compositing"
            }
            "unorm16" => {
                image::ImageBuffer::from_pixel(3, 3, image::Rgba([1000u16, 2000, 3000, 65535]))
                    .save(directory.path().join("assets/input.png"))
                    .unwrap();
                "sRGB8 conversion"
            }
            "metadata" => {
                image::RgbaImage::new(2, 2)
                    .save(directory.path().join("assets/input.png"))
                    .unwrap();
                "registered source"
            }
            "budget" => {
                let mut policy = session.effects.memory_policy();
                policy.decoded_image_budget_bytes = 98; // 9*(8+3) needs 99 bytes.
                session.configure_memory_policy(policy).unwrap();
                "payload budget"
            }
            _ => unreachable!(),
        };
        session.request_image_depth(7, geometry(), model()).unwrap();
        wait_done(&mut session, 1);
        let error = session.poll_depth().unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "case={case} error={error}"
        );
        assert_eq!(session.depth_status().unwrap().unwrap().counts.failed, 1);
        assert!(matches!(seen.try_recv(), Err(mpsc::TryRecvError::Empty)));
    }
}
