use super::*;
use motion_host::video_frame::{DecodedFrame, VideoPixels};
use motion_render::{AllocationBudget, ChromaLayout, RenderError, Renderer, Yuv420Frame};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn video_frame_refusals_keep_native_causes_and_default_output_channels() {
    const CHILD: &str = "MOTION_VIDEO_FRAME_LOG_CHILD";
    const NAME: &str = "engine::failure::video_frame_tests::video_frame_refusals_keep_native_causes_and_default_output_channels";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let budget =
            AllocationBudget::new(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET).unwrap();
        let mut renderer =
            pollster::block_on(Renderer::headless_with_allocation_budget(budget.clone())).unwrap();
        #[cfg(target_os = "windows")]
        assert_eq!(renderer.adapter_info.backend, wgpu::Backend::Dx12);
        let baseline = budget.info().unwrap();
        budget.set_limit(baseline.admitted_bytes).unwrap();
        let mut decoded = DecodedFrame {
            pixels: VideoPixels::Rgba([20, 40, 60, 255].repeat(4)),
            pts: 100,
            end: 200,
            width: 2,
            height: 2,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "authored-fixture",
            decoder_name: "private-fixture-decoder".into(),
        };
        let allocation = decoded.upload(&mut renderer, 71, 83).unwrap_err();
        assert!(
            matches!(&allocation, motion_host::Error::Render(error) if error.resource_rejection().is_some())
        );
        assert_eq!(
            budget.info().unwrap().admitted_bytes,
            baseline.admitted_bytes
        );
        budget
            .set_limit(motion_render::DEFAULT_RENDER_ALLOCATION_BUDGET)
            .unwrap();
        decoded.upload(&mut renderer, 71, 83).unwrap();
        assert_eq!(renderer.video_uploads, 1);
        decoded.pixels = VideoPixels::Yuv(Yuv420Frame {
            width: 2,
            height: 2,
            rotation: 0,
            standard: 1,
            range: 2,
            phase: [0; 2],
            y: vec![16; 4],
            uv: vec![],
            chroma_layout: ChromaLayout::Uv,
        });
        let invalid = decoded.rgba().unwrap_err();
        assert!(
            matches!(&invalid, motion_host::Error::Render(RenderError::Invalid(cause))
            if cause == "YUV frame plane length mismatch")
        );
        let mut causes = Vec::new();
        for (request, error) in [(611, allocation), (612, invalid)] {
            let original = error.to_string();
            let (reply, receive) = channel::<Result<Value, String>>();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "test_video_frame",
                started: Instant::now(),
            }));
            respond(reply, Err(error));
            CURRENT_COMMAND.set(None);
            assert_eq!(
                receive
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .unwrap_err(),
                original
            );
            causes.push(original);
        }
        decoded.pixels = VideoPixels::Rgba([20, 40, 60, 255].repeat(4));
        assert_eq!(
            decoded.rgba().unwrap().as_ref(),
            [20, 40, 60, 255].repeat(4)
        );
        std::fs::write(
            root.join("primary-causes.json"),
            serde_json::to_vec(&causes).unwrap(),
        )
        .unwrap();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", NAME, "--test-threads=1"])
        .env(CHILD, root.path())
        .env_remove("MOTION_LOG");
    // Select the native backend for this controlled two-error fixture.
    // Production backend selection and all logger filtering remain unchanged.
    #[cfg(target_os = "windows")]
    command.env("WGPU_BACKEND", "dx12");
    #[cfg(target_os = "linux")]
    command.env("WGPU_BACKEND", "vulkan");
    #[cfg(target_os = "macos")]
    command.env("WGPU_BACKEND", "metal");
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let causes: Vec<String> =
        serde_json::from_slice(&std::fs::read(root.path().join("primary-causes.json")).unwrap())
            .unwrap();
    assert_eq!(causes.len(), 2);
    assert!(file.contains("logging initialized level=INFO"));
    for (request, cause) in (611..613).zip(causes) {
        let marker = format!("error #{request} test_video_frame ");
        for text in [&file, &stderr] {
            let records: Vec<_> = text.lines().filter(|line| line.contains(&marker)).collect();
            assert_eq!(records.len(), 1, "{text}");
            assert!(records[0].contains(&cause));
        }
        assert!(!stdout.contains(&cause));
    }
    for text in [&file, &stderr] {
        assert_eq!(
            text.lines().filter(|line| line.contains("ERROR")).count(),
            2,
            "{text}"
        );
        assert!(!text.contains("private-fixture-decoder"));
    }
    for marker in ["logging initialized", "test_video_frame", "INFO", "ERROR"] {
        assert!(!stdout.contains(marker), "{stdout}");
    }
}
