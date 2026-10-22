use super::*;

#[test]
fn actual_package_install_refusal_reaches_default_worker_reply_file_and_stderr_then_recovers() {
    const CHILD: &str = "MOTION_PACKAGE_DECODE_BUDGET_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::package_budget::actual_package_install_refusal_reaches_default_worker_reply_file_and_stderr_then_recovers";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let package = motion_effects::builtin::package().unwrap();
        assert!(package
            .manifest
            .effects
            .iter()
            .any(|effect| !effect.resources.is_empty()));
        let path = root.join("package.msfx");
        std::fs::write(&path, &package.bytes).unwrap();
        std::fs::create_dir(root.join("project")).unwrap();
        let engine = Engine::start(
            root.join("project"),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        let original = motion_render::resource_policy::MemoryPolicy::default();
        let low = motion_render::resource_policy::MemoryPolicy {
            decoded_image_budget_bytes: 4,
            ..original
        };
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":low}).to_string())
            .unwrap();
        let before = engine.state().unwrap();
        let request = serde_json::json!({"op":"install","path":path}).to_string();
        let failure = engine
            .plugin_async(request.clone())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        for context in [
            "effect PNG resources exceed configured decoded image budget",
            "package=",
            "resource=",
            "required_bytes=",
            "budget_bytes=4",
        ] {
            assert!(failure.contains(context), "{failure}");
        }
        let after = engine.state().unwrap();
        for key in ["project", "revision", "canUndo"] {
            assert_eq!(after.get(key).unwrap(), before.get(key).unwrap());
        }
        engine
            .plugin(serde_json::json!({"op":"memory_policy","policy":original}).to_string())
            .unwrap();
        engine.plugin(request).unwrap();
        let recovered = engine.state().unwrap();
        for key in ["project", "revision", "canUndo"] {
            assert_eq!(recovered.get(key).unwrap(), before.get(key).unwrap());
        }
        assert_eq!(std::fs::read(path).unwrap(), package.bytes);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "effect PNG resources exceed configured decoded image budget";
    let errors: Vec<_> = file
        .lines()
        .filter(|line| line.contains(cause) && line.contains("ERROR"))
        .collect();
    assert_eq!(errors.len(), 1, "{file}");
    assert!(file
        .lines()
        .any(|line| line.contains("INFO") && line.contains("start #") && line.contains("plugin")));
    for context in ["error #", "plugin", "ms:", "budget_bytes=4"] {
        assert!(errors[0].contains(context), "{}", errors[0]);
    }
    assert_eq!(stderr.matches(cause).count(), 1, "{stderr}");
    assert!(!stdout.contains(cause), "{stdout}");
    assert!(
        !stdout.contains(" INFO ") && !stdout.contains(" ERROR "),
        "{stdout}"
    );
}
