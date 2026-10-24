use super::*;
use motion_render::{gpu_commands::CommandStage, preview_input::PreviewInputTexture, Renderer};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_packet_upload_keeps_reader_identity_separate_from_worker_requests() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
    ))
    .unwrap();
    let mut target = PreviewInputTexture::new(
        renderer.device.clone(),
        renderer.queue.clone(),
        1 << 20,
        renderer.allocation_budget().clone(),
    )
    .unwrap();
    let Some(observer) = target.start_command_profiling(1).unwrap() else {
        eprintln!("GPU_INPUT_PACKET available=false; no actual packet identity acceptance");
        return;
    };
    // Immutable CPU fixture; conversion/submission/query mapping below are real
    // native GPU work. This is not an actual decoder or source-reader benchmark.
    let packet = PreparedInput {
        info: InputInfo {
            sequence: 33,
            revision: 9,
            source: InputSource::Layer {
                composition: "source-comp".into(),
                object: 8,
            },
            logical_size: [2., 4.],
            source_size: [2, 4],
            raster_size: [1, 2],
            clock: InputClock {
                composition_frame: 21.5,
                local_frame: 6.5,
                seconds: 6.5 / 30.,
                fps: 30,
                source_time_us: Some(316_667),
            },
            stage: "original",
            active: true,
            working_space: motion_effects::WorkingSpace::Linear,
            alpha_mode: motion_effects::AlphaMode::Premultiplied,
            video_pts_us: Some(300_000),
            video_end_us: Some(500_000),
        },
        pixels: InputPixels::Video(Arc::new(DecodedFrame {
            pixels: VideoPixels::Yuv(motion_render::Yuv420Frame {
                width: 4,
                height: 2,
                rotation: 90,
                standard: 1,
                range: 2,
                phase: [0, 0],
                y: vec![90; 8],
                uv: vec![128; 4],
                chroma_layout: motion_render::ChromaLayout::Uv,
            }),
            pts: 300_000,
            end: 500_000,
            width: 2,
            height: 4,
            decode_us: 0,
            codec_us: 0,
            transfer_us: 0,
            pack_us: 0,
            source_transfer: "yuv",
            decoder_name: "fixture".into(),
        })),
    };
    packet.upload(&mut target).unwrap();
    assert_eq!(target.dimensions(), (1, 2));
    let pending = observer.snapshot();
    assert_eq!(pending.commands.active.len(), 1);
    assert!(pending.commands.active[0].contexts.is_empty());
    assert_eq!(
        pending.commands.active[0].input.as_ref().unwrap().sequence,
        33
    );
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    renderer.check_health().unwrap();
    let completed = observer.snapshot();
    assert_eq!(completed.commands.counts.completed, 1);
    assert_eq!(completed.readback.valid_samples, 1);
    let command = &completed.commands.recent[0];
    assert_eq!(command.stage, CommandStage::AuthoringInput);
    assert!(
        command.contexts.is_empty(),
        "reader sequence must not fabricate a worker request"
    );
    let input = command.input.as_ref().unwrap();
    assert_eq!(
        (input.sequence, input.revision, input.object),
        (33, 9, Some(8))
    );
    assert_eq!(input.source_frame, 21.5);
    assert_eq!(input.composition.as_deref(), Some("source-comp"));
    assert!(!input.composition_omitted);
    assert!(command
        .timestamp_words
        .is_some_and(|words| words[1] > words[0]));
    eprintln!(
        "GPU_INPUT_PACKET {}",
        serde_json::json!({
            "available":true,"actualNativeCommands":1,"sequence":input.sequence,
            "revision":input.revision,"object":input.object,"sourceFrame":input.source_frame,
            "fabricatedWorkerRequests":0,
            "scope":"actual PreparedInput upload and native YUV conversion/query mapping from a constructed immutable packet; no decoder or pixel-oracle acceptance in this case"
        })
    );
}
