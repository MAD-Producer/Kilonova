//! Exercise the actual staged relink protocol with an attached calibrated camera.
use motion_core::tracking::camera::*;
use motion_core::{storage, Project};
use motion_host::tracking::{asset::*, import::Prepared, plate::*};
use motion_host::{Platform, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{fs, io::Cursor, path::Path, sync::Arc};
struct Stub;
impl Platform for Stub {
    fn name(&self) -> &'static str {
        "asset-registration-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("registration must not probe media")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("registration must not decode video")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("registration must not decode audio")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _budget: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("registration must not create a surface")
    }
}

struct Fixture {
    root: tempfile::TempDir,
    id: i64,
    bytes: Vec<u8>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        motion_host::session::close(self.id);
    }
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            16,
            12,
            image::Rgba([17, 83, 129, 255]),
        ))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
        let bytes = out.into_inner();
        fs::write(root.path().join("assets/plate.png"), &bytes).unwrap();
        let digest = storage::resource_sha256(&bytes);
        let extent = Extent::new(16, 12).unwrap();
        let data = Asset {
            version: VERSION,
            source_id: "authored relink fixture".into(),
            source_sha256: std::array::from_fn(|i| {
                u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap()
            }),
            source_extent: extent,
            coordinates: Coordinates::RightDownForward,
            ticks_per_second: 24000,
            nominal_rate: Rate::new(24, 1).unwrap(),
            source_pts: vec![0],
            meters_per_world_unit: Some(1.),
            tracks: vec![Track {
                id: "camera".into(),
                kind: Kind::Camera,
                reference_index: None,
                samples: vec![Sample {
                    source_index: 0,
                    visibility: Visibility::Visible,
                    confidence: 1.,
                    manual_correction: false,
                    observation: Some(Observation::Camera {
                        camera_to_world: Pose {
                            position: [0., 0., -5.],
                            orientation_xyzw: [0., 0., 0., 1.],
                        },
                        calibration: Calibration::new(
                            extent,
                            [32.; 2],
                            [8., 6.],
                            [-0.001, 0., 0.],
                            [0.; 2],
                        )
                        .unwrap(),
                    }),
                }],
            }],
        };
        let document = root.path().join("original.json");
        fs::write(
            &document,
            motion_host::tracking::encode::to_vec(&data).unwrap(),
        )
        .unwrap();
        let mut p = Project::new(64, 64, 24, 96).unwrap();
        p.assets.push(motion_core::Asset {
            id: 1,
            path: "assets/plate.png".into(),
            width: 16,
            height: 12,
        });
        storage::save(root.path(), &p).unwrap();
        let mut core = motion_core::Engine::new(p).unwrap();
        let mut prepared = Prepared::read(
            root.path(),
            core.project(),
            core.revision(),
            &document,
            motion_core::media_assets::AssetRef {
                kind: motion_core::media_assets::AssetKind::Image,
                id: 1,
            },
            None,
            || false,
        )
        .unwrap();
        prepared.commit(&mut core).unwrap();
        prepared.finish(Ok(())).unwrap();
        let mut p = core.snapshot();
        p.camera.created = true;
        let reference = CorrectionReference::capture(&p.camera, 0., [64, 64]).unwrap();
        p.camera.tracking = Some(CameraBinding {
            asset: 2,
            track: "camera".into(),
            anchor_local_frame: 0,
            source_anchor_pts: 0,
            speed_numerator: 1,
            speed_denominator: 1,
            world_basis: [1, -2, -3],
            target_meters_per_unit: 0.01,
            target_origin: [0.; 3],
            correction_reference: reference,
            crop: [0., 0., 16., 12.],
            rotation: Rotation::None,
            near: 0.5,
            far: 100000.,
            interpolation: motion_core::tracking::point::Interpolation::Linear,
            max_gap_ticks: 0,
            minimum_confidence: 0.8,
            missing: motion_core::tracking::point::Missing::Error,
        });
        p.validate().unwrap();
        storage::save(root.path(), &p).unwrap();
        let id = motion_host::session::open(root.path().into(), p, Arc::new(Stub)).unwrap();
        Self { root, id, bytes }
    }
    fn state(&self) -> Value {
        motion_host::ops::project::state(self.id).unwrap()
    }
    fn media(&self, v: Value) -> motion_host::Result<Value> {
        motion_host::ops::media::request(self.id, v, None)
    }
    fn stage(&self, request: &str, path: &Path) {
        let state = self.state();
        self.media(json!({"op":"start_asset_relink","request_id":request,
   "revision":state["revision"],"document_generation":state["document_generation"],
   "candidates":[{"target":{"kind":"image","id":1},"path":path}]}))
            .unwrap();
        for _ in 0..8 {
            let status = self
                .media(json!({"op":"media_status","request_id":request}))
                .unwrap();
            if status["state"] == "ready" {
                return;
            }
            assert_eq!(status["state"], "running", "{status}");
        }
        panic!("image staging did not become ready");
    }
}
#[test]
fn tracking_camera_relink_rejects_equal_length_changed_source_before_publication() {
    let f = Fixture::new();
    let warm = pixels(f.id);
    let before = f.state();
    let saved = fs::read(f.root.path().join("project.json")).unwrap();
    let candidate = f.root.path().join("different.png");
    // This differently colored fixture has the same encoded length and extent.
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        16,
        12,
        image::Rgba([18, 83, 129, 255]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    let wrong = out.into_inner();
    assert_eq!(wrong.len(), f.bytes.len());
    fs::write(&candidate, &wrong).unwrap();
    f.stage("different", &candidate);
    let result = f.media(json!({"op":"finish_asset_relink","request_id":"different"}));
    assert!(
        result.is_err(),
        "different original bytes reused a calibrated camera: {result:?}"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("tracking source content differs"));
    assert_eq!(f.state()["project"], before["project"]);
    assert_eq!(fs::read(f.root.path().join("project.json")).unwrap(), saved);
    assert_eq!(pixels(f.id), warm);
    assert_eq!(f.state()["revision"], before["revision"]);
    let status = f
        .media(json!({"op":"media_status","request_id":"different"}))
        .unwrap();
    assert_eq!(status["state"], "failed");
    // A failed candidate must not poison the next explicit relink request.
    f.media(json!({"op":"release_asset_relink","request_id":"different"}))
        .unwrap();
    let valid = f.root.path().join("same.png");
    fs::write(&valid, &f.bytes).unwrap();
    f.stage("recovery", &valid);
    assert_eq!(
        f.media(json!({"op":"finish_asset_relink","request_id":"recovery"}))
            .unwrap()["task"]["state"],
        "succeeded"
    );
}

fn pixels(id: i64) -> Vec<u8> {
    let mut pixels = vec![0; 16 * 12 * 4];
    motion_host::ops::images::asset_pixels_into(id, 1, &mut pixels).unwrap();
    pixels
}

#[test]
fn identical_plate_relink_preserves_camera_original_undo_redo_reopen_and_warm_decode() {
    let f = Fixture::new();
    let before = f.state();
    let warm = pixels(f.id);
    assert!(warm.chunks_exact(4).all(|v| v == [17, 83, 129, 255]));
    let old: Project = serde_json::from_value(before["project"].clone()).unwrap();
    let document = old.tracking_assets[0].path.clone();
    let original = fs::read(f.root.path().join(&document)).unwrap();
    let candidate = f.root.path().join("same.png");
    fs::write(&candidate, &f.bytes).unwrap();
    f.stage("same", &candidate);
    let done = f
        .media(json!({"op":"finish_asset_relink","request_id":"same"}))
        .unwrap();
    assert_eq!(done["task"]["state"], "succeeded");
    let after = f.state();
    let new: Project = serde_json::from_value(after["project"].clone()).unwrap();
    assert_ne!(old.assets[0].path, new.assets[0].path);
    assert_eq!(old.camera, new.camera);
    assert_eq!(old.tracking_assets, new.tracking_assets);
    assert_eq!(
        fs::read(f.root.path().join(&new.assets[0].path)).unwrap(),
        f.bytes
    );
    assert_eq!(pixels(f.id), warm);
    assert_eq!(fs::read(f.root.path().join(&document)).unwrap(), original);
    let undo =
        motion_host::ops::editing::history(f.id, motion_host::ops::editing::HISTORY_UNDO).unwrap();
    assert_eq!(undo["project"], before["project"]);
    assert_eq!(pixels(f.id), warm);
    let redo =
        motion_host::ops::editing::history(f.id, motion_host::ops::editing::HISTORY_REDO).unwrap();
    assert_eq!(redo["project"], after["project"]);
    assert_eq!(pixels(f.id), warm);
    motion_host::ops::project::save(f.id).unwrap();
    let reopened = motion_host::session::open(
        f.root.path().into(),
        storage::load(f.root.path()).unwrap(),
        Arc::new(Stub),
    )
    .unwrap();
    let reopened_state = motion_host::ops::project::state(reopened).unwrap();
    assert_eq!(reopened_state["project"], after["project"]);
    assert_eq!(pixels(reopened), warm);
    motion_host::session::close(reopened);
}

#[test]
fn cancelled_relink_keeps_bound_camera_and_original_plate() {
    let f = Fixture::new();
    let before = f.state();
    let candidate = f.root.path().join("same.png");
    fs::write(&candidate, &f.bytes).unwrap();
    f.stage("cancelled", &candidate);
    let cancelled = f
        .media(json!({"op":"cancel_asset_relink","request_id":"cancelled"}))
        .unwrap();
    assert_eq!(cancelled["state"], "cancelled");
    assert!(f
        .media(json!({"op":"finish_asset_relink","request_id":"cancelled"}))
        .is_err());
    assert_eq!(f.state()["project"], before["project"]);
}

#[test]
fn relink_rechecks_staged_bytes_after_readiness() {
    let f = Fixture::new();
    let before = f.state();
    let candidate = f.root.path().join("same.png");
    fs::write(&candidate, &f.bytes).unwrap();
    f.stage("tampered", &candidate);
    let workspace = fs::read_dir(f.root.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".asset-relink-")
        })
        .unwrap();
    let staged = workspace.join("assets/image-1/plate.png");
    let mut changed = f.bytes.clone();
    changed.push(0);
    fs::write(&staged, &changed).unwrap();
    assert!(f
        .media(json!({"op":"finish_asset_relink","request_id":"tampered"}))
        .unwrap_err()
        .to_string()
        .contains("tracking source content differs"));
    assert_eq!(f.state()["project"], before["project"]);
}

#[test]
fn superseded_document_cancels_relink_without_verifying_stale_files() {
    let f = Fixture::new();
    let before = f.state();
    let candidate = f.root.path().join("same.png");
    fs::write(&candidate, &f.bytes).unwrap();
    f.stage("stale", &candidate);
    // A real document replacement changes generation even for identical data.
    motion_host::session::with_session(f.id, |s| {
        let engine = motion_core::Engine::new(s.engine.snapshot())?;
        s.replace_project(engine, s.root.clone())
    })
    .unwrap();
    let result = f
        .media(json!({"op":"finish_asset_relink","request_id":"stale"}))
        .unwrap();
    assert_eq!(result["asset_operation_cancelled"], true);
    assert_eq!(result["asset_operation_reason"], "superseded");
    assert_eq!(f.state()["project"], before["project"]);
}

#[test]
fn untracked_media_still_allows_content_replacement() {
    let f = Fixture::new();
    motion_host::session::with_session(f.id, |s| {
        let mut p = s.engine.snapshot();
        p.camera.tracking = None;
        p.tracking_assets.clear();
        s.replace_project(motion_core::Engine::new(p)?, s.root.clone())
    })
    .unwrap();
    let candidate = f.root.path().join("replacement.png");
    let mut out = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        16,
        12,
        image::Rgba([51, 73, 111, 255]),
    ))
    .write_to(&mut out, image::ImageFormat::Png)
    .unwrap();
    fs::write(&candidate, out.into_inner()).unwrap();
    f.stage("untracked", &candidate);
    assert_eq!(
        f.media(json!({"op":"finish_asset_relink","request_id":"untracked"}))
            .unwrap()["task"]["state"],
        "succeeded"
    );
    assert!(pixels(f.id)
        .chunks_exact(4)
        .all(|v| v == [51, 73, 111, 255]));
}

#[test]
fn relink_rechecks_original_solve_document_before_publishing_source() {
    let f = Fixture::new();
    let before = f.state();
    let candidate = f.root.path().join("same.png");
    fs::write(&candidate, &f.bytes).unwrap();
    f.stage("solve_changed", &candidate);
    let document = f.root.path().join(
        before["project"]["tracking_assets"][0]["path"]
            .as_str()
            .unwrap(),
    );
    let mut original = fs::read(&document).unwrap();
    original.push(b' ');
    fs::write(&document, &original).unwrap();
    let cause = f
        .media(json!({"op":"finish_asset_relink","request_id":"solve_changed"}))
        .unwrap_err();
    assert!(
        cause
            .to_string()
            .contains("tracking_asset=2 source=Image:1 phase=verify_tracking"),
        "{cause}"
    );
    assert_eq!(f.state()["project"], before["project"]);
    assert_eq!(f.state()["revision"], before["revision"]);
}
