//! Native feasibility contract for encoder boundaries without the optional
//! TIMESTAMP_QUERY_INSIDE_ENCODERS feature (unavailable on Apple GPUs).
//! Availability is explicit; only the supported native path proves timing.
use motion_render::Renderer;

fn boundary(encoder: &mut wgpu::CommandEncoder, queries: &wgpu::QuerySet, index: u32, first: bool) {
    // No dispatch/pipeline is required for this marker. Actual backend execution
    // below must prove that both original timestamp words were really written.
    drop(encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("command interval boundary"),
        timestamp_writes: Some(wgpu::ComputePassTimestampWrites {
            query_set: queries,
            beginning_of_pass_write_index: first.then_some(index),
            end_of_pass_write_index: (!first).then_some(index),
        }),
    }));
}

#[test]
fn pass_timestamp_boundaries_enclose_real_commands_without_encoder_timestamp_feature() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let renderer = pollster::block_on(Renderer::new_profiled(
        &instance,
        None,
        wgpu::TextureFormat::Rgba8Unorm,
        true,
    ))
    .unwrap();
    let available = renderer
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY)
        && renderer
            .device
            .limits()
            .max_compute_workgroups_per_dimension
            > 0;
    if !available {
        eprintln!(
            "GPU_COMMAND_MARKER_PROBE={}",
            serde_json::json!({
                "available": false, "backend": format!("{:?}", renderer.adapter_info.backend),
                "scope": "unavailable is not native timing acceptance"
            })
        );
        return;
    }
    assert!(!renderer
        .device
        .features()
        .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS));
    let context = renderer.allocation_context();
    let resolve = context
        .buffer(&wgpu::BufferDescriptor {
            label: Some("command marker resolved words"),
            size: wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
        .unwrap();
    let mapped = context
        .buffer(&wgpu::BufferDescriptor {
            label: Some("command marker readable words"),
            size: 16,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        })
        .unwrap();
    let target = renderer.capture_target(8, 8).unwrap();
    let queries = renderer.device.create_query_set(&wgpu::QuerySetDescriptor {
        label: Some("command interval marker feasibility"),
        ty: wgpu::QueryType::Timestamp,
        count: 2,
    });
    let validation = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = renderer
        .allocation_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("native command marker feasibility"),
        })
        .unwrap();
    boundary(&mut encoder, &queries, 0, true);
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("real render clear between markers"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &target.view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    boundary(&mut encoder, &queries, 1, false);
    encoder.resolve_query_set(&queries, 0..2, &resolve, 0);
    encoder.copy_buffer_to_buffer(&resolve, 0, &mapped, 0, 16);
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    mapped
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let words = {
        let bytes = mapped.slice(..).get_mapped_range().unwrap();
        assert_eq!(bytes.len(), 16);
        [
            u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
        ]
    };
    mapped.unmap();
    assert!(
        pollster::block_on(validation.pop()).is_none(),
        "marker passes must be natively valid"
    );
    renderer.check_health().unwrap();
    assert!(
        words[1] > words[0],
        "original marker words must enclose real nonzero GPU work: {words:?}"
    );
    let pixels = renderer.read_target(&target).unwrap();
    assert_eq!(pixels.len(), 8 * 8 * 4);
    assert!(pixels
        .chunks_exact(4)
        .all(|pixel| pixel == [255, 0, 0, 255]));
    eprintln!(
        "GPU_COMMAND_MARKER_PROBE={}",
        serde_json::json!({
            "available": true, "actualNativeTimestamps": words,
            "elapsedUs": (words[1] - words[0]) as f64 * f64::from(renderer.queue.get_timestamp_period()) / 1000.0,
            "optionalEncoderTimestampFeatureEnabled": false,
            "mappedBytes": 16, "actualPixelsChecked": 64,
            "adapter": renderer.adapter_info.name,
            "backend": format!("{:?}", renderer.adapter_info.backend),
            "driver": renderer.adapter_info.driver, "driverInfo": renderer.adapter_info.driver_info,
            "scope": "native marker feasibility only; no full profiler or other-platform coverage claim"
        })
    );
}
