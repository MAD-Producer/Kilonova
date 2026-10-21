use super::*;

#[test]
fn blend_uniform_refusal_preserves_reply_and_reaches_default_file_and_stderr() {
    const CHILD: &str = "MOTION_BLEND_UNIFORM_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::frame_blend_admission::blend_uniform_refusal_preserves_reply_and_reaches_default_file_and_stderr";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 47).unwrap();
        let mut renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        renderer
            .upload_video_frame(9, 12, 345, 2, 2, &[255; 16])
            .unwrap();
        let failure = renderer
            .cache_video_blend_source(9, 12, 345, 0)
            .unwrap_err();
        assert_eq!(
            failure.resource_rejection(),
            Some(&motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::RenderAllocations,
                required_bytes: SYSTEM_BYTES + 48,
                budget_bytes: SYSTEM_BYTES + 47,
            })
        );
        assert_eq!(renderer.video_blend_submissions, 0);
        assert!(!renderer.video_blend_source_ready(9, 12, 345, 0));
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 205,
            operation: "test_video_blend_cache",
            started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(
            receive
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap_err(),
            original
        );
        budget.set_limit(SYSTEM_BYTES + (1 << 20)).unwrap();
        renderer.cache_video_blend_source(9, 12, 345, 0).unwrap();
        assert!(renderer.video_blend_source_ready(9, 12, 345, 0));
        renderer.check_health().unwrap();
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
    let error = file
        .lines()
        .find(|line| line.contains("error #205 test_video_blend_cache "))
        .unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    for context in [
        cause,
        "object=9 source=12 pts_us=345 slot=0 size=2x2 phase=blend_cache",
        "buffer size=16 phase=allocate label=video frame blend weight",
        &format!("required_bytes={}", SYSTEM_BYTES + 48),
        &format!("budget_bytes={}", SYSTEM_BYTES + 47),
        "ms:",
    ] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #205", "logging initialized"] {
        assert!(
            !stdout.contains(marker),
            "log output leaked to stdout: {stdout}"
        );
    }
}
