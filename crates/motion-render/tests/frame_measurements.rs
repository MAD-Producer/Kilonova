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
                Some(FrameResources {vector_upload: Default::default(),
                    textures: TextureMemoryBreakdown {
                        video_planes: sequence * 100,
                        ..Default::default()
                    },
                    preview_target_bytes: 32,
                    exposure_target_bytes: 0,
                    exposure_media_us: 0,
                    cpu_video_cache_bytes: sequence * 10,
                    cpu_video_proxy_bytes: sequence,
                    video_prepare_us: sequence * 2,
                    video_upload_us: sequence * 3,
                    image_upload_bytes_total: 0,
                    video_upload_bytes_total: sequence * 100,
                    pipeline_builds_total: sequence,
                    memory_quality_level: 0,
                    resource_refusals_total: 0,
                    image_readback: None, timestamp_readback: None,
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

#[test]
fn exposure_capacity_and_combined_media_time_are_reported_without_inventing_gpu_samples() {
    let mut recorder=FrameRecorder::new(9,1);
    recorder.record(FrameMeasurement {sequence:9,frame:1.25,elapsed_us:100,
        cpu_prepare_us:25,acquire_us:2,submit_us:3,present_call_us:4,render_wall_us:20,
        render_width:4,render_height:2,preview_fps:30,gpu:None,
        resources:Some(FrameResources {vector_upload: Default::default(),textures:TextureMemoryBreakdown::default(),
            preview_target_bytes:32,exposure_target_bytes:96,exposure_media_us:7,
            cpu_video_cache_bytes:16,cpu_video_proxy_bytes:0,video_prepare_us:0,video_upload_us:0,
            image_upload_bytes_total:0,video_upload_bytes_total:0,pipeline_builds_total:0,
            memory_quality_level:0,resource_refusals_total:0,image_readback:None,timestamp_readback:None})});
    let report=recorder.report(json!({}));
    assert_eq!(report["logicalTextureBytes"]["max"],128.);
    assert_eq!(report["exposureTargetBytes"]["max"],96.);
    assert_eq!(report["exposureMediaUs"]["max"],7.);
    assert_eq!(report["frames"][0]["resources"]["exposureTargetBytes"],96);
    assert_eq!(report["gpuSampleCount"],0);assert!(report["gpuTotalUs"].is_null());
}

#[test]
fn vector_upload_capacity_is_separate_from_texture_capacity_and_gpu_timings() {
    let mut recorder=FrameRecorder::new(1,1);
    recorder.record(FrameMeasurement {sequence:1,frame:0.,elapsed_us:1,cpu_prepare_us:0,
        acquire_us:0,submit_us:0,present_call_us:0,render_wall_us:0,
        render_width:4,render_height:2,preview_fps:30,gpu:None,
        resources:Some(FrameResources {textures:TextureMemoryBreakdown::default(),
            vector_upload:motion_render::VectorUploadInfo {
                vertex_capacity_bytes:16,opacity_capacity_bytes:16,pending_bytes:64,idle_bytes:128,
                staging_buffers_created_total:3,destination_buffers_created_total:2,encoded_upload_bytes_total:256,completed_upload_bytes_total:128,
                ..Default::default()
            },
            preview_target_bytes:32,exposure_target_bytes:0,exposure_media_us:0,
            cpu_video_cache_bytes:0,cpu_video_proxy_bytes:0,video_prepare_us:0,video_upload_us:0,
            image_upload_bytes_total:0,video_upload_bytes_total:0,pipeline_builds_total:0,
            memory_quality_level:0,resource_refusals_total:0,image_readback:None,timestamp_readback:None})});
    let report=recorder.report(json!({}));
    assert_eq!(report["logicalTextureBytes"]["max"],32.);
    assert_eq!(report["vectorBufferCapacityBytes"]["max"],224.);
    assert_eq!(report["vectorEncodedUploadBytesTotal"]["max"],256.);
    assert_eq!(report["vectorCompletedUploadBytesTotal"]["max"],128.);
    assert_eq!(report["vectorUploadBufferCreatesTotal"]["max"],5.);
    assert_eq!(report["frames"][0]["resources"]["vectorUpload"]["pendingBytes"],64);
    assert_eq!(report["gpuSampleCount"],0);
    assert!(report["gpuTotalUs"].is_null());
}

#[test]
fn readback_observations_keep_unavailable_counters_distinct_from_known_zero() {
    let mut recorder = FrameRecorder::new(1, 2);
    for sequence in 1..=2 {
        recorder.record(FrameMeasurement {
            sequence, frame:sequence as f64, elapsed_us:1, cpu_prepare_us:0,
            acquire_us:0, submit_us:0, present_call_us:0, render_wall_us:0,
            render_width:4, render_height:2, preview_fps:30, gpu:None,
            resources:Some(FrameResources {
                textures:Default::default(), vector_upload:Default::default(),
                preview_target_bytes:0, exposure_target_bytes:0, exposure_media_us:0,
                cpu_video_cache_bytes:0, cpu_video_proxy_bytes:0, video_prepare_us:0, video_upload_us:0,
                image_upload_bytes_total:0, video_upload_bytes_total:0, pipeline_builds_total:0,
                memory_quality_level:0, resource_refusals_total:0,
                image_readback:if sequence == 1 {None} else {Some(Default::default())},
                timestamp_readback:None,
            }),
        });
    }
    let report = recorder.report(json!({}));
    assert!(report["frames"][0]["resources"]["imageReadback"].is_null());
    assert_eq!(report["frames"][1]["resources"]["imageReadback"]["requestedCalls"], 0);
    assert_eq!(report["frames"][1]["resources"]["imageReadback"]["countersSaturated"], false);
    assert!(report["frames"][1]["resources"]["timestampReadback"].is_null());
    assert_eq!(report["gpuSampleCount"], 0);
    assert!(report["gpuTotalUs"].is_null());
}
