use super::*;
use crate::workflow::tracked_video::Request;

#[test]
fn action_contract_is_strict_bounded_and_keeps_null_bindings_explicit() {
    let prepare = json!({"action":"prepare","requestId":"new-plate","target":{"kind":"video","id":1},
        "path":"new.mp4","expectedRevision":0,"expectedDocumentGeneration":1,"withAudio":true});
    let finish = json!({"action":"finish","requestId":"new-plate","target":{"kind":"video","id":1},
        "expectedRevision":0,"expectedDocumentGeneration":1,"solves":[{"asset":2,"path":"new.mstrack"}],
        "bindings":[{"kind":"camera","composition":"main","object":0,"binding":null}]});
    let parse = |v| -> Result<Request, String> {
        let request: Request = serde_json::from_value(v).map_err(|e| e.to_string())?;
        request.validate()?;
        Ok(request)
    };
    for valid in [
        prepare.clone(),
        finish.clone(),
        json!({"action":"status","requestId":"new-plate"}),
        json!({"action":"cancel","requestId":"new-plate"}),
        json!({"action":"release","requestId":"new-plate"}),
    ] {
        let request = parse(valid).unwrap();
        assert_eq!(
            request.read_only(),
            request.operation() == "tracked_video_replacement_status"
        );
    }
    for (field, value) in [
        ("target", json!({"kind":"audio","id":1})),
        ("target", json!({"kind":"video","id":0})),
        ("target", json!({"kind":"video","id":1,"extra":true})),
        ("requestId", json!("x".repeat(129))),
        ("requestId", json!("line\nbreak")),
        ("path", json!("")),
        ("path", json!("x".repeat(4097))),
        ("path", json!("nul\0path")),
        ("expectedRevision", json!(-1)),
        ("expectedDocumentGeneration", json!(null)),
        ("withAudio", json!(1)),
        ("track", json!(u64::from(u32::MAX) + 1)),
        ("unknown", json!(true)),
    ] {
        let mut bad = prepare.clone();
        bad[field] = value;
        assert!(parse(bad).is_err(), "accepted {field}");
    }
    for field in [
        "target",
        "path",
        "expectedRevision",
        "expectedDocumentGeneration",
        "withAudio",
    ] {
        let mut bad = prepare.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(parse(bad).is_err(), "missing {field}");
    }
    for (field, value) in [
        ("solves", json!([])),
        ("solves", json!([{"asset":0,"path":"new"}])),
        (
            "solves",
            json!([{"asset":2,"path":"new"},{"asset":2,"path":"other"}]),
        ),
        ("solves", json!([{"asset":2,"path":"new","unknown":true}])),
        (
            "bindings",
            json!([{"kind":"camera","composition":"main","object":0}]),
        ),
    ] {
        let mut bad = finish.clone();
        bad[field] = value;
        assert!(parse(bad).is_err(), "accepted {field}");
    }
    let mut bad = finish.clone();
    bad["solves"] = json!(vec![json!({"asset":2,"path":"x"}); 257]);
    assert!(parse(bad).is_err());
    let mut bad = finish;
    bad["bindings"] = json!(vec![
        json!({"kind":"camera","composition":"main","object":0,"binding":null});
        1025
    ]);
    assert!(parse(bad).is_err());
    assert!(parse(json!({"action":"status","requestId":"x","path":"not-a-status-field"})).is_err());
}

#[test]
fn advertised_actions_reuse_all_three_core_binding_schemas() {
    let schema = schema();
    let actions = schema["oneOf"].as_array().unwrap();
    assert_eq!(actions.len(), 5);
    for action in actions {
        assert_eq!(action["additionalProperties"], false);
    }
    let finish = actions
        .iter()
        .find(|v| v["properties"]["action"]["const"] == "finish")
        .unwrap();
    let bindings = finish["properties"]["bindings"]["items"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(bindings.len(), 3);
    for (kind, op) in [
        ("point", "set_tracking_point"),
        ("plane", "set_tracking_plane"),
        ("camera", "set_tracking_camera"),
    ] {
        let binding = bindings
            .iter()
            .find(|v| v["properties"]["kind"]["const"] == kind)
            .unwrap();
        let core = motion_core::CommandRegistry::input_schemas()
            .into_iter()
            .find(|v| v["properties"]["op"]["const"] == op)
            .unwrap();
        assert_eq!(
            binding["properties"]["binding"],
            core["properties"]["binding"]
        );
        assert!(binding["required"]
            .as_array()
            .unwrap()
            .contains(&json!("binding")));
    }
    let tools = crate::mcp::tools();
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "tracked_video_replacement")
        .unwrap();
    assert_eq!(tool["inputSchema"], schema);
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
}

#[cfg(feature = "ffmpeg")]
#[path = "tracked_video_native_tests.rs"]
mod native;
