use crate::{
    engine::Engine,
    mcp::{Access, ToolRouter},
};
use motion_host::tracking::{
    asset::{Asset, Observation, Sample},
    encode,
    plate::{Calibration, Extent},
};
use motion_host::Platform;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[path = "../../../../crates/motion-model/tests/support/tracking_camera.rs"]
mod authored;

fn solve(path: &Path, name: &str, x: f64) -> Asset {
    let platform = motion_desktop_media::platform();
    let probe = platform
        .probe_video(path, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let manifest: Value =
        serde_json::from_slice(&fs::read(fixtures.join("native-timing.json")).unwrap()).unwrap();
    let mut data = (*authored::camera_fixture().tracking_assets[0].data).clone();
    data.source_extent = Extent::new(probe.asset.width, probe.asset.height).unwrap();
    let hash = motion_core::storage::resource_sha256(&fs::read(path).unwrap());
    data.source_sha256 =
        std::array::from_fn(|i| u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).unwrap());
    data.ticks_per_second = manifest[name]["timescale"].as_u64().unwrap() as u32;
    data.source_pts = manifest[name]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s[0].as_i64().unwrap())
        .collect();
    data.source_pts.sort_unstable();
    let mut sample = data.tracks[0].samples[0].clone();
    if let Some(Observation::Camera {
        camera_to_world,
        calibration,
    }) = &mut sample.observation
    {
        camera_to_world.position[0] = x;
        *calibration = Calibration::new(
            data.source_extent,
            [180., 160.],
            [128., 72.],
            [0.; 3],
            [0.; 2],
        )
        .unwrap();
    }
    data.tracks[0].samples = (0..data.source_pts.len())
        .map(|i| Sample {
            source_index: i as u32,
            ..sample.clone()
        })
        .collect();
    data
}

#[test]
fn native_editor_router_prepares_checks_context_and_commits_one_saved_history_step() {
    let temporary = tempfile::tempdir().unwrap();
    let observation = std::env::var_os("MOTION_TRACKED_RPC_DIR").map(PathBuf::from);
    let directory = observation.as_deref().unwrap_or(temporary.path());
    let root = directory.join("native-project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/motion-media/tests/fixtures/video");
    let old = root.join("assets/original.mp4");
    fs::copy(fixtures.join("video-ctts.mp4"), &old).unwrap();
    let mut project = authored::camera_fixture();
    let mut binding = project.camera.tracking.take().unwrap();
    project.tracking_assets.clear();
    project.assets.clear();
    let mut probe = motion_desktop_media::platform()
        .probe_video(&old, None, Some(u32::MAX), &|| Ok(()))
        .unwrap();
    probe.asset.id = 1;
    probe.asset.path = "assets/original.mp4".into();
    project.video_assets = vec![probe.asset.clone()];
    let old_solve = solve(&old, "video-ctts.mp4", 0.);
    binding.crop = [0., 0., probe.asset.width as f64, probe.asset.height as f64];
    binding.source_anchor_pts = old_solve.source_pts[0];
    motion_core::storage::save(&root, &project).unwrap();
    let index = motion_media::video_cache_path(&root, &probe.asset).unwrap();
    fs::create_dir_all(index.parent().unwrap()).unwrap();
    motion_media::save_index(&index, &probe.asset, &probe.timestamps).unwrap();
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let edit = ToolRouter {
        engine: &engine,
        access: Access::Edit,
    };
    let read = ToolRouter {
        engine: &engine,
        access: Access::ReadOnly,
    };
    let old_document = directory.join("old.mstrack");
    fs::write(&old_document, encode::to_vec(&old_solve).unwrap()).unwrap();
    let state = engine.state().unwrap();
    let imported=edit.call("import_tracking",json!({"path":old_document,"source":{"kind":"video","id":1},
        "expectedRevision":state["revision"],"expectedDocumentGeneration":state["document_generation"]})).unwrap();
    let asset = imported["project"]["tracking_assets"][0]["id"]
        .as_u64()
        .unwrap();
    binding.asset = asset;
    edit.call("command_run",json!({"command":{"op":"set_tracking_camera","object":0,"binding":binding},"expectedRevision":imported["revision"]})).unwrap();
    engine.save().unwrap();
    let before = engine.state().unwrap();
    let disk = fs::read(root.join("project.json")).unwrap();
    let fresh = fixtures.join("video-delayed.mp4");
    let fresh_solve = solve(&fresh, "video-delayed.mp4", 0.5);
    let fresh_document = directory.join("new.mstrack");
    let original = encode::to_vec(&fresh_solve).unwrap();
    fs::write(&fresh_document, &original).unwrap();
    binding.source_anchor_pts = fresh_solve.source_pts[0];
    let prepare = json!({"action":"prepare","requestId":"rpc-native","target":{"kind":"video","id":1},"path":fresh,
        "expectedRevision":before["revision"],"expectedDocumentGeneration":before["document_generation"],"withAudio":false});
    let finish = json!({"action":"finish","requestId":"rpc-native","target":{"kind":"video","id":1},
        "expectedRevision":before["revision"],"expectedDocumentGeneration":before["document_generation"],
        "solves":[{"asset":asset,"path":fresh_document}],"bindings":[{"kind":"camera","composition":project.composition_id,"object":0,"binding":binding}]});
    for request in [
        prepare.clone(),
        finish.clone(),
        json!({"action":"cancel","requestId":"rpc-native"}),
        json!({"action":"release","requestId":"rpc-native"}),
    ] {
        assert!(read
            .call("tracked_video_replacement", request)
            .unwrap_err()
            .contains("disabled"));
    }
    for (field, state_field) in [
        ("expectedRevision", "revision"),
        ("expectedDocumentGeneration", "document_generation"),
    ] {
        let mut bad = prepare.clone();
        bad[field] = json!(before[state_field].as_u64().unwrap() + 1);
        assert!(edit
            .call("tracked_video_replacement", bad)
            .unwrap_err()
            .contains("superseded"));
    }
    let mut missing = prepare.clone();
    missing["target"]["id"] = json!(999);
    assert!(edit.call("tracked_video_replacement", missing).is_err());
    assert!(read
        .call(
            "tracked_video_replacement",
            json!({"action":"status","requestId":"rpc-native"})
        )
        .unwrap_err()
        .contains("video task missing"));
    edit.call("tracked_video_replacement", prepare.clone())
        .unwrap();
    let began = Instant::now();
    loop {
        let status = read
            .call(
                "tracked_video_replacement",
                json!({"action":"status","requestId":"rpc-native"}),
            )
            .unwrap();
        if status["state"] == "ready" {
            break;
        }
        assert_eq!(status["state"], "running", "{status}");
        assert!(began.elapsed() < Duration::from_secs(30));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(fs::read(root.join("project.json")).unwrap(), disk);
    for field in ["expectedRevision", "expectedDocumentGeneration"] {
        let mut bad = finish.clone();
        bad[field] = json!(u64::MAX);
        assert!(edit
            .call("tracked_video_replacement", bad)
            .unwrap_err()
            .contains("superseded"));
    }
    let mut bad = finish.clone();
    bad["solves"][0]["path"] = json!(old_document);
    assert!(edit
        .call("tracked_video_replacement", bad)
        .unwrap_err()
        .contains("tracking source content differs"));
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    let result = edit
        .call("tracked_video_replacement", finish.clone())
        .unwrap();
    assert_eq!(
        result["state"]["revision"].as_u64().unwrap(),
        before["revision"].as_u64().unwrap() + 1
    );
    let changed = &result["state"]["project"];
    assert_eq!(changed["video_assets"][0]["id"], 1);
    assert_eq!(changed["tracking_assets"][0]["id"], asset);
    let saved = motion_core::storage::load(&root).unwrap();
    assert_eq!(
        saved,
        serde_json::from_value::<motion_core::Project>(changed.clone()).unwrap()
    );
    let managed = root.join(changed["tracking_assets"][0]["path"].as_str().unwrap());
    assert_eq!(fs::read(managed).unwrap(), original);
    let undone = edit
        .call("motion_history", json!({"operation":"undo"}))
        .unwrap();
    assert_eq!(undone["project"], before["project"]);
    let redone = edit
        .call("motion_history", json!({"operation":"redo"}))
        .unwrap();
    assert_eq!(redone["project"], *changed);
    edit.call(
        "tracked_video_replacement",
        json!({"action":"release","requestId":"rpc-native"}),
    )
    .unwrap();
    let mut cancel = prepare.clone();
    cancel["requestId"] = json!("rpc-cancel");
    cancel["expectedRevision"] = redone["revision"].clone();
    edit.call("tracked_video_replacement", cancel).unwrap();
    edit.call(
        "tracked_video_replacement",
        json!({"action":"cancel","requestId":"rpc-cancel"}),
    )
    .unwrap();
    assert_eq!(
        read.call(
            "tracked_video_replacement",
            json!({"action":"status","requestId":"rpc-cancel"})
        )
        .unwrap()["state"],
        "cancelled"
    );
    edit.call(
        "tracked_video_replacement",
        json!({"action":"release","requestId":"rpc-cancel"}),
    )
    .unwrap();
    assert_eq!(engine.state().unwrap()["project"], *changed);
    // Leave a valid original project for the separate real-stdio acceptance run.
    edit.call("motion_history", json!({"operation":"undo"}))
        .unwrap();
    engine.save().unwrap();
    if observation.is_some() {
        fs::write(directory.join("rpc-fixture.json"),serde_json::to_vec_pretty(&json!({"project":root,"prepare":prepare,"finish":finish,"initialProject":before["project"]})).unwrap()).unwrap();
    }
    drop(engine);
}
