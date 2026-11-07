use super::*;

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn unmap_guard_cleans_pending_and_successful_maps_without_repeating_a_failed_map() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    for phase in ["pending", "success", "failed"] {
        let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback cleanup state"),
            size: 256,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        if phase == "failed" {
            buffer.unmap();
        }
        if phase != "pending" {
            renderer
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
        }
        drop(UnmapOnDrop {
            buffer: &buffer,
            receive: &receive,
            failed: false,
        });
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        if phase == "pending" {
            assert!(receive.recv().unwrap().is_err());
        } else {
            assert!(matches!(
                receive.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            ));
        }
        assert!(buffer.slice(..).get_mapped_range().is_err());
        renderer.check_health().unwrap();
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                send.send(result).unwrap();
            });
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        receive.recv().unwrap().unwrap();
        {
            let view = buffer.slice(..).get_mapped_range().unwrap();
            assert_eq!(&*view, &[0; 256]);
        }
        buffer.unmap();
        renderer.check_health().unwrap();
    }
}
