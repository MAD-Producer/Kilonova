use motion_media::{decode_audio, AudioJobs, DecodeAudio, ImportOptions, Limits, TaskStatus};
use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};

const LIMIT: u64 = 16 * 1024 * 1024;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/formats/lossless.flac")
}

fn unknown_length_flac() -> Vec<u8> {
    let mut bytes = fs::read(fixture()).unwrap();
    assert_eq!(&bytes[..4], b"fLaC");
    assert_eq!(bytes[4] & 0x7f, 0, "first FLAC block must be STREAMINFO");
    assert_eq!(&bytes[5..8], &[0, 0, 34]);
    // STREAMINFO stores a 36-bit sample count; zero means unknown. Keep the
    // sample rate, channels, bit depth, MD5 and compressed packets unchanged.
    assert!(bytes[21] & 0x0f != 0 || bytes[22..26].iter().any(|b| *b != 0));
    bytes[21] &= 0xf0;
    bytes[22..26].fill(0);
    bytes
}

fn options(id: &str) -> ImportOptions {
    ImportOptions {
        request_id: id.into(),
        at_frame: 0,
        track: None,
        name: "progress fixture".into(),
    }
}

fn terminal(jobs: &AudioJobs, id: &str) -> TaskStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = jobs.status(id).unwrap();
        if status.state != "running" {
            return status;
        }
        assert!(Instant::now() < deadline, "task did not finish: {status:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn unknown_stream_length_keeps_identical_pcm_without_inventing_a_ratio() {
    let root = tempfile::tempdir().unwrap();
    let unknown = root.path().join("unknown.flac");
    fs::write(&unknown, unknown_length_flac()).unwrap();
    let known_pcm = root.path().join("known.pcm");
    let unknown_pcm = root.path().join("unknown.pcm");
    let mut known_progress = Vec::new();
    let known = decode_audio(&fixture(), &known_pcm, None, LIMIT, &mut |value| {
        known_progress.push(value);
        Ok(())
    })
    .unwrap();
    let mut unknown_progress = Vec::new();
    let unknown = decode_audio(&unknown, &unknown_pcm, None, LIMIT, &mut |value| {
        unknown_progress.push(value);
        Ok(())
    })
    .unwrap();
    assert!(known_progress.len() > 1);
    assert!(known_progress
        .iter()
        .all(|value| value.is_some_and(|v| (0.0..=1.0).contains(&v))));
    assert!(known_progress
        .iter()
        .any(|value| value.is_some_and(|v| v > 0.0)));
    assert_eq!(unknown_progress.len(), known_progress.len());
    assert!(unknown_progress.iter().all(Option::is_none));
    assert_eq!(unknown, known);
    assert_eq!(fs::read(unknown_pcm).unwrap(), fs::read(known_pcm).unwrap());
}

#[test]
fn unknown_progress_keeps_cancellation_checkpoints_and_original_cause() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("unknown.flac");
    fs::write(&source, unknown_length_flac()).unwrap();
    let mut calls = 0;
    let error = decode_audio(
        &source,
        &root.path().join("cancelled.pcm"),
        None,
        LIMIT,
        &mut |value| {
            calls += 1;
            assert!(value.is_none());
            Err("controlled cancellation at unknown progress".into())
        },
    )
    .unwrap_err();
    assert_eq!(calls, 1);
    assert_eq!(
        error.to_string(),
        "controlled cancellation at unknown progress"
    );
}

#[test]
fn actual_worker_exposes_null_during_decode_then_one_at_success() {
    let root = tempfile::tempdir().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let continue_rx = Mutex::new(continue_rx);
    let decode: DecodeAudio = Arc::new(move |source, pcm, track, limit, check| {
        let mut paused = false;
        decode_audio(source, pcm, track, limit, &mut |value| {
            check(value)?;
            if !paused && value.is_none() {
                paused = true;
                entered_tx.send(()).unwrap();
                continue_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
            }
            Ok(())
        })
    });
    let jobs = AudioJobs::with_decoder(root.path().into(), Limits::default(), decode).unwrap();
    let initial = jobs
        .start(
            Box::new(Cursor::new(unknown_length_flac())),
            None,
            options("unknown"),
            true,
        )
        .unwrap();
    assert!(initial.progress.is_none());
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let running = jobs.status("unknown").unwrap();
    assert_eq!(running.state, "running");
    assert_eq!(running.phase, "decoding");
    assert!(running.progress.is_none());
    let wire = serde_json::to_value(&running).unwrap();
    assert!(wire.as_object().unwrap().contains_key("progress"));
    assert!(wire["progress"].is_null());
    continue_tx.send(()).unwrap();
    let success = terminal(&jobs, "unknown");
    assert_eq!(success.state, "succeeded");
    assert_eq!(success.progress, Some(1.0));
    assert_eq!(serde_json::to_value(success).unwrap()["progress"], 1.0);
    assert_eq!(
        fs::read_dir(root.path()).unwrap().count(),
        0,
        "probe must not retain staging files"
    );
    jobs.release("unknown").unwrap();
}

#[test]
fn a_cancelled_unknown_length_worker_cannot_publish_success() {
    let root = tempfile::tempdir().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let continue_rx = Mutex::new(continue_rx);
    let (done_tx, done_rx) = mpsc::channel();
    let decode: DecodeAudio = Arc::new(move |source, pcm, track, limit, check| {
        let mut paused = false;
        let result = decode_audio(source, pcm, track, limit, &mut |value| {
            check(value)?;
            if !paused {
                assert!(value.is_none());
                paused = true;
                entered_tx.send(()).unwrap();
                continue_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
                check(value)?;
            }
            Ok(())
        });
        done_tx
            .send(result.as_ref().unwrap_err().to_string())
            .unwrap();
        result
    });
    let jobs = AudioJobs::with_decoder(root.path().into(), Limits::default(), decode).unwrap();
    jobs.start(
        Box::new(Cursor::new(unknown_length_flac())),
        None,
        options("cancel"),
        false,
    )
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(jobs.cancel("cancel").unwrap().progress.is_none());
    continue_tx.send(()).unwrap();
    assert_eq!(
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        "audio task cancelled"
    );
    let cancelled = jobs.status("cancel").unwrap();
    assert_eq!(cancelled.state, "cancelled");
    assert!(cancelled.progress.is_none());
    assert!(cancelled.error.is_none());
    assert!(cancelled.metadata.is_none());
    let deadline = Instant::now() + Duration::from_secs(10);
    while fs::read_dir(root.path()).unwrap().next().is_some() {
        assert!(
            Instant::now() < deadline,
            "cancelled worker left staging files"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    jobs.release("cancel").unwrap();
}
