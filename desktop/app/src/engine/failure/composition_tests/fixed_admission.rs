use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn fixed_startup_refusal_preserves_cause_and_reaches_default_file_and_stderr() {
    const CHILD: &str = "MOTION_FIXED_STARTUP_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::fixed_admission::fixed_startup_refusal_preserves_cause_and_reaches_default_file_and_stderr";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES - 1).unwrap();
        let failure = pollster::block_on(motion_render::Renderer::headless_with_allocation_budget(budget.clone())).err().unwrap();
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: SYSTEM_BYTES, budget_bytes: SYSTEM_BYTES - 1,
        }));
        assert_eq!(budget.info().unwrap().admitted_bytes, 0);
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 206, operation: "test_renderer_start", started: Instant::now(),
        }));
        respond(reply, Err(failure.into()));
        CURRENT_COMMAND.set(None);
        assert_eq!(receive.recv_timeout(Duration::from_secs(5)).unwrap().unwrap_err(), original);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME]).env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    let error = file.lines().find(|line| line.contains("error #206 test_renderer_start ")).unwrap();
    for context in [cause, "effect identity LUT size=256x1 phase=allocate", "required_bytes=1344592", "budget_bytes=1344591", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #206", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
