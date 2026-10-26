use super::*;
use crate::engine::{self, Engine};
use motion_core::media_assets::{AssetKind, AssetRef};
use motion_core::Project;
use serde_json::json;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn engine(root: &Path) -> Engine {
    Engine::start(root.to_owned(), motion_desktop_media::platform(), 8 << 30).unwrap()
}
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures")
        .join(name)
}
fn request(engine: &Engine, value: Value) -> Value {
    engine.media(value.to_string()).unwrap()
}
fn start(engine: &Engine, id: &str, path: &Path, kind: &str) -> Value {
    request(
        engine,
        json!({"op":"import_media","request_id":id,"path":path,"kind":kind,"at_frame":0,"name":"asset fixture"}),
    );
    let began = Instant::now();
    loop {
        let status = request(engine, json!({"op":"media_status","request_id":id}));
        if status["state"] != "running" {
            return status;
        }
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "media task did not return: {status}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn finish(engine: &Engine, id: &str, target: AssetRef, revision: u64) -> Result<Value, String> {
    engine.media(json!({"op":"finish_media_replacement","request_id":id,"target":target,"revision":revision}).to_string())
}
fn release(engine: &Engine, id: &str) {
    request(engine, json!({"op":"release_media_task","request_id":id}));
}
fn replace_image(
    engine: &Engine,
    source: PathBuf,
    asset: u64,
    revision: u64,
) -> Result<Value, String> {
    engine
        .request(|reply| engine::Command::ReplaceImage {
            file: source,
            asset,
            revision,
            reply,
        })
        .unwrap()
        .recv()
        .unwrap()
}
fn wave(samples: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(b"RIFF");
    data.extend((36 + samples * 4).to_le_bytes());
    data.extend(b"WAVEfmt ");
    data.extend(16u32.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(2u16.to_le_bytes());
    data.extend(48_000u32.to_le_bytes());
    data.extend(192_000u32.to_le_bytes());
    data.extend(4u16.to_le_bytes());
    data.extend(16u16.to_le_bytes());
    data.extend(b"data");
    data.extend((samples * 4).to_le_bytes());
    for _ in 0..samples {
        data.extend(1024i16.to_le_bytes());
        data.extend(2048i16.to_le_bytes());
    }
    data
}

#[test]
fn media_asset_image_decode_missing_stale_and_success_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let original = temp.path().join("original.png");
    let replacement = temp.path().join("replacement.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 0, 255]))
        .save(&original)
        .unwrap();
    image::RgbaImage::from_pixel(8, 6, image::Rgba([0, 0, 255, 255]))
        .save(&replacement)
        .unwrap();
    let e = engine(&root);
    let before = e.import_image(original).unwrap();
    let id = before["project"]["assets"][0]["id"].as_u64().unwrap();
    let revision = before["revision"].as_u64().unwrap();
    let path = before["project"]["assets"][0]["path"].as_str().unwrap();
    let old_bytes = fs::read(root.join(path)).unwrap();
    let cancelled = replace_image(&e, replacement.clone(), id, revision - 1).unwrap();
    assert_eq!(cancelled["asset_operation_cancelled"], true);
    assert_eq!(cancelled["asset_operation_reason"], "superseded");
    assert_eq!(cancelled["project"], before["project"]);
    for (source, revision) in [
        (temp.path().join("missing.png"), revision),
        (replacement.clone(), revision + 100),
    ] {
        assert!(replace_image(&e, source, id, revision).is_err());
        assert_eq!(e.state().unwrap()["project"], before["project"]);
    }
    let broken = temp.path().join("truncated.png");
    let bytes = fs::read(&replacement).unwrap();
    fs::write(&broken, &bytes[..40]).unwrap();
    assert!(replace_image(&e, broken, id, revision).is_err());
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    let after = replace_image(&e, replacement, id, revision).unwrap();
    assert_eq!(after["project"]["layers"], before["project"]["layers"]);
    assert_eq!(after["project"]["assets"][0]["id"], id);
    assert_eq!(after["project"]["assets"][0]["width"], 8);
    let pixels = image::open(root.join(after["project"]["assets"][0]["path"].as_str().unwrap()))
        .unwrap()
        .to_rgba8();
    assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 255, 255]);
    assert_eq!(fs::read(root.join(path)).unwrap(), old_bytes);
    e.save().unwrap();
    assert_eq!(
        motion_core::storage::load(&root).unwrap().assets[0].width,
        8
    );
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    assert_eq!(e.history(1).unwrap()["project"], after["project"]);
    e.save().unwrap();
    drop(e);
    assert_eq!(engine(&root).state().unwrap()["project"], after["project"]);
}

#[test]
fn media_asset_audio_short_cancel_stale_decode_fail_and_success_are_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    let original = temp.path().join("original.wav");
    fs::write(&original, wave(96_000)).unwrap();
    assert_eq!(start(&e, "original", &original, "audio")["state"], "ready");
    let before = request(
        &e,
        json!({"op":"finish_media_import","request_id":"original"}),
    )["state"]
        .clone();
    release(&e, "original");
    let target = AssetRef {
        kind: AssetKind::Audio,
        id: before["project"]["audio_assets"][0]["id"].as_u64().unwrap(),
    };
    let revision = before["revision"].as_u64().unwrap();
    let short = temp.path().join("short.wav");
    fs::write(&short, wave(4800)).unwrap();
    let files = fs::read_dir(root.join("assets")).unwrap().count();
    assert_eq!(start(&e, "short", &short, "audio")["state"], "ready");
    let error = finish(&e, "short", target, revision).unwrap_err();
    assert!(error.contains("recoverable source interval"), "{error}");
    release(&e, "short");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    assert_eq!(fs::read_dir(root.join("assets")).unwrap().count(), files);
    e.save().unwrap();
    assert_eq!(start(&e, "cancel", &original, "audio")["state"], "ready");
    request(
        &e,
        json!({"op":"cancel_media_import","request_id":"cancel"}),
    );
    release(&e, "cancel");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    let invalid = temp.path().join("invalid.wav");
    fs::write(&invalid, b"invalid audio").unwrap();
    assert_eq!(start(&e, "bad", &invalid, "audio")["state"], "failed");
    release(&e, "bad");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    assert_eq!(start(&e, "stale", &original, "audio")["state"], "ready");
    assert!(finish(&e, "stale", target, revision + 1)
        .unwrap_err()
        .contains("project changed"));
    request(&e, json!({"op":"cancel_media_import","request_id":"stale"}));
    release(&e, "stale");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    // Repair a source removed after the document was opened; no old bytes overwritten.
    assert_eq!(
        start(&e, "superseded", &original, "audio")["state"],
        "ready"
    );
    let superseded = finish(&e, "superseded", target, revision - 1).unwrap();
    assert_eq!(superseded["asset_operation_cancelled"], true);
    assert_eq!(superseded["task"]["state"], "cancelled");
    assert_eq!(superseded["state"]["project"], before["project"]);
    release(&e, "superseded");
    fs::remove_file(
        root.join(
            before["project"]["audio_assets"][0]["path"]
                .as_str()
                .unwrap(),
        ),
    )
    .unwrap();
    assert_eq!(
        start(&e, "replace", &fixture("formats/pcm24.wav"), "audio")["state"],
        "ready"
    );
    let after = finish(&e, "replace", target, revision).unwrap()["state"].clone();
    release(&e, "replace");
    assert_eq!(after["project"]["layers"], before["project"]["layers"]);
    assert_eq!(
        after["project"]["audio_assets"].as_array().unwrap().len(),
        1
    );
    assert_eq!(after["project"]["audio_assets"][0]["id"], target.id);
    e.save().unwrap();
    assert_eq!(
        motion_core::storage::load(&root).unwrap().audio_assets[0].id,
        target.id
    );
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    assert_eq!(e.history(1).unwrap()["project"], after["project"]);
}

#[cfg(feature = "ffmpeg")]
#[test]
fn media_asset_real_video_and_linked_audio_keep_ids_and_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    let source = fixture("video/sound-24fps.mp4");
    let ready = start(&e, "video-original", &source, "video");
    assert_eq!(ready["state"], "ready", "{ready}");
    let imported = request(
        &e,
        json!({"op":"finish_media_import","request_id":"video-original"}),
    )["state"]
        .clone();
    release(&e, "video-original");
    let object = imported["project"]["layers"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()["id"]
        .as_u64()
        .unwrap();
    let before = e
        .edit(serde_json::to_string(&Command::ExtractAudio { object }).unwrap())
        .unwrap();
    let video = &before["project"]["video_assets"][0];
    let target = AssetRef {
        kind: AssetKind::Video,
        id: video["id"].as_u64().unwrap(),
    };
    let old_audio = video["audio_asset"].clone();
    assert!(
        old_audio.as_u64().is_some(),
        "fixture must cover linked audio"
    );
    let uses = request(
        &e,
        json!({"op":"media_asset_usages","target":{"kind":"audio","id":old_audio}}),
    );
    assert_eq!(uses["usages"].as_array().unwrap().len(), 2);
    assert!(uses["usages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["role"] == "audio"));
    assert!(uses["usages"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["role"] == "video_audio"));
    let old_path = video["path"].as_str().unwrap();
    let bytes = fs::read(root.join(old_path)).unwrap();
    assert_eq!(
        start(
            &e,
            "video-no-audio",
            &fixture("video/silent-24fps.mp4"),
            "video"
        )["state"],
        "ready"
    );
    let rejected = finish(
        &e,
        "video-no-audio",
        target,
        before["revision"].as_u64().unwrap(),
    )
    .unwrap_err();
    assert!(rejected.contains("referenced"), "{rejected}");
    release(&e, "video-no-audio");
    assert_eq!(e.state().unwrap()["project"], before["project"]);
    assert_eq!(
        start(&e, "video-replace", &source, "video")["state"],
        "ready"
    );
    let after = finish(
        &e,
        "video-replace",
        target,
        before["revision"].as_u64().unwrap(),
    )
    .unwrap()["state"]
        .clone();
    release(&e, "video-replace");
    assert_eq!(after["project"]["layers"], before["project"]["layers"]);
    assert_eq!(
        after["project"]["video_assets"][0]["audio_asset"],
        old_audio
    );
    assert_eq!(
        after["project"]["audio_assets"].as_array().unwrap().len(),
        1
    );
    assert_eq!(fs::read(root.join(old_path)).unwrap(), bytes);
    e.save().unwrap();
    assert_eq!(
        motion_core::storage::load(&root).unwrap().video_assets[0].id,
        target.id
    );
    assert_eq!(e.history(0).unwrap()["project"], before["project"]);
    assert_eq!(e.history(1).unwrap()["project"], after["project"]);
}

#[cfg(feature = "ffmpeg")]
#[test]
fn media_asset_video_audio_creation_and_unused_removal_are_single_edits() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let e = engine(&root);
    assert_eq!(
        start(
            &e,
            "silent-import",
            &fixture("video/silent-24fps.mp4"),
            "video"
        )["state"],
        "ready"
    );
    let first = request(
        &e,
        json!({"op":"finish_media_import","request_id":"silent-import"}),
    )["state"]
        .clone();
    release(&e, "silent-import");
    let target = AssetRef {
        kind: AssetKind::Video,
        id: first["project"]["video_assets"][0]["id"].as_u64().unwrap(),
    };
    assert!(serde_json::from_value::<Project>(first["project"].clone())
        .unwrap()
        .audio_assets
        .is_empty());
    assert_eq!(
        start(&e, "add-audio", &fixture("video/sound-24fps.mp4"), "video")["state"],
        "ready"
    );
    let with_audio = finish(&e, "add-audio", target, first["revision"].as_u64().unwrap()).unwrap()
        ["state"]
        .clone();
    assert!(finish(
        &e,
        "add-audio",
        target,
        with_audio["revision"].as_u64().unwrap()
    )
    .unwrap_err()
    .contains("not ready"));
    release(&e, "add-audio");
    assert_eq!(
        with_audio["project"]["audio_assets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(with_audio["project"]["layers"], first["project"]["layers"]);
    assert_eq!(
        start(
            &e,
            "remove-audio",
            &fixture("video/silent-24fps.mp4"),
            "video"
        )["state"],
        "ready"
    );
    let silent = finish(
        &e,
        "remove-audio",
        target,
        with_audio["revision"].as_u64().unwrap(),
    )
    .unwrap()["state"]
        .clone();
    release(&e, "remove-audio");
    assert!(serde_json::from_value::<Project>(silent["project"].clone())
        .unwrap()
        .audio_assets
        .is_empty());
    assert!(silent["project"]["video_assets"][0]["audio_asset"].is_null());
    assert_eq!(silent["project"]["layers"], first["project"]["layers"]);
    assert_eq!(e.history(0).unwrap()["project"], with_audio["project"]);
    assert_eq!(e.history(1).unwrap()["project"], silent["project"]);
    e.save().unwrap();
    assert!(motion_core::storage::load(&root)
        .unwrap()
        .audio_assets
        .is_empty());
}

#[test]
fn media_asset_default_error_channel_preserves_causes_and_stdout() {
    const CHILD: &str = "MOTION_MEDIA_ASSET_ERROR_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.join("logs")).unwrap();
        let e = engine(&root.join("project"));
        let source = root.join("original.png");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255]))
            .save(&source)
            .unwrap();
        let before = e.import_image(source).unwrap();
        let id = before["project"]["assets"][0]["id"].as_u64().unwrap();
        let revision = before["revision"].as_u64().unwrap();
        let cancelled =
            replace_image(&e, root.join("superseded-source.png"), id, revision - 1).unwrap();
        assert_eq!(cancelled["asset_operation_cancelled"], true);
        assert_eq!(cancelled["project"], before["project"]);
        let reason =
            replace_image(&e, root.join("missing-replacement.png"), id, revision).unwrap_err();
        assert_eq!(e.state().unwrap()["project"], before["project"]);
        let audio = root.join("original.wav");
        fs::write(&audio, wave(96_000)).unwrap();
        assert_eq!(
            start(&e, "error-original", &audio, "audio")["state"],
            "ready"
        );
        let audio_before = request(
            &e,
            json!({"op":"finish_media_import","request_id":"error-original"}),
        )["state"]
            .clone();
        release(&e, "error-original");
        let short = root.join("short.wav");
        fs::write(&short, wave(4800)).unwrap();
        assert_eq!(start(&e, "error-short", &short, "audio")["state"], "ready");
        let audio_target = AssetRef {
            kind: AssetKind::Audio,
            id: audio_before["project"]["audio_assets"][0]["id"]
                .as_u64()
                .unwrap(),
        };
        let audio_cause = finish(
            &e,
            "error-short",
            audio_target,
            audio_before["revision"].as_u64().unwrap(),
        )
        .unwrap_err();
        release(&e, "error-short");
        assert_eq!(e.state().unwrap()["project"], audio_before["project"]);
        fs::write(
            root.join("failure.json"),
            json!({"cause":reason,"audioCause":audio_cause,"asset":id,"revision":revision,"projectUnchanged":true})
                .to_string(),
        )
        .unwrap();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_MEDIA_ASSET_ERROR_ARTIFACTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","workflow::media_asset_tests::media_asset_default_error_channel_preserves_causes_and_stdout"])
        .env_remove("MOTION_LOG").env(CHILD,&root).output().unwrap();
    fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value =
        serde_json::from_slice(&fs::read(root.join("failure.json")).unwrap()).unwrap();
    let log = fs::read_to_string(root.join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let cause = data["cause"].as_str().unwrap();
    assert!(log.contains(cause), "original file cause missing: {log}");
    assert!(
        stderr.contains(cause),
        "Error did not reach stderr: {stderr}"
    );
    assert!(
        stderr.contains("missing-replacement.png")
            && stderr.contains("asset=")
            && stderr.contains("phase=replace")
    );
    assert!(!stdout.contains(cause));
    let audio_cause = data["audioCause"].as_str().unwrap();
    assert!(log.contains(audio_cause) && stderr.contains(audio_cause));
    assert!(
        audio_cause.contains("request=error-short")
            && audio_cause.contains("phase=commit")
            && audio_cause.contains("recoverable source interval")
    );
    assert!(!stdout.contains(audio_cause));
    assert!(
        !stderr.contains("superseded-source.png"),
        "superseded requests are normal cancellation"
    );
    assert!(
        log.contains("start #") && log.contains(" replace_image"),
        "dispatch start missing: {log}"
    );
    assert!(log.contains("image replacement preflight asset="));
    assert!(
        !stderr.contains("image replacement preflight asset="),
        "Info leaked to stderr"
    );
    assert!(!stdout.contains("motion_studio::engine"));
    assert!(
        log.find("image replacement preflight asset=").unwrap() < log.find("error #").unwrap(),
        "start must precede failure"
    );
}
