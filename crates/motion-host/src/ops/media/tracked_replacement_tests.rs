//! Publication-policy tests. The owned originals are admitted through actual PNG
//! decoding; their model video envelopes exercise the save/history boundary only.
//! Native video admission and decoding are covered by the desktop FFmpeg tests.
use super::*;
use crate::tracking::{asset::*, encode, plate::Extent};
use motion_core::{storage, VideoAsset};
use std::fs;

fn prepare(root: &Path, stem: &str, shade: u8) -> Prepared {
    let path = format!("assets/{stem}.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([shade, 83, 129, 255]))
        .save(root.join(&path))
        .unwrap();
    let mut view = Project::new(64, 64, 30, 90).unwrap();
    view.assets.push(motion_core::Asset {
        id: 1,
        path: path.clone(),
        width: 8,
        height: 6,
    });
    let digest = storage::resource_sha256(&fs::read(root.join(&path)).unwrap());
    let solve = Asset {
        version: VERSION,
        source_id: stem.into(),
        source_sha256: std::array::from_fn(|i| {
            u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap()
        }),
        source_extent: Extent::new(8, 6).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 1000,
        nominal_rate: Rate::new(30, 1).unwrap(),
        source_pts: vec![0],
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "manual".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 0,
                visibility: Visibility::Visible,
                confidence: 0.9,
                observation: Some(Observation::Point([2., 3.])),
                manual_correction: true,
            }],
        }],
    };
    let original = root.join(format!("{stem}.json"));
    fs::write(&original, encode::to_vec(&solve).unwrap()).unwrap();
    Prepared::read(
        root,
        &view,
        0,
        &original,
        AssetRef {
            kind: AssetKind::Image,
            id: 1,
        },
        None,
        || false,
    )
    .unwrap()
}
fn envelope(root: &Path, stem: &str) -> VideoAsset {
    VideoAsset {
        id: 1,
        path: format!("assets/{stem}.png"),
        bytes: fs::metadata(root.join(format!("assets/{stem}.png")))
            .unwrap()
            .len(),
        mime: "video/avc".into(),
        track: 0,
        width: 8,
        height: 6,
        rotation: 0,
        display_width: 8,
        display_height: 6,
        video_start_us: 0,
        video_end_us: 33334,
        duration_us: 33334,
        frame_count: 1,
        variable_frame_rate: false,
        nominal_frame_rate: 30.,
        color_standard: 1,
        color_range: 1,
        audio_asset: None,
    }
}
struct Fixture {
    root: tempfile::TempDir,
    before: Project,
    original: Prepared,
    next: TrackingAsset,
    command: Command,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("assets")).unwrap();
        let mut old = prepare(root.path(), "old", 17);
        let mut descriptor = old.descriptor().clone();
        descriptor.source.kind = AssetKind::Video;
        old.retain_original(true);
        let mut before = Project::new(64, 64, 30, 90).unwrap();
        before.video_assets.push(envelope(root.path(), "old"));
        before.tracking_assets.push(descriptor);
        storage::save(root.path(), &before).unwrap();
        let original = prepare(root.path(), "new", 199);
        let mut next = original.descriptor().clone();
        next.source.kind = AssetKind::Video;
        let command = Command::MediaAsset {
            action: motion_core::media_assets::AssetAction::ReplaceVideo {
                video: envelope(root.path(), "new"),
                audio: None,
            },
        };
        Self {
            root,
            before,
            original,
            next,
            command,
        }
    }
}

#[test]
fn published_acknowledgement_keeps_matching_model_history_and_both_originals() {
    let mut f = Fixture::new();
    let path = f.root.path().join(&f.next.path);
    let bytes = fs::read(&path).unwrap();
    let old_path = f.root.path().join(&f.before.tracking_assets[0].path);
    let old_bytes = fs::read(&old_path).unwrap();
    let mut engine = Engine::new(f.before.clone()).unwrap();
    let mut sink = PairSink {
        engine: &mut engine,
        originals: std::slice::from_mut(&mut f.original),
        tracking: vec![f.next.clone()],
        bindings: vec![],
        acknowledgement: None,
    };
    // Write real bytes before injecting ONLY the durability acknowledgement.
    sink.apply_saved_using(vec![f.command], f.root.path(), |root, candidate| {
        storage::save(root, candidate)?;
        Err(motion_core::Error::Storage {
            phase: "test_pair_directory_ack",
            path: root.into(),
            published: true,
            source: std::io::Error::other("controlled paired durability acknowledgement"),
        })
    })
    .unwrap();
    let error = sink.acknowledgement.take().unwrap();
    assert!(matches!(
        error,
        motion_core::Error::Storage {
            published: true,
            ..
        }
    ));
    assert!(error
        .to_string()
        .contains("controlled paired durability acknowledgement"));
    drop(sink);
    f.original.cleanup_uncommitted().unwrap();
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read(old_path).unwrap(), old_bytes);
    assert_eq!(storage::load(f.root.path()).unwrap(), engine.snapshot());
    assert_eq!(engine.revision(), 1);
    let after = engine.snapshot();
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &f.before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
}

#[test]
fn panic_after_real_publication_retains_reopenable_pair_and_unwinds_author_memory() {
    let mut f = Fixture::new();
    let path = f.root.path().join(&f.next.path);
    let bytes = fs::read(&path).unwrap();
    let mut engine = Engine::new(f.before.clone()).unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut sink = PairSink {
            engine: &mut engine,
            originals: std::slice::from_mut(&mut f.original),
            tracking: vec![f.next.clone()],
            bindings: vec![],
            acknowledgement: None,
        };
        sink.apply_saved_using(vec![f.command], f.root.path(), |root, candidate| {
            storage::save(root, candidate)?;
            panic!("controlled panic after paired publication");
        })
    }));
    assert!(result.is_err());
    drop(f.original);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(engine.project(), &f.before);
    assert_eq!(engine.revision(), 0);
    assert!(!engine.undo().unwrap());
    let reopened = storage::load(f.root.path()).unwrap();
    assert_eq!(reopened.tracking_assets, [f.next]);
    assert_ne!(reopened.video_assets[0].path, f.before.video_assets[0].path);
    assert!(f.root.path().join(&reopened.video_assets[0].path).is_file());
}
