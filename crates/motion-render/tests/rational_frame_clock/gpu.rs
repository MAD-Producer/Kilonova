use super::clock_project;
use motion_core::{ExpressionEvaluator, FrameRate};
use motion_render::{Renderer, Scene};

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn cached_rational_subframes_and_rate_changes_match_fresh_gpu_captures() {
    let mut cached = pollster::block_on(Renderer::headless()).unwrap();
    let mut fresh = pollster::block_on(Renderer::headless()).unwrap();
    println!(
        "RATIONAL_CLOCK_GPU adapter={} backend={:?}",
        cached.adapter_info.name, cached.adapter_info.backend
    );
    let target = cached.capture_target(64, 64).unwrap();
    let reference = fresh.capture_target(64, 64).unwrap();
    let directory = std::env::var_os("MOTION_RATE_GPU_EVIDENCE_DIR").map(std::path::PathBuf::from);
    if let Some(directory) = &directory {
        std::fs::create_dir_all(directory).unwrap();
    }
    let mut records = Vec::new();
    let mut hits = 0;
    for (rate_index, [numerator, denominator]) in [
        [24, 1],
        [24000, 1001],
        [30000, 1001],
        [30000, 1003],
        [60000, 1001],
    ]
    .into_iter()
    .enumerate()
    {
        let rate = FrameRate::new(numerator, denominator).unwrap();
        let project = clock_project(rate);
        let mut scene = Scene::new(&project).unwrap();
        let mut original = None;
        for (frame_index, frame) in [10.125, 10.75, 27.5, 10.125].into_iter().enumerate() {
            scene
                .sample(&project, frame, None, &ExpressionEvaluator)
                .unwrap();
            let mut encoder = cached.device.create_command_encoder(&Default::default());
            cached
                .encode_preview(&scene, &target.view, 64, 64, &mut encoder, None)
                .unwrap();
            cached.queue.submit(Some(encoder.finish()));
            let actual = cached.read_target(&target).unwrap();
            let expected = fresh.capture(&scene, &reference).unwrap().0;
            assert_eq!(actual, expected, "rate {rate}, frame {frame}");
            assert!(
                actual.chunks_exact(4).any(|pixel| pixel[3] != 0),
                "empty clock fixture"
            );
            if frame_index == 0 {
                original = Some(actual.clone());
            } else {
                hits += 1;
                if frame_index == 3 {
                    assert_eq!(Some(&actual), original.as_ref());
                } else {
                    assert_ne!(
                        Some(&actual),
                        original.as_ref(),
                        "clock did not change pixels"
                    );
                }
            }
            assert_eq!(
                cached.effect_plan_cache_hits(),
                hits,
                "rate {rate}, frame {frame}"
            );
            let file = format!("rate-{rate_index}-frame-{frame_index}.png");
            if let Some(directory) = &directory {
                image::save_buffer(
                    directory.join(&file),
                    &actual,
                    64,
                    64,
                    image::ColorType::Rgba8,
                )
                .unwrap();
            }
            records
                .push(serde_json::json!({"rate":rate,"frame":frame,"file":file,"cacheHits":hits}));
        }
    }
    if let Some(directory) = &directory {
        std::fs::write(
            directory.join("captures.json"),
            serde_json::to_vec_pretty(&records).unwrap(),
        )
        .unwrap();
    }
}
