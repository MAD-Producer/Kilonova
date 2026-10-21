//! Exercise the public audio clock and its actual Shell consumer without a
//! device or sleeps. PCM still passes through the production callback and ring.
use super::*;

fn output(rate: u32, start_sample: u64, total_samples: u64) -> AudioOutput {
    let mut shared = Shared::new();
    shared.test_now_ns = Some(AtomicU64::new(0));
    shared.ready.store(true, Ordering::Release);
    shared.begin_playback_at(0);
    AudioOutput {
        stream: None,
        producer: None,
        monitor: None,
        shared: Arc::new(shared),
        rate,
        start_sample,
        total_samples,
    }
}

fn at(output: &AudioOutput, now: u64) {
    output
        .shared
        .test_now_ns
        .as_ref()
        .unwrap()
        .store(now, Ordering::Relaxed);
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-10,
        "actual={actual} expected={expected}"
    );
}

#[test]
fn public_clock_rescales_device_samples_and_retains_latency_underrun_and_end_boundaries() {
    for rate in [44_100, 96_000] {
        for fps in [24, 30, 60] {
            let output = output(rate, 48_000, 51_600);
            let shared = &output.shared;
            close(output.frame(fps), f64::from(fps));
            // Submit 50 ms, played from 20 ms after the callback timestamp.
            for _ in 0..rate / 20 {
                assert!(shared.ring.push([0.25, -0.5]));
            }
            let mut consumed = 0;
            let mut pcm = vec![0.; (rate / 20) as usize * 2];
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_000_000_000,
                20_000_000,
            );
            assert!(pcm.chunks_exact(2).all(|p| p == [0.25, -0.5]));
            at(&output, 1_010_000_000);
            close(output.frame(fps), f64::from(fps));
            // 30 ms is an integral number of samples at both device rates.
            at(&output, 1_050_000_000);
            close(output.frame(fps), 1.03 * f64::from(fps));
            at(&output, 1_100_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));

            // A real empty callback outputs silence, retaining the final sample.
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_100_000_000,
                20_000_000,
            );
            assert!(pcm.iter().all(|&p| p == 0.));
            assert_eq!(shared.underruns.load(Ordering::Relaxed), 1);
            at(&output, 1_150_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));

            for _ in 0..rate / 20 {
                assert!(shared.ring.push([0.5, -0.25]));
            }
            render_callback(
                shared,
                &mut pcm,
                2,
                &mut consumed,
                1_160_000_000,
                20_000_000,
            );
            at(&output, 1_160_000_000);
            close(output.frame(fps), 1.05 * f64::from(fps));
            at(&output, 1_210_000_000);
            close(output.frame(fps), 1.075 * f64::from(fps));
            assert!(output.error().is_none());
        }
    }
}

#[test]
fn shell_uses_audio_position_for_seek_and_stops_at_the_final_composition_frame() {
    let directory = tempfile::tempdir().unwrap();
    let mut shell = crate::Shell::new(
        directory.path().join("project"),
        None,
        Some(crate::Locale::En),
    )
    .unwrap();
    let fps = shell.state["project"]["fps"].as_u64().unwrap() as u32;
    let frames = shell.state["project"]["frames"].as_u64().unwrap();
    let total_samples = frames * 48_000 / u64::from(fps);
    let output = output(44_100, total_samples - 4_800, total_samples);
    let shared = output.shared.clone();
    for _ in 0..4_410 {
        assert!(shared.ring.push([0.25, -0.5]));
    }
    let mut consumed = 0;
    let mut pcm = vec![0.; 4_410 * 2];
    render_callback(
        &shared,
        &mut pcm,
        2,
        &mut consumed,
        1_000_000_000,
        20_000_000,
    );
    at(&output, 1_070_000_000);
    shell.audio = Some(output);
    shell.playback = Some((Instant::now(), 0.));
    shell.advance_playback().unwrap();
    let expected = frames as f64 - 0.05 * f64::from(fps);
    close(shell.frame, expected);
    assert!(shell.playback.is_some());
    let (_, reply) = shell.editing.take().unwrap();
    let state = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    close(state["frame"].as_f64().unwrap(), expected);
    assert!(shell.panels.message.is_none());

    at(shell.audio.as_ref().unwrap(), 1_120_000_000);
    shell.advance_playback().unwrap();
    assert!(shell.audio.is_none());
    assert!(shell.playback.is_none());
    close(shell.frame, (frames - 1) as f64);
    let (_, reply) = shell.editing.take().unwrap();
    let state = reply.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    close(state["frame"].as_f64().unwrap(), (frames - 1) as f64);
    assert!(shell.panels.message.is_none());
    assert!(shared.stop.load(Ordering::Acquire));
}
