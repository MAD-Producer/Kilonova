use super::*;

#[test]
fn capacity_checks_alignment_limit_and_growth_before_allocating() {
    for required in [0, 3] {
        assert!(matches!(
            capacity(required, 64),
            Err(VectorUploadError::Alignment { .. })
        ));
    }
    assert_eq!(capacity(24, 64).unwrap(), 32);
    assert_eq!(capacity(48, 48).unwrap(), 48);
    for required in [68, u64::MAX - 3] {
        assert!(matches!(
            capacity(required, 64),
            Err(VectorUploadError::Size { .. })
        ));
    }
}

#[test]
fn ordered_copies_reuse_completed_uploads_and_retire_abandoned_work() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let device = &renderer.device;
    let queue = &renderer.queue;
    let pool = UploadPool::new(renderer.allocation_budget().clone());
    let mut destination = ReusableBuffer::new(renderer.allocation_budget().clone());
    let target = destination
        .get(
            device,
            4,
            wgpu::BufferUsages::COPY_SRC,
            "upload test target",
        )
        .unwrap();
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ordered vector readback"),
        size: 8,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    for iteration in 0..3 {
        let mut encoder = device.create_command_encoder(&Default::default());
        for (offset, value) in [(0, 17u32 + iteration), (4, 91 + iteration)] {
            pool.write(
                device,
                &mut encoder,
                &target,
                &value.to_le_bytes(),
                7,
                iteration as f64,
            )
            .unwrap();
            encoder.copy_buffer_to_buffer(&target, 0, &readback, offset, 4);
        }
        assert_eq!(pool.info().unwrap().pending_buffers, 2);
        queue.submit([encoder.finish()]);
        let (send, receive) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap()
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receive.recv().unwrap().unwrap();
        let bytes = readback.slice(..).get_mapped_range().unwrap();
        assert_eq!(&bytes[..4], &(17u32 + iteration).to_le_bytes());
        assert_eq!(&bytes[4..], &(91u32 + iteration).to_le_bytes());
        drop(bytes);
        readback.unmap();
        let info = pool.info().unwrap();
        assert_eq!((info.pending_buffers, info.pending_bytes), (0, 0));
        assert_eq!(info.staging_buffers_created_total, 2);
        assert_eq!((info.idle_buffers, info.idle_bytes), (2, 8));
    }
    assert_eq!(destination.created, 1);
    assert_eq!(pool.info().unwrap().reused_uploads_total, 4);
    assert_eq!(pool.info().unwrap().encoded_upload_bytes_total, 24);
    assert_eq!(pool.info().unwrap().completed_upload_bytes_total, 24);

    for finished in [false, true] {
        let mut encoder = device.create_command_encoder(&Default::default());
        pool.write(device, &mut encoder, &target, &[1; 4], 7, 3.)
            .unwrap();
        if finished {
            drop(encoder.finish());
        } else {
            drop(encoder);
        }
        assert_eq!(pool.info().unwrap().pending_buffers, 0);
    }
    assert_eq!(pool.info().unwrap().abandoned_uploads_total, 2);
    assert_eq!(pool.info().unwrap().encoded_upload_bytes_total, 32);
    assert_eq!(pool.info().unwrap().completed_upload_bytes_total, 24);

    // Retiring a working set must not admit old callbacks into the new cache.
    let mut encoder = device.create_command_encoder(&Default::default());
    pool.write(device, &mut encoder, &target, &[2; 4], 7, 4.)
        .unwrap();
    pool.release();
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(pool.info().unwrap().idle_buffers, 0);
    assert_eq!(pool.info().unwrap().pending_buffers, 0);

    // Retained cache is bounded in both handle count and logical capacity.
    let mut encoder = device.create_command_encoder(&Default::default());
    for _ in 0..IDLE_BUFFERS + 2 {
        pool.write(device, &mut encoder, &target, &[3; 4], 7, 5.)
            .unwrap();
    }
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(pool.info().unwrap().idle_buffers, IDLE_BUFFERS as u64);
    assert!(pool.info().unwrap().idle_bytes <= IDLE_LIMIT);
    pool.release();
    assert_eq!(destination.bytes(), 4);
    assert_eq!(pool.info().unwrap().idle_bytes, 0);

    // Capacity grows only when needed; smaller requests retain the grown buffer.
    let large = destination
        .get(device, 12, wgpu::BufferUsages::COPY_SRC, "growth")
        .unwrap();
    assert_eq!(large.size(), 16);
    assert_eq!(destination.created, 2);
    destination
        .get(device, 4, wgpu::BufferUsages::COPY_SRC, "shrink")
        .unwrap();
    assert_eq!(destination.created, 2);

    // One source beyond the byte budget is retired even below the handle limit.
    let bytes = vec![0; IDLE_LIMIT as usize + 4];
    let big = destination
        .get(
            device,
            bytes.len() as u64,
            wgpu::BufferUsages::COPY_SRC,
            "byte cap",
        )
        .unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    pool.write(device, &mut encoder, &big, &bytes, 7, 6.)
        .unwrap();
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    assert_eq!(pool.info().unwrap().idle_buffers, 0);
    assert_eq!(pool.info().unwrap().idle_bytes, 0);
    destination.clear();
    assert_eq!(destination.bytes(), 0);
}

#[test]
fn actual_native_remap_failure_is_retained_and_does_not_retry() {
    let renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let pool = UploadPool::new(renderer.allocation_budget().clone());
    let source = AllocationBuffer::new(&renderer.device, renderer.allocation_budget(), &wgpu::BufferDescriptor {
        label: Some("destroyed remap source"),
        size: 4,
        usage: wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    }).unwrap();
    {
        let mut state = pool.0.lock().unwrap();
        state.info.pending_buffers = 1;
        state.info.pending_bytes = source.size();
    }
    let pending = Pending {
        buffer: Some(source.clone()),
        _destination: None,
        pool: Arc::downgrade(&pool.0),
        encoded_bytes: 4,
        generation: 0,
        layer: 49,
        frame: 3.25,
        began: Instant::now(),
    };
    // This executes the native mapping API, not a supplied Err callback value.
    let scope = renderer
        .device
        .push_error_scope(wgpu::ErrorFilter::Validation);
    source.destroy();
    source
        .slice(..)
        .map_async(wgpu::MapMode::Write, move |result| pending.complete(result));
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    assert!(pollster::block_on(scope.pop()).is_some());
    let info = pool.info().unwrap();
    assert_eq!(
        (
            info.remap_failures_total,
            info.pending_buffers,
            info.idle_bytes
        ),
        (1, 0, 0)
    );
    assert!(matches!(
        pool.0.lock().unwrap().failure,
        Some(VectorUploadError::Remap(_))
    ));
    let target = AllocationBuffer::new(&renderer.device, renderer.allocation_budget(), &wgpu::BufferDescriptor {
        label: None,
        size: 4,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    }).unwrap();
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    assert!(matches!(
        pool.write(&renderer.device, &mut encoder, &target, &[0; 4], 49, 4.),
        Err(RenderError::VectorUpload(VectorUploadError::Remap(_)))
    ));
    pool.release();
    assert!(matches!(
        pool.write(&renderer.device, &mut encoder, &target, &[0; 4], 49, 5.),
        Err(RenderError::VectorUpload(VectorUploadError::Remap(_)))
    ));
    assert_eq!(pool.info().unwrap().staging_buffers_created_total, 0);
    assert_eq!(pool.info().unwrap().completed_upload_bytes_total, 0);
}
