use motion_media::{AudioJobs, ImportOptions, Limits};
use std::time::{Duration, Instant};

#[test]
fn panic_detail_keeps_string_payloads_and_bounds_unicode_without_panicking() {
    assert_eq!(motion_media::panic_reason(Box::new("specific detail")), "specific detail");
    assert_eq!(motion_media::panic_reason(Box::new(String::from("owned detail"))), "owned detail");
    assert_eq!(motion_media::panic_reason(Box::new(7u32)), "unknown native error");
    let detail = motion_media::panic_reason(Box::new("界".repeat(5000)));
    assert_eq!(detail.chars().count(), 2048);
}

#[test]
fn caught_source_panic_reaches_failed_status_with_original_detail_and_can_be_released() {
    let root = tempfile::tempdir().unwrap();
    let jobs = AudioJobs::new(root.path().into(), Limits::default()).unwrap();
    jobs.start_with_source(
        || panic!("injected source panic detail"),
        ImportOptions { request_id: "panic".into(), at_frame: 0, track: None, name: "test".into() },
        false,
    ).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        let status = jobs.status("panic").unwrap();
        if status.state == "failed" { break status; }
        assert!(Instant::now() < deadline, "worker remained {}", status.state);
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(status.phase, "failed");
    assert!(status.error.unwrap().contains("injected source panic detail"));
    jobs.release("panic").unwrap();
    assert!(!jobs.contains("panic"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
