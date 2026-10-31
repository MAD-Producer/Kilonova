use super::*;

#[test]
fn disk_policy_is_optional_bounded_read_only_guarded_and_does_no_headless_io() {
    const CHILD: &str = "MOTION_PRERENDER_DISK_POLICY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let profile = tempfile::tempdir().unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "mcp::prerender_tests::disk_cache::disk_policy_is_optional_bounded_read_only_guarded_and_does_no_headless_io",
                "--exact",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env("LOCALAPPDATA", profile.path())
            .env("HOME", profile.path())
            .env("XDG_DATA_HOME", profile.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed; 0 ignored"));
        return;
    }
    let cache_directory = crate::directories().join("cache/prerender");
    assert!(!cache_directory.exists());
    let temp = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        temp.path().join("project"),
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
    assert!(read
        .call(
            "configure_prerender",
            json!({"enabled":true,"diskQuotaBytes":1048576})
        )
        .is_err());
    for quota in [
        json!(-1),
        json!(1),
        json!(80),
        json!(68719476737u64),
        json!(1.5),
        json!(null),
        json!("1048576"),
    ] {
        assert!(edit
            .call(
                "configure_prerender",
                json!({"enabled":true,"diskQuotaBytes":quota})
            )
            .is_err());
        assert_eq!(
            engine.state().unwrap()["desktop"]["prerender"]["enabled"],
            false
        );
    }
    let state = edit
        .call(
            "configure_prerender",
            json!({"enabled":true,"diskQuotaBytes":1048576}),
        )
        .unwrap();
    assert_eq!(
        state["diskCache"], false,
        "headless configuration is not an opened disk backend"
    );
    assert_eq!(state["diskConfigured"], true);
    assert_eq!(state["diskQuotaBytes"], 1048576);
    assert!(
        engine.prerender_observation().unwrap().is_none(),
        "headless policy must not open a disk/GPU owner"
    );
    assert!(
        !cache_directory.exists(),
        "headless configuration performed disk IO"
    );
    let reset = edit
        .call("configure_prerender", json!({"enabled":true}))
        .unwrap();
    assert_eq!(reset["diskCache"], false);
    assert_eq!(reset["diskConfigured"], false);
    let after = engine.state().unwrap();
    for key in [
        "project",
        "revision",
        "canUndo",
        "canRedo",
        "frame",
        "document_generation",
    ] {
        assert_eq!(before[key], after[key], "cache policy must preserve {key}");
    }
    let catalogue = tools();
    let tool = catalogue["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "configure_prerender")
        .unwrap();
    assert_eq!(
        tool["inputSchema"]["properties"]["diskQuotaBytes"]["maximum"],
        68719476736u64
    );
}
