use super::*;
use std::{fs::File, io::Write, sync::Mutex};

struct Capture(Mutex<File>);
impl log::Log for Capture {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata())
            && record
                .args()
                .to_string()
                .contains("render.timestamp.readback")
        {
            let text = format!("{} {}", record.level(), record.args());
            writeln!(self.0.lock().unwrap(), "{text}").unwrap();
            if record.level() == log::Level::Error {
                eprintln!("{text}");
            }
        }
    }
    fn flush(&self) {
        self.0.lock().unwrap().flush().unwrap();
    }
}

#[test]
fn actual_unmapped_timestamp_errors_are_bounded_and_reset_after_a_valid_mapping() {
    const CHILD: &str = "MOTION_TIMESTAMP_MAP_LOG_CHILD";
    if let Some(path) = std::env::var_os(CHILD) {
        log::set_logger(Box::leak(Box::new(Capture(Mutex::new(
            File::create(&path).unwrap(),
        )))))
        .unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let renderer = pollster::block_on(crate::Renderer::new_profiled(
            &instance, None, wgpu::TextureFormat::Rgba8UnormSrgb, true,
        )).unwrap();
        let Some(mut timer) = GpuTimer::new(&renderer.device, &renderer.queue) else {
            panic!("timestamp queries are required for this controlled GPU test");
        };
        let fail = |timer: &mut GpuTimer, count| {
            for sequence in 0..count {
                // The real buffer is deliberately unmapped, while the ready
                // flag requests a read. get_mapped_range must produce its
                // actual wgpu error rather than a fabricated test result.
                timer.slots[0].sequence = sequence;
                timer.slots[0].state.store(2, Ordering::Release);
                assert!(timer.collect().iter().all(Option::is_none));
            }
        };
        fail(&mut timer, 120);
        let first = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            first.matches("ERROR persistent degradation").count(),
            1,
            "{first}"
        );
        assert!(
            first.contains("count=60 slot=0 sequence=59 cause="),
            "{first}"
        );
        let values = [11u64, 22, 33, 44]
            .into_iter()
            .flat_map(u64::to_le_bytes)
            .collect::<Vec<_>>();
        renderer
            .queue
            .write_buffer(&timer.slots[0].buffer, 0, &values);
        renderer.queue.submit([]);
        timer.map(0);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert!(timer.collect()[0].is_some());
        fail(&mut timer, 59);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), first);
        fail(&mut timer, 1);
        assert_eq!(
            std::fs::read_to_string(&path)
                .unwrap()
                .matches("ERROR persistent degradation")
                .count(),
            2
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("timestamp-map.log");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "timing::observation_tests::actual_unmapped_timestamp_errors_are_bounded_and_reset_after_a_valid_mapping", "--nocapture"])
        .env(CHILD, &path).env("MOTION_LOG", "info").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .matches("ERROR persistent degradation site=render.timestamp.readback")
            .count(),
        2,
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("persistent degradation"));
}
