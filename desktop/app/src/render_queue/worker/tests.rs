use super::*;
use std::{fs, path::Path};

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while !condition() {
        assert!(Instant::now() < deadline, "queue test deadline exceeded");
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn reply(ticket: Ticket) -> Result<Option<u64>> {
    let mut result = None;
    wait(|| match ticket.poll() {
        Ok(None) => false,
        Ok(Some(value)) => {
            result = Some(Ok(value));
            true
        }
        Err(error) => {
            result = Some(Err(error));
            true
        }
    });
    result.unwrap()
}
fn ready(queue: &Queue) {
    wait(|| {
        let view = queue.view();
        assert!(!view.stopped, "{:?}", view.error);
        view.ready
    });
}
fn frozen(root: &Path, color: [u8; 4], revision: u64, frames: u32) -> FrozenMovie {
    fs::create_dir_all(root.join("assets")).unwrap();
    image::RgbaImage::from_pixel(32, 24, image::Rgba(color))
        .save(root.join("assets/image.png"))
        .unwrap();
    let mut project = motion_core::Project::new(32, 24, 30, frames).unwrap();
    project.assets.push(motion_core::Asset {
        id: 1,
        path: "assets/image.png".into(),
        width: 32,
        height: 24,
    });
    let mut layer =
        motion_core::Layer::solid(1, "Frozen image", [32., 24.], [16., 12., 0.], [1.; 4]);
    layer.content = motion_core::Content::Image { asset: 1 };
    project.layers.push(layer);
    FrozenMovie::new(
        project,
        root,
        motion_effects::Registry::new_with_builtins().unwrap(),
        motion_render::resource_policy::MAX_SCRATCH_BUDGET,
        revision,
    )
    .unwrap()
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
#[cfg(feature = "ffmpeg")]
fn render_queue_worker_exports_multiple_frozen_sources_after_reopen_and_live_file_removal() {
    let root = tempfile::tempdir().unwrap();
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    let a = reply(
        queue
            .add(
                frozen(root.path(), [220, 30, 50, 255], 7, 2),
                queue.view().document.templates[0].clone(),
                root.path().join("red.mp4"),
                ExportControl::default(),
            )
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    let b = reply(
        queue
            .add(
                frozen(root.path(), [20, 40, 230, 255], 8, 3),
                queue.view().document.templates[0].clone(),
                root.path().join("blue.mp4"),
                ExportControl::default(),
            )
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    fs::remove_file(root.path().join("assets/image.png")).unwrap();
    assert_eq!(queue.view().document.jobs.len(), 2);
    drop(queue);
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    assert!(queue
        .view()
        .document
        .jobs
        .iter()
        .all(|j| j.state == State::Queued));
    assert!(!root.path().join("red.mp4").exists());
    reply(queue.start().unwrap()).unwrap();
    wait(|| {
        let view = queue.view();
        assert!(view.error.is_none(), "{:?}", view.error);
        view.document
            .jobs
            .iter()
            .all(|j| matches!(j.state, State::Succeeded { .. }))
    });
    let view = queue.view();
    assert_eq!(
        view.document.jobs.iter().map(|j| j.id).collect::<Vec<_>>(),
        [a, b]
    );
    assert_eq!(
        view.document
            .jobs
            .iter()
            .map(|j| j.source.revision)
            .collect::<Vec<_>>(),
        [7, 8]
    );
    assert_eq!(
        view.document
            .jobs
            .iter()
            .map(|j| j.attempt)
            .collect::<Vec<_>>(),
        [1, 1]
    );
    for (name, count, red) in [("red.mp4", 2, true), ("blue.mp4", 3, false)] {
        let path = root.path().join(name);
        let probe = motion_desktop_media::av::probe(&path, None, None, &|| Ok(())).unwrap();
        assert_eq!(probe.timestamps.len(), count);
        let stamp = probe.timestamps[0];
        let mut decoder =
            motion_desktop_media::av::Decoder::open(&path, probe.asset, probe.timestamps).unwrap();
        let frame = decoder.frame(stamp, &|| Ok(())).unwrap();
        let pixels = frame.rgba().unwrap();
        let pixel = &pixels[(12 * 32 + 16) * 4..][..4];
        if red {
            assert!(pixel[0] > 200 && pixel[2] < 80, "{pixel:?}");
        } else {
            assert!(pixel[2] > 200 && pixel[0] < 80, "{pixel:?}");
        }
    }
    drop(queue);
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    assert!(queue
        .view()
        .document
        .jobs
        .iter()
        .all(|j| matches!(j.state, State::Succeeded { .. })));
    assert!(!queue.view().running);
}

#[test]
#[cfg(feature = "ffmpeg")]
fn render_queue_worker_cancels_queued_jobs_and_rejects_invalid_template_without_partial_write() {
    let root = tempfile::tempdir().unwrap();
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    let id = reply(
        queue
            .add(
                frozen(root.path(), [1, 2, 3, 255], 1, 2),
                queue.view().document.templates[0].clone(),
                root.path().join("cancel.mp4"),
                ExportControl::default(),
            )
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    reply(queue.cancel(id).unwrap()).unwrap();
    assert_eq!(queue.view().document.jobs[0].state, State::Cancelled);
    assert!(!root.path().join("cancel.mp4").exists());
    let before = queue.view().document.clone();
    let mut invalid = before.templates[0].clone();
    invalid.pattern = "../outside.mp4".into();
    assert!(reply(queue.save_template(invalid).unwrap()).is_err());
    assert_eq!(queue.view().document, before);
    reply(queue.retry(id, None).unwrap()).unwrap();
    assert_eq!(queue.view().document.jobs[0].state, State::Queued);
    reply(queue.remove(id).unwrap()).unwrap();
    assert!(queue.view().document.jobs.is_empty());
    assert_eq!(
        fs::read_dir(root.path().join(".motion-render-queue/snapshots"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
#[cfg(feature = "ffmpeg")]
fn render_queue_active_cancel_stops_real_export_and_explicit_retry_keeps_frozen_input() {
    let root = tempfile::tempdir().unwrap();
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    let id = reply(
        queue
            .add(
                frozen(root.path(), [210, 40, 60, 255], 21, 1000),
                queue.view().document.templates[0].clone(),
                root.path().join("cancel-active.mp4"),
                ExportControl::default(),
            )
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    let original = queue.view().document.jobs[0].clone();
    reply(queue.start().unwrap()).unwrap();
    wait(|| queue.view().active_job == Some(id));
    let ticket = queue.cancel(id).unwrap();
    // This check happens before waiting for the serial worker's command reply.
    if let Some(active) = queue.shared.update(|p| p.active.clone()) {
        assert_eq!(
            active.control.check().unwrap_err(),
            "movie export cancelled"
        );
    }
    reply(ticket).unwrap();
    wait(|| {
        let view = queue.view();
        view.document.jobs[0].state == State::Cancelled && !view.running
    });
    assert!(!root.path().join("cancel-active.mp4").exists());
    assert!(!fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".motion-movie-")));
    reply(queue.retry(id, None).unwrap()).unwrap();
    let retried = queue.view().document.jobs[0].clone();
    assert_eq!(retried.state, State::Queued);
    assert_eq!(retried.attempt, 1);
    assert_eq!(retried.source, original.source);
    assert_eq!(retried.snapshot, original.snapshot);
    assert_eq!(retried.template, original.template);
    assert!(!queue.view().running);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
#[cfg(feature = "ffmpeg")]
fn render_queue_failed_job_is_retained_and_explicit_retry_can_publish_to_a_new_output() {
    let root = tempfile::tempdir().unwrap();
    let queue = Queue::open(root.path().to_owned()).unwrap();
    ready(&queue);
    let output = root.path().join("foreign.mp4");
    let id = reply(
        queue
            .add(
                frozen(root.path(), [30, 210, 60, 255], 31, 2),
                queue.view().document.templates[0].clone(),
                output.clone(),
                ExportControl::default(),
            )
            .unwrap(),
    )
    .unwrap()
    .unwrap();
    fs::write(&output, b"foreign finished movie").unwrap();
    reply(queue.start().unwrap()).unwrap();
    wait(|| {
        let view = queue.view();
        matches!(view.document.jobs[0].state, State::Failed { .. }) && !view.running
    });
    let failed = queue.view().document.jobs[0].clone();
    assert!(matches!(&failed.state, State::Failed { cause } if cause.contains("already exists")));
    assert_eq!(failed.attempt, 1);
    assert_eq!(fs::read(&output).unwrap(), b"foreign finished movie");
    assert!(reply(queue.retry(id, None).unwrap()).is_err());
    let destination = root.path().join("explicit-retry.mp4");
    reply(queue.retry(id, Some(destination.clone())).unwrap()).unwrap();
    reply(queue.start().unwrap()).unwrap();
    wait(|| matches!(queue.view().document.jobs[0].state, State::Succeeded { .. }));
    let completed = queue.view().document.jobs[0].clone();
    assert_eq!(completed.attempt, 2);
    assert_eq!(completed.source, failed.source);
    assert_eq!(completed.snapshot, failed.snapshot);
    assert_eq!(fs::read(&output).unwrap(), b"foreign finished movie");
    let probe = motion_desktop_media::av::probe(&destination, None, None, &|| Ok(())).unwrap();
    assert_eq!(probe.timestamps.len(), 2);
}

#[test]
fn render_queue_default_error_sinks_and_inflight_are_visible_before_a_handler_returns() {
    const CHILD: &str = "MOTION_RENDER_QUEUE_ERROR_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let queue = Queue::open(root.clone()).unwrap();
        ready(&queue);
        let mut invalid = queue.view().document.templates[0].clone();
        invalid.pattern = "../outside.mp4".into();
        assert!(reply(queue.save_template(invalid).unwrap()).is_err());
        let state = queue.shared.clone();
        let (entered, entry) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let blocked = std::thread::spawn(move || {
            observe(&state, 999, "controlled_block", Some(12), || {
                entered.send(()).unwrap();
                released.recv().unwrap();
                Err::<(), _>("controlled original IO cause".into())
            })
        });
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let view = queue.view();
        let flight = view.flight.unwrap();
        assert_eq!(
            (flight.request, flight.operation, flight.job),
            (999, "controlled_block", Some(12))
        );
        assert!(flight.began.elapsed() < Duration::from_secs(5));
        let log = fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(log.contains("start request=999 operation=controlled_block job=Some(12)"));
        assert!(!log.contains("controlled original IO cause"));
        release.send(()).unwrap();
        assert!(blocked.join().unwrap().is_err());
        assert!(queue.view().flight.is_none());
        drop(queue);
        fs::write(
            root.join(".motion-render-queue/queue.json"),
            b"{invalid saved queue",
        )
        .unwrap();
        let invalid = Queue::open(root.clone()).unwrap();
        wait(|| invalid.view().stopped);
        assert!(!invalid.view().ready);
        assert!(invalid
            .view()
            .error
            .as_ref()
            .unwrap()
            .contains("parse render queue"));
        crate::logging::flush();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "render_queue::worker::tests::render_queue_default_error_sinks_and_inflight_are_visible_before_a_handler_returns"])
        .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let log = fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for needle in [
        "operation=save_template",
        "operation=controlled_block",
        "operation=open",
    ] {
        assert_eq!(
            log.lines()
                .filter(|s| s.contains("ERROR") && s.contains(needle))
                .count(),
            1,
            "{log}"
        );
        assert_eq!(
            stderr
                .lines()
                .filter(|s| s.contains("ERROR") && s.contains(needle))
                .count(),
            1,
            "{stderr}"
        );
        assert!(!stdout.contains(needle), "{stdout}");
    }
    assert!(
        log.contains("controlled original IO cause")
            && stderr.contains("controlled original IO cause")
    );
    assert!(
        !log.contains("done request="),
        "routine completion stays Debug at default Info"
    );
}
