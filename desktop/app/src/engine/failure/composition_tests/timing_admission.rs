use super::*;

#[test]
fn timing_refusal_preserves_cause_and_reaches_default_file_and_stderr() {
    const CHILD: &str = "MOTION_TIMING_ADMISSION_LOG_CHILD";
    const NAME: &str = "engine::failure::composition_tests::timing_admission::timing_refusal_preserves_cause_and_reaches_default_file_and_stderr";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(PathBuf::from(root)).unwrap();
        let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 1152 - 1).unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let renderer = pollster::block_on(motion_render::Renderer::new_profiled_with_allocation_budget(
            &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, true, budget.clone(),
        )).unwrap();
        let failure = motion_render::GpuTimer::new(&renderer).err()
            .expect("supported timing must not hide budget refusal as unsupported");
        assert_eq!(failure.resource_rejection(), Some(&motion_render::ResourceRejection::Budget {
            scope: motion_render::ResourceBudgetScope::RenderAllocations,
            required_bytes: SYSTEM_BYTES + 1152, budget_bytes: SYSTEM_BYTES + 1151,
        }));
        assert_eq!(budget.info().unwrap().admitted_bytes, SYSTEM_BYTES);
        let original = failure.to_string();
        let (reply, receive) = channel::<Result<Value, String>>();
        CURRENT_COMMAND.set(Some(Flight {
            request: 207, operation: "test_gpu_timer", started: Instant::now(),
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
    let error = file.lines().find(|line| line.contains("error #207 test_gpu_timer ")).unwrap();
    for context in [cause, "buffer size=32 phase=allocate label=Motion Studio timing words", "required_bytes=1345744", "budget_bytes=1345743", "ms:"] {
        assert!(error.contains(context), "missing {context}: {error}");
    }
    assert!(file.contains("logging initialized level=INFO"));
    assert!(stderr.contains(error.split("motion_studio::engine: ").nth(1).unwrap()));
    assert_eq!(stderr.matches(cause).count(), 1);
    assert!(!stderr.contains("logging initialized"));
    for marker in [cause, "error #207", "logging initialized"] {
        assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
    }
}
