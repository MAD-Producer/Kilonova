//! Exercise actual Present and CaptureUi channel routing through the worker.
use super::*;
use motion_render::ViewChannel;

pub(super) fn qualify(
    engine: &Engine,
    frame: impl Fn() -> crate::engine::UiFrame,
    count: usize,
    working: [f32; 3],
    color: [u8; 3],
    root: &std::path::Path,
) {
    let before = engine.state().unwrap();
    let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
    let channels = [
        ViewChannel::Red,
        ViewChannel::Green,
        ViewChannel::Blue,
        ViewChannel::Alpha,
    ];
    for phase in 0..5 {
        let channel = |i: usize| {
            if phase == 4 {
                ViewChannel::Color
            } else {
                channels[(i + phase) % 4]
            }
        };
        let make = || {
            let mut ui = frame();
            if let Some(plan) = &mut ui.views {
                for (i, cell) in plan.cells.iter_mut().enumerate() {
                    cell.inspection.channel = channel(i);
                }
            } else {
                ui.inspection.channel = channel(0);
            }
            ui
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        while !engine
            .present_async(make())
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap()
        {
            assert!(
                Instant::now() < deadline,
                "component Present did not complete"
            );
        }
        let path = root.join(format!("managed-components-{count}-{phase}.png"));
        engine.capture_ui(make(), path.clone()).unwrap();
        let pixels = image::open(path).unwrap().to_rgba8();
        for i in 0..count {
            let (cx, cy) = match count {
                1 => (32, 32),
                2 => (i * 32 + 16, 32),
                _ => (i % 2 * 32 + 16, i / 2 * 32 + 16),
            };
            let expected = match channel(i) {
                ViewChannel::Color => color,
                ViewChannel::Alpha => [255; 3],
                ch => [(working[ch as usize - 1].clamp(0., 1.) * 255.).round() as u8; 3],
            };
            for y in cy - 2..cy + 2 {
                for x in cx - 2..cx + 2 {
                    let pixel = pixels.get_pixel(x as u32, y as u32).0;
                    assert!(
                        (0..3).all(|c| pixel[c].abs_diff(expected[c]) <= 1),
                        "component worker cells={count} phase={phase} cell={i} actual={pixel:?} CPU={expected:?}"
                    );
                    assert_eq!(pixel[3], 255);
                }
            }
        }
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            attempts,
            "channel Present and CaptureUi must not sample"
        );
        let after = engine.state().unwrap();
        for key in [
            "project", "revision", "frame", "history", "canUndo", "canRedo",
        ] {
            assert_eq!(after[key], before[key], "channel inspection changed {key}");
        }
    }
}
