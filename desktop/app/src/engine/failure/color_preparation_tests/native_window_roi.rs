//! Real worker ROI requests, PNG pixels, invalid last cells and recovery.
use super::*;
use std::path::Path;

pub(super) fn qualify(
    engine: &Engine,
    frame: impl Fn() -> crate::engine::UiFrame,
    count: usize,
    expected: [u8; 3],
    root: &Path,
) {
    let before = engine.state().unwrap();
    let regions = [
        [0.25, 0.25, 0.5, 0.5],
        [0., 0., 0.25, 0.5],
        [0.5, 0., 0.5, 0.25],
        [0.5, 0.5, 0.5, 0.5],
    ];
    let make = |panned: bool| {
        let mut ui = frame();
        if let Some(plan) = &mut ui.views {
            for (index, cell) in plan.cells.iter_mut().enumerate() {
                cell.inspection.region = Some(regions[index]);
                if panned {
                    let [x, y, w, h] = cell.placement.viewport;
                    cell.placement.image = Some([x - w * 0.25, y - h * 0.125, w * 1.5, h * 1.25]);
                }
            }
        } else {
            ui.inspection.region = Some(regions[0]);
            if panned {
                ui.composition_image = Some([-16., -8., 96., 80.]);
            }
        }
        ui
    };
    let present = |make: &dyn Fn() -> crate::engine::UiFrame| {
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
                "managed ROI Present did not complete"
            );
        }
    };
    for panned in [false, true] {
        present(&|| make(panned));
        let sampled = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
        present(&|| make(panned));
        let path = root.join(format!(
            "managed-roi-{}{count}.png",
            if panned { "pan-" } else { "" }
        ));
        engine.capture_ui(make(panned), path.clone()).unwrap();
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            sampled,
            "ROI repaint and capture must not sample"
        );
        let pixels = image::open(path).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (64, 64));
        let mut visible = 0;
        for y in 0..64 {
            for x in 0..64 {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let inside = (0..count).any(|index| {
                    let (vx, vy, vw, vh) = match count {
                        1 => (0., 0., 64., 64.),
                        2 => (index as f32 * 32., 0., 32., 64.),
                        _ => ((index % 2) as f32 * 32., (index / 2) as f32 * 32., 32., 32.),
                    };
                    if px < vx || px >= vx + vw || py < vy || py >= vy + vh {
                        return false;
                    }
                    let (ix, iy, iw, ih) = if panned {
                        (vx - vw * 0.25, vy - vh * 0.125, vw * 1.5, vh * 1.25)
                    } else {
                        let side = vw.min(vh);
                        (vx + (vw - side) / 2., vy + (vh - side) / 2., side, side)
                    };
                    let u = (px - ix) / iw;
                    let v = (py - iy) / ih;
                    let [rx, ry, rw, rh] = regions[index];
                    u >= rx && u < rx + rw && v >= ry && v < ry + rh
                });
                let actual = pixels.get_pixel(x, y).0;
                if inside {
                    assert!(
                        (0..3).all(|c| actual[c].abs_diff(expected[c]) <= 1),
                        "ROI count={count} pan={panned} ({x},{y}) actual={actual:?} CPU={expected:?}"
                    );
                    assert_eq!(actual[3], 255);
                    visible += 1;
                } else {
                    assert_eq!(
                        actual,
                        [0, 0, 0, 255],
                        "outside ROI must clear previous pixels"
                    );
                }
            }
        }
        assert!(visible > 100 && visible < 3000);
    }
    // The final cell is invalid: nothing may publish or be written to disk.
    let invalid = || {
        let mut ui = frame();
        let region = Some([0.75, 0., 0.5, 0.5]);
        if let Some(plan) = &mut ui.views {
            plan.cells.last_mut().unwrap().inspection.region = region;
        } else {
            ui.inspection.region = region;
        }
        ui
    };
    let sampled = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
    let error = engine
        .present_async(invalid())
        .unwrap()
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("phase=inspection"), "{error}");
    let path = root.join(format!("invalid-roi-{count}.png"));
    let error = engine.capture_ui(invalid(), path.clone()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("invalid viewer inspection viewport, image or region"),
        "{error}"
    );
    assert!(!path.exists());
    assert_eq!(
        engine.preview_info().unwrap()["video"]["renderAttempts"],
        sampled,
        "an invalid final ROI must not sample"
    );
    present(&frame);
    let path = root.join(format!("managed-display-{count}.png"));
    engine.capture_ui(frame(), path.clone()).unwrap();
    let pixels = image::open(path).unwrap().to_rgba8();
    // A point outside the central ROI must become visible after disabling it.
    assert!((0..3).all(|c| pixels.get_pixel(24, 32).0[c].abs_diff(expected[c]) <= 1));
    let after = engine.state().unwrap();
    for key in [
        "project", "revision", "frame", "history", "canUndo", "canRedo",
    ] {
        assert_eq!(after[key], before[key], "ROI changed {key}");
    }
}
