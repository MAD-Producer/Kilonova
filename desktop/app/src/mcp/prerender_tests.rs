//! Public application-policy routing, independent of project editing/history.
use super::*;
mod disk_cache;

#[test]
fn prerender_policy_schema_permissions_and_bounds_preserve_the_authored_project() {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let edit = ToolRouter {
        engine: &engine,
        access: Access::Edit,
    };
    let read = ToolRouter {
        engine: &engine,
        access: Access::ReadOnly,
    };
    let before = engine.state().unwrap();
    assert_eq!(before["desktop"]["prerender"]["enabled"], false);
    assert!(read
        .call("configure_prerender", json!({"enabled":true}))
        .unwrap_err()
        .contains("disabled"));
    for arguments in [
        json!({}),
        json!({"enabled":1}),
        json!({"enabled":true,"window":-1}),
        json!({"enabled":true,"window":9}),
        json!({"enabled":true,"window":1.5}),
        json!({"enabled":true,"idleDelayMs":60001}),
        json!({"enabled":true,"idleDelayMs":null}),
        json!({"enabled":true,"unknown":0}),
    ] {
        assert!(
            edit.call("configure_prerender", arguments.clone()).is_err(),
            "{arguments}"
        );
        assert_eq!(
            engine.state().unwrap()["desktop"]["prerender"]["enabled"],
            false
        );
    }
    let configured = edit
        .call(
            "configure_prerender",
            json!({"enabled":true,"window":8,"idleDelayMs":60000}),
        )
        .unwrap();
    assert_eq!(configured["enabled"], true);
    assert_eq!(configured["policy"]["window"], 8);
    assert_eq!(configured["policy"]["idleDelayMs"], 60000);
    assert_eq!(configured["policy"]["maxSeekFramesPerSecond"], 120.);
    assert!(
        engine.prerender_observation().unwrap().is_none(),
        "headless enable must not start a native owner"
    );
    assert!(edit
        .call("configure_prerender", json!({"enabled":false,"window":9}))
        .is_err());
    assert_eq!(
        engine.state().unwrap()["desktop"]["prerender"]["policy"],
        configured["policy"]
    );
    let zero = edit
        .call("configure_prerender", json!({"enabled":true,"window":0}))
        .unwrap();
    assert_eq!(zero["enabled"], false);
    assert_eq!(zero["policy"]["enabled"], true);
    assert_eq!(zero["policy"]["window"], 0);
    let disabled = edit
        .call("configure_prerender", json!({"enabled":false}))
        .unwrap();
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["policy"]["window"], 3);
    assert_eq!(disabled["policy"]["idleDelayMs"], 200);
    let after = engine.state().unwrap();
    for key in [
        "project",
        "revision",
        "canUndo",
        "canRedo",
        "frame",
        "document_generation",
    ] {
        assert!(before.get(key).is_some(), "missing state field: {key}");
        assert_eq!(after[key], before[key], "policy must preserve {key}");
    }
    let catalogue = tools();
    let tool = catalogue["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "configure_prerender")
        .unwrap();
    assert_eq!(tool["inputSchema"]["required"], json!(["enabled"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["inputSchema"]["properties"]["window"]["maximum"], 8);
    assert_eq!(tool["annotations"]["readOnlyHint"], false);
}

#[test]
fn prerender_json_rpc_configures_the_shared_engine_and_keeps_stdout_json_only() {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let before = engine.state().unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":PROTOCOL,"capabilities":{},"clientInfo":{"name":"prerender-control","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"configure_prerender","arguments":{"enabled":true,"window":2,"idleDelayMs":0}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"configure_prerender","arguments":{"enabled":false,"window":99}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"motion_state","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"configure_prerender","arguments":{"enabled":false}}}),
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"motion_state","arguments":{}}}),
    ];
    let input = requests
        .iter()
        .map(|request| request.to_string() + "\n")
        .collect::<String>();
    let notifications = std::cell::Cell::new(0);
    let notify = || notifications.set(notifications.get() + 1);
    let mut bytes = Vec::new();
    run(
        &engine,
        Access::Edit,
        &notify,
        &mut io::Cursor::new(input),
        &mut bytes,
    )
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let replies = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(replies.len(), 7);
    for (index, reply) in replies.iter().enumerate() {
        assert_eq!(reply["jsonrpc"], "2.0");
        assert_eq!(reply["id"], index + 1);
    }
    assert!(replies[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "configure_prerender"));
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(
        replies[4]["result"]["structuredContent"]["desktop"]["prerender"]["enabled"],
        true
    );
    assert_eq!(replies[5]["result"]["isError"], false);
    assert_eq!(
        replies[6]["result"]["structuredContent"]["desktop"]["prerender"]["enabled"],
        false
    );
    assert_eq!(notifications.get(), 2);
    assert_eq!(engine.state().unwrap()["project"], before["project"]);
    assert_eq!(engine.state().unwrap()["revision"], before["revision"]);
    assert!(engine.prerender_observation().unwrap().is_none());
}
