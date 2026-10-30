use super::*;

fn policy() -> schedule::Policy {
    schedule::Policy {
        enabled: true,
        window: 2,
        ..Default::default()
    }
}
fn demand() -> Demand {
    Demand {
        frame: 0.,
        direction: schedule::Direction::Forward,
        playing: true,
        editing: false,
        foreground_pending: false,
        pressure: false,
        now: Duration::ZERO,
    }
}
fn session(root: &Path) -> Session {
    let mut project = Project::new(32, 24, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Coordinator pixels",
        [32., 24.],
        [16., 12., 0.],
        [1., 0., 0., 1.],
    ));
    Session::new(project, root.into(), crate::platform()).unwrap()
}
fn prepare(owner: &mut Coordinator, session: &Session, renderer: &Renderer) {
    let mut plan =
        PrerenderPlan::capture(session, 1, 2, 82, 3072, &ExportControl::default()).unwrap();
    plan.adapter = Some(renderer.adapter.clone());
    assert!(owner.prepare(plan, ExportControl::default()).unwrap());
    drain(owner, renderer);
    assert!(owner.profile.is_some());
}
fn drain(owner: &mut Coordinator, renderer: &Renderer) -> bool {
    let began = Instant::now();
    let mut installed = false;
    while owner.active.is_some() {
        installed |= owner.poll(renderer).unwrap();
        assert!(
            began.elapsed() < Duration::from_secs(30),
            "coordinator failed to acknowledge"
        );
        std::thread::yield_now();
    }
    installed
}
fn pixels(renderer: &Renderer, source: &motion_render::RenderTarget) -> Vec<u8> {
    // Test-only raw readback; production cache lookup never allocates/maps CPU buffers.
    let stride = 256;
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("coordinator native oracle"),
        size: stride * 24,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .allocation_command_encoder(&Default::default())
        .unwrap();
    encoder.copy_texture_to_buffer(
        source.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride as u32),
                rows_per_image: Some(24),
            },
        },
        wgpu::Extent3d {
            width: 32,
            height: 24,
            depth_or_array_layers: 1,
        },
    );
    renderer.submit_allocated(encoder.finish(), |_| {}).unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| send.send(r).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receive.recv().unwrap().unwrap();
    let raw = buffer.slice(..).get_mapped_range().unwrap();
    let result = raw
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..128].iter().copied())
        .collect();
    drop(raw);
    buffer.unmap();
    result
}

#[test]
fn disabled_coordinator_owns_no_worker_or_native_cache() {
    let owner = Coordinator::new(Default::default(), 0, 0).unwrap();
    assert!(owner.observation().is_none());
    assert!(owner.cache_info().is_none());
    assert!(owner.active.is_none());
    assert!(!owner.status().enabled);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_coordinator_admits_before_dispatch_installs_exact_pixels_and_counts_hits() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert_eq!(owner.cache_info().unwrap().pending_bytes, 3072);
    assert_eq!(owner.cache_info().unwrap().resident_frames, 0);
    assert!(!owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    let frame = owner.get(&renderer, 1.).unwrap().unwrap();
    assert_eq!(frame.key().frame(), 1.);
    assert_eq!(
        pixels(&renderer, frame.source()),
        [255, 0, 0, 255].repeat(32 * 24)
    );
    assert_eq!(owner.status().counts.completed, 1);
    let info = owner.cache_info().unwrap();
    assert_eq!(
        (info.hits, info.misses, info.installed, info.pending_bytes),
        (1, 1, 1, 0)
    );
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn invalidated_actual_job_keeps_its_slot_until_ack_and_cannot_install_then_recovers() {
    let root = tempfile::tempdir().unwrap();
    let session = session(root.path());
    let renderer = pollster::block_on(Renderer::headless_with_working_precision(
        session.engine.project().working_precision,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut owner = Coordinator::new(policy(), 3072, 6144).unwrap();
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    owner.invalidate().unwrap();
    assert!(owner.active.is_some());
    assert!(!owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(!drain(&mut owner, &renderer));
    assert_eq!(owner.cache_info().unwrap().installed, 0);
    assert!(owner.get(&renderer, 1.).unwrap().is_none());
    assert_eq!(
        owner
            .observation()
            .unwrap()
            .snapshot()
            .unwrap()
            .retained_cpu_bytes,
        0
    );
    prepare(&mut owner, &session, &renderer);
    owner.update(demand()).unwrap();
    assert!(owner.schedule(&renderer, ExportControl::default()).unwrap());
    assert!(drain(&mut owner, &renderer));
    assert!(owner.get(&renderer, 1.).unwrap().is_some());
}
