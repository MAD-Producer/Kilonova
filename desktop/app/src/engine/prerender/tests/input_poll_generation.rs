//! The production polling Present must acknowledge only its dispatch-time input.
use super::*;
use crate::engine::prerender::InputActivity;

#[test]
#[ignore = "explicit native polling Present input-generation integration"]
fn native_polling_present_preserves_input_generations_and_frozen_hold() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("project");
    let mut project = motion_core::Project::new(16, 16, 30, 6).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1, "Polling input control", [16.; 2], [8., 8., 0.], [1., 0., 0., 1.],
    ));
    motion_core::storage::save(&directory, &project).unwrap();
    let mut events = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let window = Arc::new(events.create_window(
        winit::window::Window::default_attributes().with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(64, 64)),
    ).unwrap());
    let engine = Engine::start(directory, motion_desktop_media::platform(), 8 << 30).unwrap();
    receive(&mut events, &engine.attach_async(window, 64, 64).unwrap());
    // Finish initial pipeline preparation before controlling the dispatch order.
    present(&mut events, &engine);
    engine.prerender_input(InputActivity::Pulse).unwrap();
    let (entered, entry) = channel();
    let (release, gate) = channel();
    let blocked = engine.request(|reply| Command::TestWait {
        entered, release: gate, reply,
    }).unwrap();
    entry.recv_timeout(Duration::from_secs(5)).unwrap();
    let old = engine.present_poll_async(frame(), false).unwrap();
    // New input arrives while the old frame is queued. It must survive that
    // frame's successful completion, irrespective of the current input token.
    engine.prerender_input(InputActivity::Pulse).unwrap();
    release.send(()).unwrap();
    receive(&mut events, &blocked);
    assert!(receive(&mut events, &old));
    assert!(engine.prerender_signals.input_pending().unwrap());
    assert!(receive(&mut events, &engine.present_poll_async(frame(), true).unwrap()));
    assert!(engine.prerender_signals.input_pending().unwrap(),
        "a frozen polling frame must not acknowledge new composition input");
    assert!(receive(&mut events, &engine.present_poll_async(frame(), false).unwrap()));
    assert!(!engine.prerender_signals.input_pending().unwrap(),
        "the current production polling frame must acknowledge its own input");
    let front = read_front(&mut events, &engine);
    let pixels: Vec<u8> = serde_json::from_value(front["pixels"].clone()).unwrap();
    assert_eq!(pixels.len(), 16 * 16 * 4);
    assert!(pixels.chunks_exact(4).all(|pixel| pixel == [255, 0, 0, 255]));
}
