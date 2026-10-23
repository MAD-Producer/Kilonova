use super::finish;
use crate::engine::{respond, Flight, CURRENT_COMMAND};

trait NativeRejection {
    fn resource_rejection(&self) -> Option<&motion_render::ResourceRejection>;
}
impl NativeRejection for motion_ui::paint::PaintError {
    fn resource_rejection(&self) -> Option<&motion_render::ResourceRejection> {
        std::error::Error::source(self)?
            .downcast_ref::<motion_render::RenderError>()?.resource_rejection()
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout() {
    const CHILD: &str = "MOTION_UI_ADMISSION_CHILD";
    const TEST: &str = "engine::ui_paint::admission_tests::native_ui_refusals_preserve_cause_and_reach_default_file_stderr_without_stdout";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(root.into()).unwrap();
        let renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
        let context = renderer.allocation_context();
        let budget = context.budget().clone();
        let before = budget.info().unwrap();
        budget.set_limit(before.admitted_bytes + 39).unwrap();
        let startup =
            motion_ui::Painter::new_admitted(super::allocation::context(context.clone()), wgpu::TextureFormat::Rgba8UnormSrgb)
                .err()
                .unwrap();
        assert!(startup.resource_rejection().is_some());
        assert!(std::error::Error::source(&startup)
            .unwrap()
            .downcast_ref::<motion_render::RenderError>()
            .is_some());
        assert_eq!(budget.info().unwrap().admitted_bytes, before.admitted_bytes);
        budget.set_limit(before.budget_bytes).unwrap();
        let mut painter =
            motion_ui::Painter::new_admitted(super::allocation::context(context.clone()), wgpu::TextureFormat::Rgba8UnormSrgb)
                .unwrap();
        let occupied = budget.info().unwrap().admitted_bytes;
        budget.set_limit(occupied + 3).unwrap();
        let atlas = painter
            .upload_atlas(&renderer.device, &renderer.queue, &[255; 4], 1, 1)
            .unwrap_err();
        assert_eq!(
            atlas.resource_rejection(),
            Some(&motion_render::ResourceRejection::Budget {
                scope: motion_render::ResourceBudgetScope::RenderAllocations,
                required_bytes: occupied + 4,
                budget_bytes: occupied + 3,
            })
        );
        assert_eq!(budget.info().unwrap().admitted_bytes, occupied);
        for (request, result, failure) in [
            (904, Ok(true), startup),
            (905, Ok(false), atlas.clone()),
            (906, Err("original host failure".into()), atlas),
        ] {
            let result = finish(result, Some(failure), 7);
            let original = result.as_ref().unwrap_err().clone();
            if request == 906 {
                assert!(original.starts_with("original host failure; UI overlay"));
            }
            let (reply, receive) = std::sync::mpsc::channel();
            CURRENT_COMMAND.set(Some(Flight {
                request,
                operation: "present",
                started: std::time::Instant::now(),
            }));
            respond(reply, result.map_err(motion_host::Error::from));
            CURRENT_COMMAND.set(None);
            assert_eq!(receive.recv().unwrap(), Err(original));
        }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture"])
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
    assert!(file.contains("logging initialized level=INFO"));
    for marker in [
        "UI glyph vertices",
        "UI glyph atlas",
        "render allocations",
        "window=7",
        "original host failure",
    ] {
        assert!(file.contains(marker), "file missing {marker}");
        assert!(stderr.contains(marker), "stderr missing {marker}");
        assert!(!stdout.contains(marker), "stdout leaked {marker}");
    }
    for request in [904, 905, 906] {
        let marker = format!("error #{request} present ");
        assert_eq!(file.matches(&marker).count(), 1);
        assert_eq!(stderr.matches(&marker).count(), 1);
    }
}
