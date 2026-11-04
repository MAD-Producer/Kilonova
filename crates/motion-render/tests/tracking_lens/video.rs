//! Independently measured video color and immutable queued lens input revisions.
use super::*;
use motion_render::{ChromaLayout, Yuv420Frame};

fn assert_oracle(p: &Project, scene: &Scene, actual: &[u8]) -> (usize, u8) {
    let lens = scene.native_lens().unwrap();
    let mut reference = Vec::with_capacity(actual.len());
    for y in 0..480 {
        for x in 0..640 {
            reference.extend_from_slice(&expected(p, lens, [x as f64 + 0.5, y as f64 + 0.5]));
        }
    }
    let mut checked = 0;
    let mut maximum = 0;
    for y in 2..478usize {
        for x in 2..638usize {
            let at = (y * 640 + x) * 4;
            let color = &reference[at..at + 4];
            // Preserve the existing independent ray oracle's two-pixel edge
            // exclusion and two-byte threshold, including alpha/depth/flat runs.
            if [-2isize, 0, 2].into_iter().any(|dy| {
                [-2isize, 0, 2].into_iter().any(|dx| {
                    let i = ((y as isize + dy) as usize * 640 + (x as isize + dx) as usize) * 4;
                    reference[i..i + 4] != *color
                })
            }) {
                continue;
            }
            checked += 1;
            for channel in 0..4 {
                maximum = maximum.max(actual[at + channel].abs_diff(color[channel]));
            }
        }
    }
    assert!(checked > 250000, "{checked}");
    assert!(maximum <= 2, "maximum video/lens error={maximum}");
    (checked, maximum)
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires actual GPU")]
fn rgba_and_yuv_video_lens_inputs_match_rays_and_keep_unsubmitted_revisions() {
    let root = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    for yuv in [false, true] {
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        for rotation in [Rotation::None, Rotation::Clockwise90] {
            let mut outputs = Vec::new();
            for phase in 0..2u64 {
                let mut p = project(rotation, if phase == 0 { 0.018 } else { -0.018 });
                // The oracle treats the ordinary solid color explicitly; the
                // sampled GPU layer instead reads an actual uploaded video.
                p.layers[1].id = 21;
                let color = if yuv {
                    if phase == 0 {
                        [255, 255, 255, 255]
                    } else {
                        [0, 0, 0, 255]
                    }
                } else if phase == 0 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                };
                p.layers[1].content = Content::Solid {
                    color: color.map(|v| f32::from(v) / 255.),
                };
                renderer.configure_assets(&p, root.path()).unwrap();
                let mut scene = sample(&p, 0.);
                let layer = scene.layers.iter_mut().find(|l| l.id == 21).unwrap();
                layer.video = Some(VideoSample {
                    asset: 1,
                    source_time_us: phase,
                });
                let pts = phase + if rotation == Rotation::None { 0 } else { 2 };
                if yuv {
                    let frame = Yuv420Frame {
                        width: 16,
                        height: 16,
                        rotation: 0,
                        standard: 1,
                        range: 2,
                        phase: [0, 0],
                        chroma_layout: ChromaLayout::Uv,
                        y: vec![if phase == 0 { 235 } else { 16 }; 256],
                        uv: vec![128; 128],
                    };
                    assert!(frame.to_rgba().unwrap().chunks_exact(4).all(|p| p == color));
                    renderer.upload_video_yuv(21, 1, pts, &frame).unwrap();
                } else {
                    renderer
                        .upload_video_frame(21, 1, pts, 16, 16, &color.repeat(256))
                        .unwrap();
                }
                let target = renderer.capture_target(640, 480).unwrap();
                let mut encoder = renderer
                    .allocation_command_encoder(&Default::default())
                    .unwrap();
                renderer
                    .encode(&scene, &target.view, 640, 480, &mut encoder, None)
                    .unwrap();
                outputs.push((p, scene, target, encoder.finish()));
            }
            // Upload/encode both different revisions before submitting either.
            // A mutable video texture alias makes the first output use phase 1.
            for (p, scene, target, commands) in outputs {
                renderer.submit_allocated(commands, |_| {}).unwrap();
                let actual = renderer.read_target(&target).unwrap();
                let (checked, maximum) = assert_oracle(&p, &scene, &actual);
                rows.push(
                    serde_json::json!({"yuv":yuv,"rotation":format!("{rotation:?}"),
                    "checked":checked,"maximumError":maximum}),
                );
            }
            renderer.check_health().unwrap();
        }
    }
    if let Some(directory) =
        std::env::var_os("MOTION_LENS_VIDEO_ARTIFACTS").map(std::path::PathBuf::from)
    {
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("video-lens-report.json"),
            serde_json::to_vec_pretty(&rows).unwrap(),
        )
        .unwrap();
    }
}
