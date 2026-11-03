use super::*;
use motion_core::{
    media_assets::{AssetKind, AssetRef},
    storage, Engine, Project,
};
use motion_host::{
    platform::Platform,
    tracking::{asset::*, encode, import::Prepared, plate::Extent},
};
use std::{cell::Cell, fs, path::PathBuf};

fn solve(path: &Path, pts: Vec<i64>, ticks: u32, width: u32, height: u32) -> Asset {
    let digest = storage::resource_sha256(&fs::read(path).unwrap());
    Asset {
        version: VERSION,
        source_id: "independently authored native video clock".into(),
        source_sha256: std::array::from_fn(|i| {
            u8::from_str_radix(&digest[i * 2..i * 2 + 2], 16).unwrap()
        }),
        source_extent: Extent::new(width, height).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: ticks,
        nominal_rate: Rate::new(30, 1).unwrap(),
        source_pts: pts,
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "point".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 0,
                visibility: Visibility::Visible,
                confidence: 1.,
                observation: Some(Observation::Point([2., 3.])),
                manual_correction: false,
            }],
        }],
    }
}

struct Fixture {
    root: tempfile::TempDir,
    path: PathBuf,
    asset: VideoAsset,
    solve: Asset,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let path = root.path().join("assets/plate.mp4");
        let mut encoder = crate::encode::Mp4Encoder::new(&path, 64, 32, 30).unwrap();
        for index in 0..6 {
            let pixels: Vec<u8> = (0..64 * 32)
                .flat_map(|p| [index as u8 * 31, (p % 64) as u8 * 3, 79, 255])
                .collect();
            encoder.video_frame(&pixels, index).unwrap();
        }
        // The product MP4 encoder requires both video and audio. Author exactly
        // 0.2 seconds of silence for this six-frame 30 fps source.
        let mut start = 0;
        while start < 9600 {
            let samples = encoder.audio_frame_size().min(9600 - start);
            encoder
                .audio_frame(&vec![0.; samples * 2], start as u64)
                .unwrap();
            start += samples;
        }
        encoder.finish().unwrap();
        let mut probe = super::super::probe(&path, None, None, &|| Ok(())).unwrap();
        probe.asset.path = "assets/plate.mp4".into();
        assert_eq!(probe.asset.frame_count, 6);
        let cache = motion_media::video_cache_path(root.path(), &probe.asset).unwrap();
        fs::create_dir_all(cache.parent().unwrap()).unwrap();
        motion_media::save_index(&cache, &probe.asset, &probe.timestamps).unwrap();
        // Authored encoder frame n has PTS n/30. The declared clock deliberately
        // resolves below one microsecond; the preview index is not our oracle.
        let solve = solve(
            &path,
            (0..6).map(|n| n * 30_000_000).collect(),
            900_000_000,
            64,
            32,
        );
        Self {
            root,
            path,
            asset: probe.asset,
            solve,
        }
    }
    fn verify(&self, solve: &Asset) -> Result<()> {
        verify(&self.path, &self.asset, solve, &|| Ok(()))
    }
    fn project(&self) -> Project {
        let mut project = Project::new(64, 64, 30, 90).unwrap();
        project.video_assets.push(self.asset.clone());
        storage::save(self.root.path(), &project).unwrap();
        project
    }
}

#[test]
fn original_native_frames_verify_and_managed_import_is_saved_and_undoable() {
    let f = Fixture::new();
    f.verify(&f.solve).unwrap();
    crate::DesktopPlatform::new()
        .verify_tracking_video(&f.path, &f.asset, &f.solve, &|| Ok(()))
        .unwrap();
    let project = f.project();
    let original = encode::to_vec(&f.solve).unwrap();
    let document = f.root.path().join("original.json");
    fs::write(&document, &original).unwrap();
    let platform = crate::DesktopPlatform::new();
    let mut prepared = Prepared::read(
        f.root.path(),
        &project,
        0,
        &document,
        AssetRef {
            kind: AssetKind::Video,
            id: 1,
        },
        Some(&platform),
        || false,
    )
    .unwrap();
    let managed = f.root.path().join(&prepared.descriptor().path);
    let mut engine = Engine::new(project.clone()).unwrap();
    prepared.commit(&mut engine).unwrap();
    prepared.finish(Ok(())).unwrap();
    assert_eq!(fs::read(managed).unwrap(), original);
    assert_eq!(storage::load(f.root.path()).unwrap(), *engine.project());
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &project);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
}

#[test]
fn native_sub_microsecond_disagreement_is_rejected_before_any_project_mutation() {
    let mut f = Fixture::new();
    f.solve.source_pts[1] += 1;
    let error = f.verify(&f.solve).unwrap_err().to_string();
    assert!(
        error.contains("frame=1") && error.contains("expected=30000001/900000000"),
        "{error}"
    );
    let project = f.project();
    let document = f.root.path().join("wrong-clock.json");
    fs::write(&document, encode::to_vec(&f.solve).unwrap()).unwrap();
    let platform = crate::DesktopPlatform::new();
    assert!(Prepared::read(
        f.root.path(),
        &project,
        0,
        &document,
        AssetRef {
            kind: AssetKind::Video,
            id: 1
        },
        Some(&platform),
        || false
    )
    .is_err());
    assert!(!f.root.path().join("assets/tracking").exists());
    assert_eq!(storage::load(f.root.path()).unwrap(), project);
}

#[test]
fn native_verification_rejects_frame_count_and_changed_source_dimensions() {
    let f = Fixture::new();
    let mut missing = f.solve.clone();
    missing.source_pts.pop();
    assert!(f.verify(&missing).is_err());
    let mut extra = f.solve.clone();
    extra.source_pts.push(180_000_000);
    assert!(f.verify(&extra).is_err());
    let mut wrong = f.asset.clone();
    wrong.width = 62;
    wrong.display_width = 62;
    assert!(verify(&f.path, &wrong, &f.solve, &|| Ok(())).is_err());
    wrong = f.asset.clone();
    wrong.track = 100;
    assert!(verify(&f.path, &wrong, &f.solve, &|| Ok(())).is_err());
}

#[test]
fn cancellation_is_observed_inside_native_decode() {
    let f = Fixture::new();
    let count = Cell::new(0);
    let error = verify(&f.path, &f.asset, &f.solve, &|| {
        count.set(count.get() + 1);
        if count.get() == 5 {
            Err("controlled decoded cancellation".into())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(count.get(), 5);
    let bounded = verify(
        &PathBuf::from("p".repeat(4096)),
        &f.asset,
        &f.solve,
        &|| Err("controlled original decoded cause".into()),
    )
    .unwrap_err()
    .to_string();
    assert!(
        bounded.len() < 512 && bounded.contains("controlled original decoded cause"),
        "{bounded}"
    );
    assert!(error
        .to_string()
        .contains("controlled decoded cancellation"));
}

#[test]
fn prerecorded_b_frames_nonzero_origin_and_vfr_use_original_packet_clock() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../motion-media/tests/fixtures/video");
    let timing: serde_json::Value =
        serde_json::from_slice(&fs::read(folder.join("native-timing.json")).unwrap()).unwrap();
    for name in ["video-ctts.mp4", "video-delayed.mp4"] {
        let path = folder.join(name);
        let probe = super::super::probe(&path, None, None, &|| Ok(())).unwrap();
        let row = &timing[name];
        let mut pts: Vec<i64> = row["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s[0].as_i64().unwrap())
            .collect();
        // Fixture packet tables are in decode order. Sorting this independent
        // oracle obtains presentation order; production never repairs its input.
        pts.sort();
        assert!(pts.windows(3).any(|p| p[2] - p[1] != p[1] - p[0]) || pts[0] != 0);
        let solve = solve(
            &path,
            pts,
            row["timescale"].as_u64().unwrap() as u32,
            probe.asset.width,
            probe.asset.height,
        );
        verify(&path, &probe.asset, &solve, &|| Ok(())).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}
