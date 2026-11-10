//! Read the actual ordinary window surface, including its format conversion.
use super::*;
use motion_core::{
    vector::{ShapeKind, VectorContent},
    Command, Content, Track,
};
use motion_effects::native_color::NativeColorRuntime;
use motion_host::{session::viewer::View, Session, SurfaceTarget};
use motion_render::{
    multiview::{ViewInspection, ViewPlacement},
    ViewCamera,
};
use std::{collections::BTreeMap, time::Instant};
use winit::platform::windows::EventLoopBuilderExtWindows;
#[path = "authored_window_components.rs"]
mod components;
#[path = "authored_window_compositions.rs"]
mod compositions;
#[path = "authored_window_capture.rs"]
mod display_capture;
#[path = "authored_window_images.rs"]
mod images;
#[path = "authored_window_recording.rs"]
mod recording;
#[path = "authored_window_roi.rs"]
mod roi;
#[path = "authored_window_snapshots.rs"]
mod snapshots;

fn attach(session: &mut Session, events: &winit::event_loop::EventLoop<()>) {
    attach_profiled(session, events, false);
}

fn attach_profiled(
    session: &mut Session,
    events: &winit::event_loop::EventLoop<()>,
    profiled: bool,
) {
    #[allow(deprecated)]
    let window = Arc::new(
        events
            .create_window(
                winit::window::Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(winit::dpi::PhysicalSize::new(128, 128)),
            )
            .unwrap(),
    );
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let surface = instance.create_surface(window).unwrap();
    let renderer = pollster::block_on(Renderer::new_profiled_with_precision_changes(
        &instance,
        Some(&surface),
        wgpu::TextureFormat::Rgba8UnormSrgb,
        profiled,
        session.allocation_budget().clone(),
    ))
    .unwrap();
    let mut config = surface
        .get_default_config(&renderer.adapter, 128, 128)
        .unwrap();
    assert!(surface
        .get_capabilities(&renderer.adapter)
        .usages
        .contains(wgpu::TextureUsages::COPY_SRC));
    config.usage |= wgpu::TextureUsages::COPY_SRC;
    session
        .attach(SurfaceTarget {
            instance,
            surface,
            renderer,
            config,
        })
        .unwrap();
    session.preview.mode = motion_render::PreviewMode::High;
}

fn views(count: usize) -> Vec<View<'static>> {
    (0..count)
        .map(|index| View {
            camera: ViewCamera::Composition,
            placement: ViewPlacement {
                viewport: match count {
                    1 => [0., 0., 128., 128.],
                    2 => [index as f32 * 64., 0., 64., 128.],
                    4 => [(index % 2) as f32 * 64., (index / 2) as f32 * 64., 64., 64.],
                    _ => panic!("unsupported fixture layout"),
                },
                image: None,
            },
        })
        .collect()
}

fn capture(session: &mut Session, frame: f64, views: &[View<'_>], retained: bool) -> Vec<u8> {
    capture_owner(session, frame, views, retained, 77)
}
fn capture_owner(
    session: &mut Session,
    frame: f64,
    views: &[View<'_>],
    retained: bool,
    owner: u64,
) -> Vec<u8> {
    capture_inspected(session, frame, views, retained, owner, None)
}
fn capture_inspected(
    session: &mut Session,
    frame: f64,
    views: &[View<'_>],
    retained: bool,
    owner: u64,
    inspections: Option<&[ViewInspection<'_>]>,
) -> Vec<u8> {
    let graphics = session.graphics.as_ref().unwrap();
    let format = graphics.config.format;
    let buffer = graphics
        .renderer
        .device
        .create_buffer(&wgpu::BufferDescriptor {
            label: Some("actual managed surface readback"),
            size: 128 * 128 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let overlay =
            |_: &Renderer, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView| {
                encoder.copy_texture_to_buffer(
                    wgpu::TexelCopyTextureInfo {
                        texture: view.texture(),
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyBufferInfo {
                        buffer: &buffer,
                        layout: wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(512),
                            rows_per_image: Some(128),
                        },
                    },
                    wgpu::Extent3d {
                        width: 128,
                        height: 128,
                        depth_or_array_layers: 1,
                    },
                );
            };
        let done = if let Some(inspections) = inspections {
            if retained {
                session.present_retained_views_with_inspection(
                    owner,
                    views,
                    inspections,
                    false,
                    overlay,
                )
            } else {
                session.render_views_with_inspection(owner, frame, views, inspections, overlay)
            }
        } else if owner == 0 && retained {
            session.present_retained_with_view(
                views[0].placement.viewport,
                views[0].placement.image,
                overlay,
            )
        } else if owner == 0 {
            session.render_with_view(
                frame,
                Some(views[0].placement.viewport),
                views[0].placement.image,
                overlay,
            )
        } else if retained {
            session.present_retained_views_with_overlay(owner, views, false, overlay)
        } else {
            session.render_views_with_overlay(owner, frame, views, overlay)
        }
        .unwrap();
        if done {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "ordinary managed window never became ready"
        );
    }
    let (tx, rx) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
    session
        .graphics
        .as_ref()
        .unwrap()
        .renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(Duration::from_secs(30)).unwrap().unwrap();
    let mut pixels = buffer.slice(..).get_mapped_range().unwrap().to_vec();
    if matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    buffer.unmap();
    pixels
}

fn check(pixels: &[u8], count: usize, expected: [[u8; 4]; 2]) -> usize {
    let mut checked = 0;
    for index in 0..count {
        let (x, y, scale) = match count {
            1 => (0, 0, 2),
            2 => (index * 64, 32, 1),
            4 => (index % 2 * 64, index / 2 * 64, 1),
            _ => unreachable!(),
        };
        // Entire constant interiors, away from the bilinear seam/shape edges.
        for tile in 0..2 {
            for py in 12..52 {
                for px in 8..24 {
                    let offset = ((y + py * scale) * 128 + x + (tile * 32 + px) * scale) * 4;
                    let actual = &pixels[offset..offset + 4];
                    assert!(
                        (0..4)
                            .all(|channel| actual[channel].abs_diff(expected[tile][channel]) <= 1),
                        "layout={count} view={index} tile={tile} actual={actual:?} expected={:?}",
                        expected[tile]
                    );
                    checked += 1;
                }
            }
        }
    }
    if count == 2 {
        assert_eq!(&pixels[..4], &[0, 0, 0, 255]);
    }
    checked
}

fn expected(reference: &Reference, case: &Case, runtime: &NativeColorRuntime) -> [[u8; 4]; 2] {
    let item = &reference.configs[case.config];
    let verified = VerifiedColorConfig::new(
        item.manifest.clone(),
        Arc::from(item.config.as_bytes()),
        BTreeMap::new(),
    )
    .unwrap();
    let native = runtime
        .prepare(
            &verified,
            &item.selection,
            ColorProcessorPhase::WorkingToDisplay,
            AlphaMode::Premultiplied,
            AlphaMode::Premultiplied,
        )
        .unwrap();
    [3, 4].map(|sample| {
        let working = premult(case.expected[sample], case.processor.output_alpha);
        let rgb = native
            .apply_rgb([working[0], working[1], working[2]])
            .unwrap();
        [rgb[0], rgb[1], rgb[2], 1.].map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
    })
}

fn qualify(events: &winit::event_loop::EventLoop<()>) {
    let reference = reference();
    let runtime = NativeColorRuntime::load(&runtime()).unwrap();
    let mut checked = 0;
    for case in reference
        .cases
        .iter()
        .filter(|case| case.processor.phase == ColorProcessorPhase::InputToWorking)
    {
        let root = tempfile::tempdir().unwrap();
        let mut project = document(
            &reference,
            case.config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut project).display.transfer = ColorTransfer::Srgb;
        project.layers.push(Layer::solid(
            1,
            "native window left",
            [32., 64.],
            [16., 32., 0.],
            case.input[3],
        ));
        let mut right = Layer::solid(
            2,
            "native window right",
            [32., 64.],
            [48., 32., 0.],
            [1.; 4],
        );
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        vector.fill = Some(Track::constant(case.input[4]));
        right.content = Content::Vector { vector };
        project.layers.push(right);
        project.validate().unwrap();
        let original_expected = expected(&reference, case, &runtime);
        let id = motion_host::session::open(
            root.path().to_owned(),
            project.clone(),
            Arc::new(session_installation::NativePlatform(super::runtime())),
        )
        .unwrap();
        motion_host::session::with_session(id, |session| {
            attach(session, &events);
            let config_path = root
                .path()
                .join(&project.color_management.resources().unwrap()[0].path);
            std::fs::remove_file(&config_path).unwrap();
            for count in [1, 2, 4] {
                let cells = views(count);
                let frame = count as f64 / 8.;
                let pixels = capture(session, frame, &cells, false);
                checked += check(&pixels, count, original_expected);
                assert_eq!(session.last_presented_frame, Some(frame));
                assert!(session.view_group_current(77, &cells, false)?);
                assert_eq!(capture(session, frame, &cells, true), pixels);
            }
            let cells = views(4);
            let previous = capture(session, 0.5, &cells, true);
            let before = serde_json::to_vec(session.engine.project()).unwrap();
            let history = session.engine.history_diagnostics();
            let frame = session.last_presented_frame;
            let mut changed = project.clone();
            settings(&mut changed).output.alpha = AlphaMode::Straight;
            if changed.color_management == project.color_management {
                settings(&mut changed).output.alpha = AlphaMode::Premultiplied;
            }
            assert!(
                session
                    .apply_command_batch(vec![Command::SetColorManagement {
                        settings: changed.color_management.clone(),
                    }])
                    .is_err(),
                "missing config must reject edit before model publication"
            );
            assert_eq!(
                serde_json::to_vec(session.engine.project()).unwrap(),
                before
            );
            assert_eq!(session.engine.history_diagnostics(), history);
            assert_eq!(session.last_presented_frame, frame);
            assert_eq!(capture(session, 0.5, &cells, true), previous);

            let policy = session.graphics.as_ref().unwrap().renderer.memory_policy();
            let mut limited = policy;
            limited.preview_target_budget_bytes = 32;
            session.configure_memory_policy(limited)?;
            for _ in 0..2 {
                assert!(session
                    .render_views_with_overlay(77, 0.75, &cells, |_, _, _| panic!(
                        "rejected group published"
                    ))
                    .is_err());
                assert_eq!(session.last_presented_frame, frame);
                assert_eq!(
                    serde_json::to_vec(session.engine.project()).unwrap(),
                    before
                );
            }
            session.configure_memory_policy(policy)?;
            checked += check(&capture(session, 0.75, &cells, false), 4, original_expected);
            let mut inspections = [ViewInspection::default(); 4];
            let legacy = session
                .graphics
                .as_ref()
                .unwrap()
                .renderer
                .render_target(64, 64)?;
            inspections[1].comparison = Some((&legacy, 0.5));
            assert!(session
                .render_views_with_inspection(77, 1., &cells, &inspections, |_, _, _| panic!(
                    "unqualified inspection published"
                ))
                .is_err());
            assert_eq!(session.last_presented_frame, Some(0.75));
            assert_eq!(capture(session, 0.75, &cells, true), previous);
            session.clear_view_group()?;
            assert!(
                session.view_group_current(77, &cells, true)?,
                "clearing the legacy pool must preserve the shared managed owner"
            );
            checked += check(&capture(session, 1., &cells, false), 4, original_expected);
            assert_eq!(
                serde_json::to_vec(session.engine.project()).unwrap(),
                before
            );
            let single = views(1);
            let pixels = capture_owner(session, 1.25, &single, false, 0);
            checked += check(&pixels, 1, original_expected);
            assert_eq!(capture_owner(session, 1.25, &single, true, 0), pixels);
            let attempts = session.render_attempts;
            session.frame = 1.375;
            assert_eq!(capture_owner(session, 1.375, &single, true, 0), pixels);
            assert_eq!(session.frame, 1.375);
            assert_eq!(session.last_presented_frame, Some(1.25));
            assert_eq!(
                session.render_attempts, attempts,
                "held single-view repaint must not resample the advanced clock"
            );
            std::fs::write(&config_path, &reference.configs[case.config].config).unwrap();
            Ok(())
        })
        .unwrap();

        // Change the real native programme, then traverse ordinary public history.
        // Old cells must not become the spare group for a different programme.
        let next_config = (case.config + 1) % reference.configs.len();
        let next_case = reference
            .cases
            .iter()
            .find(|candidate| {
                candidate.config == next_config
                    && candidate.processor.phase == ColorProcessorPhase::InputToWorking
                    && candidate.processor.input_alpha == case.processor.input_alpha
            })
            .unwrap();
        assert_eq!(case.input, next_case.input);
        let mut next = document(
            &reference,
            next_config,
            case.processor.input_alpha,
            root.path(),
        );
        settings(&mut next).display.transfer = ColorTransfer::Srgb;
        let next_expected = expected(&reference, next_case, &runtime);
        motion_host::session::with_session(id, |session| {
            session.apply_command_batch(vec![Command::SetColorManagement {
                settings: next.color_management.clone(),
            }])?;
            checked += check(&capture(session, 1.5, &views(4), false), 4, next_expected);
            assert_eq!(
                session.engine.project().color_management,
                next.color_management
            );
            Ok(())
        })
        .unwrap();
        use motion_host::ops::editing::{history, HISTORY_REDO, HISTORY_UNDO};
        history(id, HISTORY_UNDO).unwrap();
        motion_host::session::with_session(id, |session| {
            assert_eq!(
                session.engine.project().color_management,
                project.color_management
            );
            checked += check(
                &capture(session, 1.75, &views(4), false),
                4,
                original_expected,
            );
            Ok(())
        })
        .unwrap();
        history(id, HISTORY_REDO).unwrap();
        motion_host::session::with_session(id, |session| {
            assert_eq!(
                session.engine.project().color_management,
                next.color_management
            );
            checked += check(&capture(session, 2., &views(4), false), 4, next_expected);
            Ok(())
        })
        .unwrap();
        motion_host::session::close(id);
    }
    assert_eq!(checked, 215_040);
    println!(
        "actual Windows surface native CPU pixels={checked}; 6 inputs; 1/2/4 views; retained repaint; failed edit/budget atomicity; native programme switch and undo/redo; no config IO during draws"
    );
}

#[test]
#[ignore = "requires an actual Windows surface, GPU and pinned native OCIO runtime"]
fn ordinary_managed_window_publishes_whole_groups_and_preserves_failed_edits() {
    const CHILD: &str = "MOTION_COLOR_WINDOW_CHILD";
    const TEST: &str = "authored::window::ordinary_managed_window_publishes_whole_groups_and_preserves_failed_edits";
    if std::env::var_os(CHILD).is_some() {
        let events = winit::event_loop::EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap();
        qualify(&events);
        images::qualify(&events);
        display_capture::qualify(&events);
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let stdout_path = root.path().join("child.stdout");
    let stderr_path = root.path().join("child.stderr");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            TEST,
            "--include-ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .stdout(std::fs::File::create(&stdout_path).unwrap())
        .stderr(std::fs::File::create(&stderr_path).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let killed = child.kill();
            let joined = child.wait();
            panic!("actual window child exceeded 120s: kill={killed:?} wait={joined:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = std::fs::read_to_string(stdout_path).unwrap();
    let stderr = std::fs::read_to_string(stderr_path).unwrap();
    if let Some(evidence) = std::env::var_os("MOTION_COLOR_TEST_EVIDENCE") {
        let evidence = std::path::PathBuf::from(evidence).join("managed-window-pixels");
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join("child.stdout"), &stdout).unwrap();
        std::fs::write(evidence.join("child.stderr"), &stderr).unwrap();
    }
    assert!(status.success(), "{status}\n{stdout}\n{stderr}");
    assert!(stdout.contains("actual Windows surface native CPU pixels=215040"));
    assert!(stdout.contains("actual source reassignment CPU pixels=23040"));
    assert!(stdout.contains("managed display capture CPU pixels=53760"));
}
