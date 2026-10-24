//! Pure recorder protocol tests. These values are callback identity fixtures,
//! not measured GPU costs or native/device acceptance.
use motion_render::{FrameMeasurement, FrameRecorder, GpuTiming};
use serde_json::json;

fn frame(sequence: u64, source_frame: f64) -> FrameMeasurement {
    FrameMeasurement {
        sequence,
        frame: source_frame,
        elapsed_us: 0,
        cpu_prepare_us: 0,
        acquire_us: 0,
        submit_us: 0,
        present_call_us: 0,
        render_wall_us: 0,
        render_width: 16,
        render_height: 16,
        preview_fps: 30,
        gpu: None,
        presentation_command: None,
        exposure: None,
        resources: None,
    }
}

fn timing(sequence: u64, total_us: f64) -> GpuTiming {
    GpuTiming {
        sequence,
        composition_us: total_us * 0.5,
        presentation_us: total_us * 0.25,
        total_us,
    }
}

#[test]
fn presentation_links_keep_epoch_and_command_identity_without_source_frame_guessing() {
    use motion_render::gpu_commands::{CommandIdentity, ExposureCommandContext};
    let mut recorder = FrameRecorder::new(10, 3);
    for (sequence, epoch) in [(10, 71), (14, 72)] {
        let mut entry = frame(sequence, 1.25);
        entry.presentation_command = Some(CommandIdentity { epoch, id: 1 });
        entry.exposure = Some(ExposureCommandContext {
            request: Some(9),
            poll: Some(sequence),
        });
        recorder.record(entry);
    }
    recorder.record(frame(15, 1.25));
    let report = recorder.report(json!({}));
    assert_eq!(report["presentationCommandIdentityCount"], 2);
    assert_eq!(
        report["frames"][0]["presentationCommand"],
        json!({"epoch":71,"id":1})
    );
    assert_eq!(
        report["frames"][1]["presentationCommand"],
        json!({"epoch":72,"id":1})
    );
    assert_eq!(
        report["frames"][1]["exposure"],
        json!({"request":9,"poll":14})
    );
    assert!(report["frames"][2]["presentationCommand"].is_null());
    assert!(report["frames"][2]["exposure"].is_null());
    assert_eq!(report["gpuSampleCount"], 0);
    assert!(report["gpuTotalUs"].is_null());
    assert!(report["commandJoinScope"]
        .as_str()
        .unwrap()
        .contains("Never join by authored source-frame"));
}

#[test]
fn callbacks_follow_recorded_sequences_across_chrome_only_presentations() {
    // Decoder polling presents editor chrome, advancing the host's sequence
    // without recording a complete composition. Both the first and later
    // recorded frames can therefore have gaps before their GPU callbacks.
    let mut recorder = FrameRecorder::new(100, 8);
    for (sequence, source_frame) in [(103, 0.), (107, 1.5), (108, 2.)] {
        recorder.record(frame(sequence, source_frame));
    }
    // Mapping slots can finish in a different order from frame recording.
    for (sequence, total_us) in [(107, 20.), (108, 30.), (103, 10.)] {
        recorder.timing(timing(sequence, total_us));
    }
    let report = recorder.report(json!({}));
    assert_eq!(report["gpuSampleCount"], 3);
    for (index, (sequence, total_us)) in
        [(103, 10.), (107, 20.), (108, 30.)].into_iter().enumerate()
    {
        assert_eq!(report["frames"][index]["sequence"], sequence);
        assert_eq!(report["frames"][index]["gpuTotalUs"], total_us);
        assert_eq!(report["frames"][index]["gpuCompositionUs"], total_us * 0.5);
    }
    assert_eq!(report["frames"][1]["frame"], 1.5);
    assert_eq!(
        report["gpuTotalUs"],
        json!({"count":3,"p50":20.,"p95":30.,"p99":30.,"max":30.})
    );
}

#[test]
fn unrelated_and_overflow_callbacks_cannot_replace_a_recorded_frame() {
    let mut recorder = FrameRecorder::new(10, 2);
    recorder.record(frame(11, 0.));
    recorder.record(frame(15, 1.));
    recorder.record(frame(17, 2.)); // The existing bounded recorder discards this.
    for sequence in [0, 9, 10, 12, 17, u64::MAX] {
        recorder.timing(timing(sequence, 999.));
    }
    let before = recorder.report(json!({}));
    assert_eq!(before["gpuSampleCount"], 0);
    assert!(before["frames"][0]["gpuTotalUs"].is_null());
    assert!(before["frames"][1]["gpuTotalUs"].is_null());
    recorder.timing(timing(15, 8.));
    recorder.timing(timing(11, 4.));
    let after = recorder.report(json!({}));
    assert_eq!(after["frameCount"], 2);
    assert_eq!(after["overflowFrames"], 1);
    assert_eq!(after["gpuSampleCount"], 2);
    assert_eq!(after["frames"][0]["gpuTotalUs"], 4.);
    assert_eq!(after["frames"][1]["gpuTotalUs"], 8.);
}

#[test]
fn delayed_callbacks_preserve_contiguous_sequences_and_unknown_samples() {
    let mut recorder = FrameRecorder::new(50, 3);
    for sequence in 50..53 {
        recorder.record(frame(sequence, sequence as f64));
    }
    recorder.timing(timing(52, 3.));
    recorder.timing(timing(50, 1.));
    let report = recorder.report(json!({}));
    assert_eq!(report["gpuSampleCount"], 2);
    assert_eq!(report["frames"][0]["gpuTotalUs"], 1.);
    assert!(report["frames"][1]["gpuTotalUs"].is_null());
    assert_eq!(report["frames"][2]["gpuTotalUs"], 3.);
}

#[test]
fn caller_order_is_preserved_and_callbacks_before_recording_are_ignored() {
    let mut recorder = FrameRecorder::new(50, 4);
    for sequence in [55, 49, 53, 57] {
        recorder.record(frame(sequence, sequence as f64));
    }
    for sequence in [49, 53, 55, 57] {
        recorder.timing(timing(sequence, sequence as f64));
    }
    let report = recorder.report(json!({}));
    assert_eq!(report["gpuSampleCount"], 3);
    for (index, sequence) in [55, 49, 53, 57].into_iter().enumerate() {
        assert_eq!(report["frames"][index]["sequence"], sequence);
        if sequence < 50 {
            assert!(report["frames"][index]["gpuTotalUs"].is_null());
        } else {
            assert_eq!(report["frames"][index]["gpuTotalUs"], sequence as f64);
        }
    }
}
