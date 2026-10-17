use motion_render::{FrameMeasurement, FrameRecorder, FrameResources, TextureMemoryBreakdown};
use serde_json::json;

#[test]
fn resources_are_separate_sampled_distributions_and_recording_remains_bounded() {
    let mut recorder = FrameRecorder::new(1, 3);
    for sequence in 1..=5 {
        recorder.record(FrameMeasurement {
            sequence,
            frame: sequence as f64,
            elapsed_us: sequence * 100,
            cpu_prepare_us: sequence * 2,
            acquire_us: 0,
            submit_us: 0,
            present_call_us: 0,
            render_wall_us: sequence * 2,
            render_width: 16,
            render_height: 16,
            preview_fps: 30,
            gpu: None,
            resources: if sequence == 2 {
                None
            } else {
                Some(FrameResources {
                    textures: TextureMemoryBreakdown {
                        video_planes: sequence * 100,
                        ..Default::default()
                    },
                    preview_target_bytes: 32,
                    cpu_video_cache_bytes: sequence * 10,
                    cpu_video_proxy_bytes: sequence,
                    video_prepare_us: sequence * 2,
                    video_upload_us: sequence * 3,
                    image_upload_bytes_total: 0,
                    video_upload_bytes_total: sequence * 100,
                    pipeline_builds_total: sequence,
                    memory_quality_level: 0,
                    resource_refusals_total: 0,
                })
            },
        });
    }
    let report = recorder.report(json!({"scope":"fixture"}));
    assert_eq!(report["frameCount"], 3);
    assert_eq!(report["overflowFrames"], 2);
    assert_eq!(report["resourceSampleCount"], 2);
    assert_eq!(
        report["logicalTextureBytes"],
        json!({"count":2,"p50":132.,"p95":332.,"p99":332.,"max":332.})
    );
    assert_eq!(report["cpuVideoCacheBytes"]["max"], 30.);
    assert_eq!(report["videoPrepareUs"]["p99"], 6.);
    assert!(report["frames"][1]["resources"].is_null());
    assert_eq!(
        report["frames"][2]["resources"]["videoUploadBytesTotal"],
        300
    );
    assert_eq!(report["gpuSampleCount"], 0);
    assert!(report["gpuTotalUs"].is_null());
    assert!(report["resourceSampleScope"]
        .as_str()
        .unwrap()
        .contains("Excludes transient peaks"));
}
