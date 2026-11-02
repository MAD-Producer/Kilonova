use super::*;

fn mapped(h: &mut Harness) {
    h.engine
        .edit(
            serde_json::to_string(&Command::LayerTime {
                object: 7,
                action: motion_core::layer_time::LayerTimeAction::Stretch {
                    stretch: motion_core::layer_time::TimeStretch::new(3, 2).unwrap(),
                    anchor_frame: 0.,
                    resize_clip: false,
                },
            })
            .unwrap(),
        )
        .unwrap();
    h.engine.seek(6.5).unwrap();
}

#[test]
fn mapped_native_chrome_cancels_stale_numeric_drafts_in_both_owners_and_languages() {
    cases(|h| {
        mapped(h);
        let before = h.project();
        h.click();
        h.type_value("42");
        h.engine.seek(11.).unwrap();
        h.send(press(h.outside));
        assert!(!h.editing());
        assert_eq!(h.project(), before);
        h.click();
        h.type_value("42");
        h.send(press(h.outside));
        assert!(!h.editing());
        assert!((h.value() - 42.).abs() < 0.00001);
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
    });
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires native GPU windows; run explicitly"]
fn native_mapped_numeric_cancel_and_new_draft_render_in_both_owners_and_languages() {
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(
        std::env::var_os("MOTION_NUMERIC_TIME_NATIVE_ARTIFACTS").expect("artifact directory"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let mut events = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    #[allow(deprecated)]
    let windows: Vec<_> = (0..2)
        .map(|_| {
            Arc::new(
                events
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(
                                SIZE[0] as u32,
                                SIZE[1] as u32,
                            )),
                    )
                    .unwrap(),
            )
        })
        .collect();
    cases(|h| {
        mapped(h);
        crate::native_test_support::receive(
            &mut events,
            &h.engine
                .attach_async(windows[0].clone(), SIZE[0] as u32, SIZE[1] as u32)
                .unwrap(),
            Duration::from_secs(30),
        )
        .unwrap()
        .unwrap();
        h.engine
            .float_window(1, windows[1].clone(), SIZE[0] as u32, SIZE[1] as u32)
            .unwrap();
        let name = format!(
            "{:?}-{}-{:?}",
            h.provider,
            if h.floating { "floating" } else { "main" },
            h.panels.locale
        );
        let mut capture = |h: &mut Harness, suffix: &str| {
            let (paint, actions) = h.frame(Input::default());
            assert!(actions.is_empty());
            let config = h.atlas.config();
            let path = root.join(format!("{name}-{suffix}.png"));
            let frame = crate::engine::UiFrame {
                views: None,
                inspection: Default::default(),
                paint,
                atlas: Some(crate::engine::AtlasUpload {
                    pixels: h.atlas.pixel_data().to_vec(),
                    width: config.width,
                    height: config.height,
                }),
                logical_size: SIZE,
                composition: None,
                composition_image: None,
                window: u64::from(h.floating),
                node_preview: None,
            };
            crate::native_test_support::capture_ui(&mut events, &h.engine, frame, path.clone())
                .unwrap();
            let rgba = image::open(path).unwrap().into_rgba8();
            assert_eq!(rgba.dimensions(), (SIZE[0] as u32, SIZE[1] as u32));
            assert!(rgba.pixels().any(|p| p.0[..3] != [0, 0, 0]));
        };
        let before = h.project();
        h.click();
        h.type_value("42");
        capture(h, "draft");
        h.engine.seek(11.).unwrap();
        h.send(press(h.outside));
        assert!(!h.editing());
        assert_eq!(h.project(), before);
        capture(h, "cancelled");
        h.click();
        h.type_value("42");
        h.send(press(h.outside));
        assert!(!h.editing());
        assert!((h.value() - 42.).abs() < 0.00001);
        capture(h, "committed");
        let after = h.project();
        assert_eq!(h.engine.history(0).unwrap()["project"], before);
        assert_eq!(h.engine.history(1).unwrap()["project"], after);
        std::fs::write(
            root.join(format!("{name}-project.json")),
            serde_json::to_vec_pretty(&after).unwrap(),
        )
        .unwrap();
        h.engine.close_window(1).unwrap();
    });
}
