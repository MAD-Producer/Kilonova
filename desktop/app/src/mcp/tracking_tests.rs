use super::*;
use motion_host::tracking::{asset::*, encode, plate::Extent};
use std::{fs, io::Cursor};

#[test]
fn tracking_import_checks_permissions_context_and_persists_one_history_step() {
    let observation =
        std::env::var_os("MOTION_TRACKING_IMPORT_LOG_TEST_DIR").map(std::path::PathBuf::from);
    let _logger = observation
        .as_ref()
        .map(|dir| crate::logging::init(dir.join("logs")).unwrap());
    let folder = tempfile::tempdir().unwrap();
    let root = folder.path().join("project");
    fs::create_dir_all(root.join("assets")).unwrap();
    let mut image = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        6,
        image::Rgba([19, 71, 97, 255]),
    ))
    .write_to(&mut image, image::ImageFormat::Png)
    .unwrap();
    let bytes = image.into_inner();
    fs::write(root.join("assets/original.png"), &bytes).unwrap();
    let digest = motion_core::storage::resource_sha256(&bytes);
    let mut project = motion_core::Project::new(64, 64, 30, 90).unwrap();
    project.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/original.png".into(),
        width: 8,
        height: 6,
    });
    motion_core::storage::save(&root, &project).unwrap();
    let asset = Asset {
        version: VERSION,
        source_id: "MCP original fixture".into(),
        source_sha256: std::array::from_fn(|i| {
            u8::from_str_radix(&digest[2 * i..2 * i + 2], 16).unwrap()
        }),
        source_extent: Extent::new(8, 6).unwrap(),
        coordinates: Coordinates::RightDownForward,
        ticks_per_second: 90000,
        nominal_rate: Rate::new(30, 1).unwrap(),
        source_pts: vec![0],
        meters_per_world_unit: None,
        tracks: vec![Track {
            id: "visible".into(),
            kind: Kind::Point,
            reference_index: None,
            samples: vec![Sample {
                source_index: 0,
                visibility: Visibility::Visible,
                confidence: 1.,
                observation: Some(Observation::Point([2., 3.])),
                manual_correction: true,
            }],
        }],
    };
    let original = [b" \n".as_slice(), &encode::to_vec(&asset).unwrap(), b"\n "].concat();
    let file = folder.path().join("solve.json");
    fs::write(&file, &original).unwrap();
    let engine = Engine::start(root.clone(), motion_desktop_media::platform(), 8 << 30).unwrap();
    let edit = ToolRouter {
        engine: &engine,
        access: Access::Edit,
    };
    let read = ToolRouter {
        engine: &engine,
        access: Access::ReadOnly,
    };
    let before = engine.state().unwrap();
    let args = json!({"path":file,"source":{"kind":"image","id":1},"expectedRevision":before["revision"],
        "expectedDocumentGeneration":before["document_generation"]});
    assert!(read
        .call("import_tracking", args.clone())
        .unwrap_err()
        .contains("disabled"));
    for (field, value) in [
        (
            "expectedRevision",
            json!(before["revision"].as_u64().unwrap() + 1),
        ),
        (
            "expectedDocumentGeneration",
            json!(before["document_generation"].as_u64().unwrap() + 1),
        ),
        ("source", json!({"kind":"audio","id":1})),
        ("source", json!({"kind":"image","id":0})),
        ("source", json!({"kind":"image","id":1,"extra":true})),
        ("unknown", json!(true)),
    ] {
        let mut bad = args.clone();
        bad[field] = value;
        assert!(
            edit.call("import_tracking", bad).is_err(),
            "accepted invalid {field}"
        );
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
    assert!(!root.join("assets/tracking").exists());
    let imported = edit.call("import_tracking", args.clone()).unwrap();
    assert_eq!(imported["trackingImport"]["encodedSourceVerified"], true);
    assert_eq!(imported["trackingImport"]["decodedSourceVerified"], true);
    assert_eq!(imported["trackingImport"]["decodedTimelineVerified"], false);
    assert_eq!(imported["trackingImport"]["poseApplied"], false);
    let attachment = &imported["project"]["tracking_assets"][0];
    let managed = root.join(attachment["path"].as_str().unwrap());
    assert_eq!(fs::read(&managed).unwrap(), original);
    assert_eq!(
        motion_core::storage::load(&root)
            .unwrap()
            .tracking_assets
            .len(),
        1
    );
    assert!(edit.call("import_tracking", args).is_err());
    let undone = edit
        .call("motion_history", json!({"operation":"undo"}))
        .unwrap();
    assert_eq!(undone["project"], before["project"]);
    assert_eq!(undone["canUndo"], false);
    fs::remove_file(file).unwrap();
    let redone = edit
        .call("motion_history", json!({"operation":"redo"}))
        .unwrap();
    assert_eq!(redone["project"], imported["project"]);
    assert_eq!(fs::read(managed).unwrap(), original);
    edit.call("motion_save", json!({})).unwrap();
    assert_eq!(
        motion_core::storage::load(&root)
            .unwrap()
            .tracking_assets
            .len(),
        1
    );
    let state = engine.state().unwrap();
    let rpc = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,
            "capabilities":{},"clientInfo":{"name":"tracking-import-test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"import_tracking","arguments":{
            "path":folder.path().join("missing-original.json"),"source":{"kind":"image","id":1},
            "expectedRevision":state["revision"],"expectedDocumentGeneration":state["document_generation"]}}}),
    ].iter().map(|v| format!("{v}\n")).collect::<String>();
    let mut wire = Vec::new();
    run(
        &engine,
        Access::Edit,
        &|| {},
        &mut Cursor::new(rpc),
        &mut wire,
    )
    .unwrap();
    let replies: Vec<Value> = std::str::from_utf8(&wire)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[1]["result"]["isError"], true);
    assert!(replies[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("open_document"));
    assert_eq!(engine.state().unwrap()["project"], state["project"]);
    if let Some(dir) = observation {
        fs::write(dir.join("protocol.jsonl"), wire).unwrap();
    }
}

#[test]
fn default_tracking_import_logs_start_and_error_without_polluting_rpc() {
    let evidence = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "mcp::tracking_tests::tracking_import_checks_permissions_context_and_persists_one_history_step", "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG").env("MOTION_TRACKING_IMPORT_LOG_TEST_DIR", evidence.path()).output().unwrap();
    assert!(
        output.status.success(),
        "child={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(evidence.path().join("logs/motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        log.contains(" INFO ") && log.contains("import_tracking"),
        "{log}"
    );
    assert!(log.contains("start #"), "{log}");
    assert!(
        log.contains("tracking import start site=host.tracking.import request="),
        "{log}"
    );
    assert!(
        !log.lines().any(|line| line.contains(" ERROR ")
            && line.contains("tracking import superseded by a project edit")),
        "{log}"
    );
    assert!(
        !stderr.contains("tracking import superseded by a project edit"),
        "{stderr}"
    );
    for sink in [log.as_str(), stderr.as_ref()] {
        assert!(
            sink.contains("tracking import failed request=")
                && sink.contains("phase=open_document"),
            "{sink}"
        );
        assert!(
            sink.contains("source=Image:1") && sink.contains("elapsed_ms="),
            "{sink}"
        );
    }
    let wire = fs::read_to_string(evidence.path().join("protocol.jsonl")).unwrap();
    for line in wire.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["jsonrpc"], "2.0");
    }
    assert!(!wire.contains("tracking import start"));
}

#[test]
fn tracking_tool_declares_required_context_and_explicit_incomplete_scope() {
    let tools = tools();
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "import_tracking")
        .unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
    assert_eq!(
        tool["inputSchema"]["required"],
        json!([
            "path",
            "source",
            "expectedRevision",
            "expectedDocumentGeneration"
        ])
    );
    assert_eq!(
        tool["inputSchema"]["properties"]["source"]["additionalProperties"],
        false
    );
    assert!(tool["description"]
        .as_str()
        .unwrap()
        .contains("does not apply poses"));
}
