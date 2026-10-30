//! Real import jobs, atomic registrations and separate placement transactions.
mod errors;
use super::{Command, Engine};
use crate::{layer_clipboard::KeyframeSource, workflow::library_media::Request};
use motion_core::{
    media_assets::{AssetKind, AssetRef},
    LibraryAction, Project,
};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn engine(root: &Path) -> Engine {
    Engine::start(root.into(), motion_desktop_media::platform(), 8 << 30).unwrap()
}
fn apply(engine: &Engine, state: &Value, operation: Request) -> Result<Value, String> {
    engine.call(|reply| Command::ProjectAsset {
        source: KeyframeSource::of(state),
        operation,
        reply,
    })
}
fn folder(engine: &Engine, id: u64) -> Value {
    engine
        .edit(
            serde_json::to_string(&motion_core::Command::ProjectLibrary {
                action: LibraryAction::CreateFolder {
                    id,
                    parent: None,
                    name: "素材 Assets".into(),
                },
            })
            .unwrap(),
        )
        .unwrap()
}
fn picture(root: &Path) -> PathBuf {
    let path = root.join("selected-image.png");
    image::RgbaImage::from_pixel(12, 8, image::Rgba([40, 130, 200, 255]))
        .save(&path)
        .unwrap();
    path
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures")
        .join(name)
}
fn start(engine: &Engine, request: &str, file: &Path, kind: &str) {
    engine
        .media(
            json!({"op":"import_media","request_id":request,"path":file,"kind":kind,
        "at_frame":0,"name":"Library fixture"})
            .to_string(),
        )
        .unwrap();
    let began = Instant::now();
    loop {
        let status = engine
            .media(json!({"op":"media_status","request_id":request}).to_string())
            .unwrap();
        if status["state"] == "ready" {
            return;
        }
        assert_eq!(status["state"], "running", "{status}");
        assert!(began.elapsed() < Duration::from_secs(30), "{status}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn registration_status(engine: &Engine, request: &str) {
    let status = engine
        .media(json!({"op":"media_status","request_id":request}).to_string())
        .unwrap();
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["edit_result"]["object"], Value::Null);
    assert_eq!(status["edit_result"]["op"], "register_media");
    let before = engine.state().unwrap();
    let error = apply(
        engine,
        &before,
        Request::Media {
            request: request.into(),
            folder: Some(999),
        },
    )
    .unwrap_err();
    assert!(error.contains("not ready for registration"), "{error}");
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
}

#[test]
fn project_library_foreign_document_cannot_cancel_a_new_documents_import_with_a_reused_request_id()
{
    let temp = tempfile::tempdir().unwrap();
    let e = engine(&temp.path().join("project"));
    let current = folder(&e, 7);
    start(
        &e,
        "reused-library-id",
        &fixture("tone-stereo-48000.wav"),
        "audio",
    );
    for other_root in [false, true] {
        let mut foreign = current.clone();
        if other_root {
            foreign["root"] = json!(temp.path().join("previous-project"));
        } else {
            foreign["document_generation"] =
                json!(current["document_generation"].as_u64().unwrap() + 1);
        }
        let stale = apply(
            &e,
            &foreign,
            Request::Media {
                request: "reused-library-id".into(),
                folder: None,
            },
        )
        .unwrap();
        assert_eq!(stale["assetOperation"]["cancelled"], true);
        assert_eq!(stale["project"], current["project"]);
    }
    let imported = apply(
        &e,
        &current,
        Request::Media {
            request: "reused-library-id".into(),
            folder: Some(7),
        },
    )
    .unwrap();
    assert_eq!(
        imported["project"]["audio_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    e.media(json!({"op":"release_media_task","request_id":"reused-library-id"}).to_string())
        .unwrap();
}

#[test]
fn project_library_image_registration_and_placement_have_separate_atomic_history_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    let before = folder(&e, 7);
    let registered = apply(
        &e,
        &before,
        Request::Image {
            file: picture(temp.path()),
            folder: Some(7),
        },
    )
    .unwrap();
    assert_eq!(registered["project"]["layers"], before["project"]["layers"]);
    assert_eq!(registered["project"]["assets"].as_array().unwrap().len(), 1);
    let id = registered["assetRegistration"]["asset"].as_u64().unwrap();
    assert_eq!(
        registered["project"]["library"]["placements"],
        json!([{"item":{"kind":"image","id":id},"folder":7}])
    );
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    let current = e.history(1).unwrap();
    assert_eq!(current["project"], registered["project"]);
    let target = AssetRef {
        kind: AssetKind::Image,
        id,
    };
    let placed = apply(
        &e,
        &current,
        Request::Place {
            target,
            at_frame: 15,
        },
    )
    .unwrap();
    assert_eq!(placed["project"]["assets"], registered["project"]["assets"]);
    assert_eq!(placed["project"]["layers"].as_array().unwrap().len(), 1);
    assert_eq!(placed["project"]["layers"][0]["timeline"]["in_frame"], 15);
    assert_eq!(
        placed["project"]["layers"][0]["timeline"]["offset_frame"],
        15
    );
    assert_eq!(e.history(0).unwrap()["project"], registered["project"]);
    assert_eq!(e.history(1).unwrap()["project"], placed["project"]);
    e.save().unwrap();
    drop(e);
    let saved = motion_core::storage::load(&root).unwrap();
    assert_eq!(serde_json::to_value(saved).unwrap(), placed["project"]);
    assert_eq!(engine(&root).state().unwrap()["project"], placed["project"]);
}

#[test]
fn project_library_audio_registration_reuses_prepared_pcm_without_creating_a_layer() {
    let temp = tempfile::tempdir().unwrap();
    let e = engine(&temp.path().join("project"));
    let before = folder(&e, 7);
    start(
        &e,
        "library-audio",
        &fixture("tone-stereo-48000.wav"),
        "audio",
    );
    let registered = apply(
        &e,
        &before,
        Request::Media {
            request: "library-audio".into(),
            folder: Some(7),
        },
    )
    .unwrap();
    let p: Project = serde_json::from_value(registered["project"].clone()).unwrap();
    assert!(p.layers.is_empty());
    assert_eq!(p.audio_assets.len(), 1);
    assert_eq!(p.library.placements.len(), 1);
    let cache = motion_media::cache_path(&temp.path().join("project"), &p.audio_assets[0]).unwrap();
    assert!(cache.is_file());
    registration_status(&e, "library-audio");
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    let current = e.history(1).unwrap();
    let placed = apply(
        &e,
        &current,
        Request::Place {
            target: AssetRef {
                kind: AssetKind::Audio,
                id: p.audio_assets[0].id,
            },
            at_frame: 10,
        },
    )
    .unwrap();
    assert_eq!(
        placed["project"]["audio_assets"],
        registered["project"]["audio_assets"]
    );
    assert_eq!(placed["project"]["layers"][0]["timeline"]["in_frame"], 10);
    e.media(json!({"op":"release_media_task","request_id":"library-audio"}).to_string())
        .unwrap();
}

#[cfg(feature = "ffmpeg")]
#[test]
fn project_library_video_and_linked_audio_register_together_and_remain_usable_for_placement() {
    let temp = tempfile::tempdir().unwrap();
    let e = engine(&temp.path().join("project"));
    let before = folder(&e, 7);
    start(
        &e,
        "library-video",
        &fixture("video/sound-24fps.mp4"),
        "video",
    );
    let registered = apply(
        &e,
        &before,
        Request::Media {
            request: "library-video".into(),
            folder: Some(7),
        },
    )
    .unwrap();
    let p: Project = serde_json::from_value(registered["project"].clone()).unwrap();
    assert!(p.layers.is_empty());
    assert_eq!(p.video_assets.len(), 1);
    assert_eq!(p.audio_assets.len(), 1);
    assert_eq!(p.video_assets[0].audio_asset, Some(p.audio_assets[0].id));
    assert_eq!(p.library.placements.len(), 2);
    assert!(p
        .library
        .placements
        .iter()
        .all(|placement| placement.folder == 7));
    let index =
        motion_media::video_cache_path(&temp.path().join("project"), &p.video_assets[0]).unwrap();
    assert!(index.is_file() && index.with_extension("png").is_file());
    registration_status(&e, "library-video");
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    let current = e.history(1).unwrap();
    let placed = apply(
        &e,
        &current,
        Request::Place {
            target: AssetRef {
                kind: AssetKind::Video,
                id: p.video_assets[0].id,
            },
            at_frame: 20,
        },
    )
    .unwrap();
    assert_eq!(
        placed["project"]["video_assets"],
        registered["project"]["video_assets"]
    );
    assert_eq!(placed["project"]["layers"][0]["timeline"]["in_frame"], 20);
    assert_eq!(e.history(0).unwrap()["project"], registered["project"]);
    e.media(json!({"op":"release_media_task","request_id":"library-video"}).to_string())
        .unwrap();
}

#[test]
fn project_library_delayed_video_placement_keeps_audio_clock_and_first_picture() {
    for with_audio in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/source.mp4"), b"123").unwrap();
        let mut project = Project::new(256, 144, 60, 240).unwrap();
        let video = motion_core::VideoAsset {
            id: 1, path: "assets/source.mp4".into(), bytes: 3, mime: "video/avc".into(), track: 0,
            width: 256, height: 144, rotation: 0, display_width: 256, display_height: 144,
            video_start_us: 250_000, video_end_us: 3_000_000, duration_us: 3_250_000,
            frame_count: 66, variable_frame_rate: false, nominal_frame_rate: 24.,
            color_standard: 1, color_range: 2, audio_asset: with_audio.then_some(2),
        };
        project.video_assets.push(video);
        if with_audio {
            project.audio_assets.push(motion_core::AudioAsset {
                id: 2, path: "assets/source.mp4".into(), mime: "audio/mp4".into(), bytes: 3,
                track: 1, sample_rate: 48_000, channels: 2, sample_frames: 156_000,
                duration_us: 3_250_000,
            });
        }
        motion_core::storage::save(&root, &project).unwrap();
        let e = engine(&root);
        let before = e.state().unwrap();
        let placed = apply(&e, &before, Request::Place {
            target: AssetRef { kind: AssetKind::Video, id: 1 }, at_frame: 20,
        }).unwrap();
        let placed: Project = serde_json::from_value(placed["project"].clone()).unwrap();
        let layer = &placed.layers[0];
        let motion_core::Content::Video { video } = &layer.content else { panic!("expected video") };
        assert_eq!(video.hold_first_frame, with_audio);
        assert_eq!(video.source_offset_us, if with_audio { 0 } else { 250_000 });
        assert_eq!(video.picture_time_us(0., placed.fps, &placed.video_assets[0]), 250_000);
        assert_eq!(layer.clip(placed.frames).out_frame, if with_audio { 215 } else { 200 });
        if with_audio { assert_eq!(placed.layer_audio(layer).unwrap().source_offset_us, 0); }
        assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    }
}

#[test]
fn project_library_stale_prepared_media_is_cancelled_and_released_without_publishing_or_blocking_new_imports(
) {
    let temp = tempfile::tempdir().unwrap();
    let e = engine(&temp.path().join("project"));
    let before = e.state().unwrap();
    start(
        &e,
        "stale-library",
        &fixture("tone-stereo-48000.wav"),
        "audio",
    );
    let current = folder(&e, 7);
    let stale = apply(
        &e,
        &before,
        Request::Media {
            request: "stale-library".into(),
            folder: None,
        },
    )
    .unwrap();
    assert_eq!(stale["assetOperation"]["cancelled"], true);
    assert_eq!(stale["project"], current["project"]);
    // A separate explicit user request may reuse the released job identity.
    start(
        &e,
        "stale-library",
        &fixture("tone-stereo-48000.wav"),
        "audio",
    );
    let registered = apply(
        &e,
        &current,
        Request::Media {
            request: "stale-library".into(),
            folder: Some(7),
        },
    )
    .unwrap();
    assert_eq!(
        registered["project"]["audio_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(registered["project"]["layers"]
        .as_array()
        .unwrap()
        .is_empty());
    e.media(json!({"op":"release_media_task","request_id":"stale-library"}).to_string())
        .unwrap();
    let stale = apply(
        &e,
        &before,
        Request::Image {
            file: temp.path().join("must-not-open.png"),
            folder: None,
        },
    )
    .unwrap();
    assert_eq!(stale["assetOperation"]["cancelled"], true);
    assert_eq!(stale["project"], registered["project"]);
}
