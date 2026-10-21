//! Preserve a failed chrome upload across the host's infallible overlay callback.
pub(super) fn finish(
    result: Result<bool, String>,
    error: Option<motion_ui::paint::PaintError>,
    window: u64,
) -> Result<bool, String> {
    match (result, error) {
        (result, None) => result,
        (Ok(_), Some(error)) => Err(format!("present window={window}: {error}")),
        (Err(primary), Some(error)) => {
            Err(format!("{primary}; UI overlay window={window}: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{respond, Flight, CURRENT_COMMAND};
    use motion_ui::{paint, Color, Painter, Rect};

    #[test]
    fn real_paint_limit_failure_reaches_reply_file_and_stderr_at_default_level() {
        const CHILD: &str = "MOTION_UI_BUFFER_LIMIT_CHILD";
        const TEST: &str = "engine::ui_paint::tests::real_paint_limit_failure_reaches_reply_file_and_stderr_at_default_level";
        if let Some(root) = std::env::var_os(CHILD) {
            let _logger = crate::logging::init(root.into()).unwrap();
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
            let adapter =
                pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
            let mut descriptor = wgpu::DeviceDescriptor::default();
            descriptor.required_limits.max_buffer_size = 5_767_168;
            let (device, queue) = pollster::block_on(adapter.request_device(&descriptor)).unwrap();
            let mut painter = Painter::new(&device, wgpu::TextureFormat::Rgba8Unorm);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let mut list = paint::PaintList::default();
            for _ in 0..11_092 {
                paint::rect(&mut list, Rect::new(0., 0., 8., 8.), Color([1.; 4]));
            }
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut encoder = device.create_command_encoder(&Default::default());
            let error = painter
                .render(&device, &queue, &mut encoder, &view, &list, [8.; 2])
                .unwrap_err();
            assert_eq!(error.required_bytes, 5_856_576);
            assert!(pollster::block_on(scope.pop()).is_none());
            assert_eq!(finish(Ok(false), None, 7), Ok(false));
            assert_eq!(finish(Ok(true), None, 7), Ok(true));
            assert_eq!(
                finish(Err("primary".into()), None, 7),
                Err("primary".into())
            );
            // The host overlay callback returns (), so preserve its actual error
            // after either a successful or degraded host presentation.
            for (request, host_result) in [
                (901, Ok(true)),
                (902, Ok(false)),
                (903, Err("original surface failure".into())),
            ] {
                let result = finish(host_result, Some(error.clone()), 7);
                let original = result.as_ref().unwrap_err().clone();
                if request == 903 {
                    assert!(original.starts_with("original surface failure; UI overlay"));
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
            "kind=solid",
            "vertices=66552",
            "required_bytes=5856576",
            "limit_bytes=5767168",
            "window=7",
            "original surface failure",
        ] {
            assert!(file.contains(marker), "file missing {marker}");
            assert!(stderr.contains(marker), "stderr missing {marker}");
            assert!(!stdout.contains(marker), "stdout leaked {marker}");
        }
        for request in [901, 902, 903] {
            let marker = format!("error #{request} present ");
            assert_eq!(file.matches(&marker).count(), 1);
            assert_eq!(stderr.matches(&marker).count(), 1);
        }
    }
}
