//! Actual worker capture, edit, comparison, save, failure and history recovery.
use super::*;
use motion_render::ViewChannel;

pub(super) fn qualify(
    engine: &Engine,
    frame: impl Fn() -> crate::engine::UiFrame,
    count: usize,
    old: [f32; 4],
    new: [f32; 4],
    old_color: [u8; 3],
    new_color: [u8; 3],
    root: &std::path::Path,
) {
    let present = |ui: crate::engine::UiFrame| {
        engine
            .present_async(ui)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap()
    };
    let ready = |make: &dyn Fn() -> crate::engine::UiFrame| {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !present(make()) {
            assert!(
                Instant::now() < deadline,
                "managed snapshot worker did not present"
            );
        }
    };
    ready(&frame);
    let original = engine.state().unwrap();
    let mut dimensions = [0; 2];
    let mut generation = serde_json::Value::Null;
    for index in 0..count {
        let active = || {
            let mut ui = frame();
            if let Some(plan) = &mut ui.views {
                plan.active = index;
            }
            ui
        };
        ready(&active);
        let snapshot = engine
            .capture_viewer_snapshot_async(0)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(
            snapshot["cell"],
            if count == 1 {
                serde_json::Value::Null
            } else {
                serde_json::json!(index)
            }
        );
        dimensions = [
            snapshot["width"].as_u64().unwrap() as u32,
            snapshot["height"].as_u64().unwrap() as u32,
        ];
        // The ordinary frame below selects cell zero for comparisons and save.
        // Keep that cell's identity, rather than the last captured cell's.
        if index == 0 {
            generation = snapshot["generation"].clone();
        }
    }
    if count == 1 {
        let normal_limit = engine.allocation_budget.info().unwrap().budget_bytes;
        engine.allocation_budget.set_limit(1).unwrap();
        let capture_error = engine
            .capture_viewer_snapshot_async(0)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        budget_error(&capture_error);
        let refused = root.join("managed-snapshot-refused.png");
        let save_error = engine
            .save_viewer_snapshot_async(0, refused.clone(), ViewChannel::Alpha)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        budget_error(&save_error);
        assert!(!refused.exists());
        engine.allocation_budget.set_limit(normal_limit).unwrap();
    }
    engine
        .edit(
            serde_json::to_string(&motion_core::Command::SetScalar {
                object: 1,
                property: motion_core::Property::Opacity,
                frame: 0,
                value: 0.25,
            })
            .unwrap(),
        )
        .unwrap();
    ready(&frame);
    let before = engine.state().unwrap();
    let attempts = engine.preview_info().unwrap()["video"]["renderAttempts"].clone();
    let channels = [
        ViewChannel::Red,
        ViewChannel::Green,
        ViewChannel::Blue,
        ViewChannel::Alpha,
    ];
    for phase in 0..5 {
        let settings = |i: usize| crate::engine::ViewerInspectionFrame {
            channel: if phase == 4 {
                ViewChannel::Color
            } else {
                channels[(i + phase) % 4]
            },
            region: (phase % 2 == 1).then_some([0.25, 0.25, 0.5, 0.5]),
            comparison_split: Some([0.5, 0., 1., 0.25][i]),
        };
        let make = || {
            let mut ui = frame();
            if let Some(plan) = &mut ui.views {
                for (i, cell) in plan.cells.iter_mut().enumerate() {
                    cell.inspection = settings(i);
                }
            } else {
                ui.inspection = settings(0);
            }
            ui
        };
        ready(&make);
        let path = root.join(format!("managed-snapshot-comparison-{count}-{phase}.png"));
        engine.capture_ui(make(), path.clone()).unwrap();
        let pixels = image::open(path).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), (64, 64));
        for y in 0..64 {
            for x in 0..64 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut expected = [0, 0, 0, 255];
                for i in 0..count {
                    let (vx, vy, vw, vh) = match count {
                        1 => (0., 0., 64., 64.),
                        2 => (i as f32 * 32., 0., 32., 64.),
                        _ => ((i % 2) as f32 * 32., (i / 2) as f32 * 32., 32., 32.),
                    };
                    if px < vx || px >= vx + vw || py < vy || py >= vy + vh {
                        continue;
                    }
                    let side = vw.min(vh);
                    let (ix, iy) = (vx + (vw - side) / 2., vy + (vh - side) / 2.);
                    let (u, v) = ((px - ix) / side, (py - iy) / side);
                    let inspection = settings(i);
                    let [rx, ry, rw, rh] = inspection.region.unwrap_or([0., 0., 1., 1.]);
                    if u < 0.
                        || u >= 1.
                        || v < 0.
                        || v >= 1.
                        || u < rx
                        || u >= rx + rw
                        || v < ry
                        || v >= ry + rh
                    {
                        continue;
                    }
                    let frozen = u >= inspection.comparison_split.unwrap();
                    expected = if inspection.channel == ViewChannel::Color {
                        let rgb = if frozen { old_color } else { new_color };
                        [rgb[0], rgb[1], rgb[2], 255]
                    } else {
                        let p = if frozen { old } else { new };
                        let c =
                            (p[inspection.channel as usize - 1].clamp(0., 1.) * 255.).round() as u8;
                        [c, c, c, 255]
                    };
                }
                let actual = pixels.get_pixel(x, y).0;
                assert!((0..4).all(|c|actual[c].abs_diff(expected[c])<=1),
                "snapshot worker count={count} phase={phase} ({x},{y}) actual={actual:?} CPU={expected:?}");
            }
        }
        assert_eq!(
            engine.preview_info().unwrap()["video"]["renderAttempts"],
            attempts
        );
        let after = engine.state().unwrap();
        for key in [
            "project", "revision", "frame", "history", "canUndo", "canRedo",
        ] {
            assert_eq!(after[key], before[key]);
        }
    }
    for (name, channel) in [
        ("color", ViewChannel::Color),
        ("red", ViewChannel::Red),
        ("green", ViewChannel::Green),
        ("alpha", ViewChannel::Alpha),
    ] {
        let path = root.join(format!("managed-snapshot-saved-{count}-{name}.png"));
        let saved = engine
            .save_viewer_snapshot_async(0, path.clone(), channel)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap();
        assert_eq!(saved["kind"], "display");
        assert_eq!(
            saved["generation"], generation,
            "failed capture replaced a retained snapshot"
        );
        let pixels = image::open(path).unwrap().to_rgba8();
        assert_eq!([pixels.width(), pixels.height()], dimensions);
        let expected = if channel == ViewChannel::Color {
            [old_color[0], old_color[1], old_color[2], 255]
        } else {
            let c = (old[channel as usize - 1].clamp(0., 1.) * 255.).round() as u8;
            [c, c, c, 255]
        };
        assert!(pixels
            .pixels()
            .all(|p| (0..4).all(|c| p.0[c].abs_diff(expected[c]) <= 1)));
    }
    if count == 1 {
        let blocked = root.join("blocked-snapshot.png");
        fs::create_dir(&blocked).unwrap();
        let error = engine
            .save_viewer_snapshot_async(0, blocked, ViewChannel::Color)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("phase=save managed display snapshot"),
            "{error}"
        );
        let motion_host::Error::Image(image::ImageError::IoError(cause)) = native(&error) else {
            panic!("managed PNG save lost native image IO error: {error:?}");
        };
        assert!(matches!(
            cause.kind(),
            std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::IsADirectory
        ));
        assert!(cause.raw_os_error().is_some());
        let clock = before["frame"].as_f64().unwrap();
        engine.seek(clock + 1.).unwrap();
        let error = engine
            .capture_viewer_snapshot_async(0)
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("no longer current"), "{error}");
        engine.seek(clock).unwrap();
        ready(&frame);
    }
    engine
        .history(motion_host::ops::editing::HISTORY_UNDO)
        .unwrap();
    ready(&frame);
    assert_eq!(
        engine.state().unwrap()["project"],
        original["project"],
        "history restore changed source project"
    );
    println!("managed snapshot worker cells={count} comparison_pixels=20480; four frozen PNG channels; project restored");
}

fn budget_error(error: &crate::engine::Error) {
    let cause = native(error);
    let motion_host::Error::Render(cause) = cause else {
        panic!("snapshot lost native RenderError: {error:?}")
    };
    assert!(
        matches!(cause.resource_rejection(),Some(motion_render::ResourceRejection::Budget {
            required_bytes,budget_bytes:1,..
        }) if *required_bytes>1),
        "{error:?}"
    );
}

fn native(error: &crate::engine::Error) -> &motion_host::Error {
    let mut cause = error
        .host()
        .expect("managed snapshot must preserve its native host cause");
    while let motion_host::Error::Context { cause: next, .. } = cause {
        cause = next;
    }
    cause
}
