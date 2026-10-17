//! A bounded, headless 1080p comparison. GPU timings cover composition only;
//! wall latency includes a per-frame wait and a 16-byte timestamp readback.
use motion_core::{Layer, Project, VideoSample};
use motion_render::{ChromaLayout, Renderer, Scene, Yuv420Frame};
use std::{path::PathBuf, sync::mpsc, time::Instant};

fn summary(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    serde_json::json!({"mean": values.iter().sum::<f64>() / values.len() as f64,
        "p95": sorted[(sorted.len() * 95 / 100).min(sorted.len()-1)]})
}

fn measure(encoded: bool) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        true,
    ))?;
    let (w, h) = (1920, 1080);
    let mut project = Project::new(w, h, 30, 60)?;
    project.layers.push(Layer::solid(
        1,
        "video",
        [w as f32, h as f32],
        [w as f32 / 2., h as f32 / 2., 0.],
        [1.; 4],
    ));
    let mut scene = Scene::new(&project);
    scene.sample(&project, 0., None, &motion_core::ExpressionEvaluator)?;
    scene.layers[0].video = Some(VideoSample {
        asset: 1,
        source_time_us: 0,
    });
    let frame = Yuv420Frame {
        width: w,
        height: h,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; (w * h) as usize],
        uv: vec![128; (w * h / 2) as usize],
    };
    let target = renderer.render_target(w, h)?;
    let query = renderer
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
        .then(|| {
            renderer.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("composition-only probe"),
                ty: wgpu::QueryType::Timestamp,
                count: 2,
            })
        });
    let resolve = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256,
        usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut cpu = Vec::new();
    let mut wall = Vec::new();
    let mut gpu = Vec::new();
    for pts in 0..40 {
        let began = Instant::now();
        let mut encoder = renderer.device.create_command_encoder(&Default::default());
        let upload = if encoded {
            renderer.encode_video_yuv(1, 1, pts, &frame, &mut encoder)?
        } else {
            renderer.upload_video_yuv(1, 1, pts, &frame)?;
            None
        };
        let writes = query.as_ref().map(|q| wgpu::RenderPassTimestampWrites {
            query_set: q,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        });
        renderer.encode(&scene, &target.view, w, h, &mut encoder, writes)?;
        if let Some(q) = &query {
            encoder.resolve_query_set(q, 0..2, &resolve, 0);
            encoder.copy_buffer_to_buffer(&resolve, 0, &readback, 0, 16);
        }
        renderer.queue.submit(Some(encoder.finish()));
        if let Some(upload) = upload {
            upload.mark_submitted();
        }
        let cpu_us = began.elapsed().as_secs_f64() * 1e6;
        let gpu_us = if query.is_some() {
            let (tx, rx) = mpsc::channel();
            readback
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
            renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
            rx.recv()??;
            let mapped = readback.slice(..).get_mapped_range()?;
            let start = u64::from_le_bytes(mapped[..8].try_into()?);
            let end = u64::from_le_bytes(mapped[8..16].try_into()?);
            let us = end.saturating_sub(start) as f64
                * f64::from(renderer.queue.get_timestamp_period())
                / 1000.;
            drop(mapped);
            readback.unmap();
            Some(us)
        } else {
            renderer.device.poll(wgpu::PollType::wait_indefinitely())?;
            None
        };
        renderer.check_health()?;
        if pts >= 8 {
            cpu.push(cpu_us);
            wall.push(began.elapsed().as_secs_f64() * 1e6);
            if let Some(us) = gpu_us {
                gpu.push(us);
            }
        }
    }
    Ok(
        serde_json::json!({"mode":if encoded {"encoded"} else {"standalone"},
        "adapter":format!("{:?}",renderer.adapter_info), "warmup_frames":8, "measured_frames":32,
        "cpu_upload_encode_submit_us":summary(&cpu), "waited_frame_wall_us":summary(&wall),
        "gpu_composition_only_us":if gpu.is_empty() {serde_json::Value::Null} else {summary(&gpu)},
        "main_submissions":40, "conversion_submissions":renderer.video_conversion_submissions,
        "encoded_conversions":renderer.video_encoded_conversions,
        "cpu_plane_bytes_uploaded":renderer.video_upload_bytes}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/video-submit-probe.json".into());
    let report = serde_json::json!({"resolution":[1920,1080], "debug_build":cfg!(debug_assertions),
        "notes":"Explicit submits only; queue writes still stage CPU plane uploads. GPU timestamps exclude YUV conversion. Wall time includes Wait and timestamp mapping, not playback throughput. No nested scenes, effects, decode or presentation.",
        "runs":[measure(false)?,measure(true)?]});
    if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
