use super::*;

#[test]
fn timing_refusal_preserves_cause_and_reaches_default_file_and_stderr() {
    check_timing_channels(true,
        "engine::failure::composition_tests::timing_admission::timing_refusal_preserves_cause_and_reaches_default_file_and_stderr");
}

#[test]
fn unsupported_timing_is_quiet_and_preserves_default_output_channels() {
    check_timing_channels(false,
        "engine::failure::composition_tests::timing_admission::unsupported_timing_is_quiet_and_preserves_default_output_channels");
}

fn check_timing_channels(timestamps: bool, name: &str) {
    const CHILD: &str = "MOTION_TIMING_ADMISSION_LOG_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let budget = motion_render::AllocationBudget::new(SYSTEM_BYTES + 1152 - 1).unwrap();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let renderer = pollster::block_on(motion_render::Renderer::new_profiled_with_allocation_budget(
            &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, timestamps, budget.clone(),
        )).unwrap();
        let before = budget.info().unwrap();
        let period = renderer.queue.get_timestamp_period();
        let supported = renderer.device.features().contains(wgpu::Features::TIMESTAMP_QUERY)
            && period.is_finite() && period > 0.;
        let result = motion_render::GpuTimer::new(&renderer);
        if !supported {
            // Optional timestamp support is a normal absence, not a failed
            // allocation. Exercise this path deliberately with timestamps=false
            // on every platform, as well as on genuinely unsupported adapters.
            assert!(result.unwrap().is_none());
            assert_eq!(budget.info().unwrap(), before);
            assert!(renderer.gpu_error().is_none());
            std::fs::write(root.join("timing-unsupported"), "unsupported; capacity unchanged").unwrap();
            return;
        }
        assert!(timestamps, "disabled timestamp requests must not enable the feature");
        let failure = result.err()
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
        .args(["--exact", name]).env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let cause = "admitted render allocations exceed configured combined budget";
    if root.path().join("timing-unsupported").exists() {
        assert_eq!(std::fs::read_to_string(root.path().join("timing-unsupported")).unwrap(),
            "unsupported; capacity unchanged");
        assert!(file.contains("logging initialized level=INFO"));
        for marker in [cause, "error #207", "test_gpu_timer"] {
            assert!(!file.contains(marker), "unsupported timing logged a failure: {file}");
            assert!(!stderr.contains(marker), "unsupported timing wrote an error: {stderr}");
            assert!(!stdout.contains(marker), "log output leaked to stdout: {stdout}");
        }
        assert!(!stderr.contains("logging initialized"));
        assert!(!stdout.contains("logging initialized"));
        return;
    }
    assert!(timestamps, "disabled timestamp test must exercise the unsupported path");
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
