use super::*;
use motion_core::{Content, Layer, Project, VideoAsset, VideoClip};

fn fixture(video: bool) -> Project {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    let mut layer = Layer::solid(7, "Sampling study", [32.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Vector {
        vector: motion_core::vector::VectorContent::shape(motion_core::vector::ShapeKind::Ellipse),
    };
    if video {
        project.video_assets.push(VideoAsset {
            id: 1,
            path: "assets/source.mp4".into(),
            bytes: 3,
            mime: "video/avc".into(),
            track: 0,
            width: 2,
            height: 4,
            rotation: 0,
            display_width: 2,
            display_height: 4,
            video_start_us: 0,
            video_end_us: 100_000,
            duration_us: 100_000,
            frame_count: 1,
            variable_frame_rate: false,
            nominal_frame_rate: 10.,
            color_standard: 1,
            color_range: 2,
            audio_asset: None,
        });
        layer.content = Content::Video {
            video: VideoClip::new(1),
        };
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 0,
            out_frame: 3,
            offset_frame: 0,
        });
    }
    project.layers.push(layer);
    project.validate().unwrap();
    project
}
fn session(directory: &std::path::Path, video: bool) -> motion_host::Session {
    let root = directory.join("project");
    prepare_files(&root, video);
    motion_host::Session::new(fixture(video), root, motion_desktop_media::platform()).unwrap()
}
fn prepare_files(root: &std::path::Path, video: bool) {
    std::fs::create_dir_all(root).unwrap();
    if video {
        // Opaque source bytes suffice for storage/UI contracts; never decode this fixture.
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("assets/source.mp4"), b"123").unwrap();
        let asset = fixture(true).video_assets.remove(0);
        let cache = motion_media::video_cache_path(root, &asset).unwrap();
        std::fs::create_dir_all(cache.parent().unwrap()).unwrap();
        motion_media::save_index(&cache, &asset, &[0]).unwrap();
    }
}
fn press(point: [f32; 2]) -> Input {
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn key(key: motion_ui::Key) -> Input {
    Input {
        keys: vec![(key, Default::default())],
        ..Default::default()
    }
}
fn columns(width: f32, panels: &Panels, state: &Value) -> ColumnRects {
    ColumnRects::with_output_switches(
        &PaintList::default().styles(),
        0.,
        640f32.min(width * 0.52),
        panels.timeline.column_visibility,
        panels.timeline.sampling_visibility,
        available(state),
        super::super::playback_switches::available(state),
    )
}
fn draw(panels: &mut Panels, state: &Value, input: &Input, locale: Locale) -> Vec<Action> {
    panels.locale = locale;
    let mut list = PaintList::default();
    let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
    let mut out = vec![];
    crate::panels::timeline::render(
        &mut list,
        &mut atlas,
        Rect::new(0., 0., 1200., 600.),
        panels,
        state,
        input,
        &mut out,
    );
    out
}
fn point(panels: &Panels, state: &Value, index: usize) -> [f32; 2] {
    let slot = columns(1200., panels, state).sampling[index].unwrap();
    [
        slot.x + slot.width * 0.5,
        crate::panels::timeline::header_height(&PaintList::default().styles()) + PaintList::default().styles().metric("row.height", 22.) * 0.5,
    ]
}
fn apply(session: &mut motion_host::Session, actions: Vec<Action>) {
    assert_eq!(actions.len(), 1);
    for action in actions {
        let Action::EditChecked { command, revision } = action else {
            panic!("unexpected action: {action:?}")
        };
        assert_eq!(revision, session.engine.revision());
        session.engine.apply(command).unwrap();
        session.sample().unwrap();
    }
}

#[test]
fn sampling_controls_apply_real_model_commands_and_roundtrip_history_in_both_languages() {
    for locale in [Locale::Zh, Locale::En] {
        for video in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut session = session(directory.path(), video);
            let before = session.engine.project().clone();
            let mut panels = Panels::default();
            panels.focused = Some(Panel::Timeline);
            panels.select_only(Some(7));
            for index in if video { [0, 1] } else { [1, 2] } {
                let state = session.snapshot().unwrap();
                let click = press(point(&panels, &state, index));
                apply(&mut session, draw(&mut panels, &state, &click, locale));
            }
            let sampling = session.engine.project().layers[0].sampling;
            assert_eq!(
                sampling.frame_blend,
                if video {
                    FrameBlend::Linear
                } else {
                    FrameBlend::Hold
                }
            );
            assert_eq!(sampling.motion_blur, Some(Shutter::default()));
            assert_eq!(sampling.continuous_rasterization, !video);
            assert!(panels.timeline.reorder.is_none());
            let edited = session.engine.project().clone();
            motion_core::storage::save(&session.root, &edited).unwrap();
            assert_eq!(motion_core::storage::load(&session.root).unwrap(), edited);
            session.engine.undo().unwrap();
            session.engine.undo().unwrap();
            assert_eq!(session.engine.project(), &before);
            session.engine.redo().unwrap();
            session.engine.redo().unwrap();
            assert_eq!(session.engine.project(), &edited);
        }
    }
}
#[test]
fn sampling_switches_ignore_readonly_unsupported_and_release_only() {
    let directory = tempfile::tempdir().unwrap();
    let session = session(directory.path(), false);
    let original = session.snapshot().unwrap();
    for locale in [Locale::Zh, Locale::En] {
        for reason in 0..4 {
            let mut state = original.clone();
            let mut panels = Panels::default();
            panels.focused = Some(Panel::Timeline);
            panels.select_only(Some(7));
            let mut index = 1;
            match reason {
                0 => state["project"]["layers"][0]["locked"] = true.into(),
                1 => state["observing"] = true.into(),
                2 => index = 0,
                _ => state["capabilities"]["layer_sampling"]["wgpu_preview"] = false.into(),
            };
            let click = press(point(&panels, &original, index));
            assert!(
                draw(&mut panels, &state, &click, locale).is_empty(),
                "reason={reason}"
            );
            let at = point(&panels, &original, 1);
            for input in [
                Input {
                    mouse: at,
                    released: Some((MouseButton::Left, at)),
                    ..Default::default()
                },
                Input {
                    focus_lost: true,
                    ..Default::default()
                },
                Input {
                    mouse: at,
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
            ] {
                assert!(draw(&mut panels, &original, &input, locale).is_empty());
            }
        }
    }
}
#[test]
fn sampling_column_geometry_and_preferences_preserve_other_columns_when_hidden() {
    let styles = PaintList::default().styles();
    for width in [180., 240., 320., 468., 532., 640.] {
        for flags in 0u8..8 {
            let shown = Visibility([flags & 1 != 0, flags & 2 != 0, flags & 4 != 0]);
            let columns =
                ColumnRects::with_sampling(&styles, 0., width, Default::default(), shown, true);
            let mut end = columns.name.max_x();
            for slot in columns.sampling.iter().flatten() {
                assert!(slot.x >= end && slot.width >= styles.metric("icon.slot", 20.));
                end = slot.max_x();
            }
            if let Some(slot) = columns.three_d {
                assert!(slot.x >= end);
            }
            if flags == 0 {
                assert_eq!(
                    columns,
                    ColumnRects::styled(&styles, 0., width, Default::default())
                );
            }
        }
    }
    let mut json = serde_json::to_value(crate::workspace::Workspace::default()).unwrap();
    json.as_object_mut()
        .unwrap()
        .remove("timeline_sampling_columns");
    let mut workspace: crate::workspace::Workspace = serde_json::from_value(json).unwrap();
    assert_eq!(workspace.timeline_sampling_columns, Visibility::default());
    workspace.timeline_sampling_columns = Visibility([false, true, false]);
    let restored: crate::workspace::Workspace =
        serde_json::from_slice(&serde_json::to_vec(&workspace).unwrap()).unwrap();
    assert_eq!(
        restored.timeline_sampling_columns,
        workspace.timeline_sampling_columns
    );
}

#[test]
fn single_time_routes_reject_blur_before_mutating_session_or_output_and_keep_picking() {
    use motion_host::{
        ops::{composition, export, geometry},
        session,
    };
    let directory = tempfile::tempdir().unwrap();
    let mut project = fixture(false);
    project.layers[0].sampling.motion_blur = Some(Shutter::default());
    std::fs::create_dir_all(directory.path().join("project")).unwrap();
    let id = session::open(
        directory.path().join("project"),
        project,
        motion_desktop_media::platform(),
    )
    .unwrap();
    session::with_session(id, |s| {
        s.frame = 1.25;
        s.sample()?;
        s.observing = true;
        Ok(())
    })
    .unwrap();
    let mut bytes = vec![0xa5; 32768];
    let mut vertices = vec![0x5a; 32768];
    for (operation, result) in [
        ("render_plan_info", export::render_plan_info(id).map(|_| ())),
        (
            "sample_render_plan",
            export::sample_render_plan_into(id, 9, &mut bytes).map(|_| ()),
        ),
        (
            "sample_planes",
            export::sample_into(id, 9, &mut bytes).map(|_| ()),
        ),
        (
            "sample_geometry",
            geometry::sample_geometry_into(id, 9., &mut bytes, &mut vertices).map(|_| ()),
        ),
        (
            "frame_bundle",
            composition::frame_bundle(id, motion_core::MAIN_COMPOSITION, 9., &mut bytes)
                .map(|_| ()),
        ),
        ("legacy_capture", export::capture(id).map(|_| ())),
    ] {
        let error = result.unwrap_err();
        assert!(
            error.to_string().contains(&format!("operation={operation}"))
                && error.to_string().contains("motion blur requires the exposure renderer"),
            "{error}"
        );
    }
    assert_eq!(bytes, vec![0xa5; 32768]);
    assert_eq!(vertices, vec![0x5a; 32768]);
    session::with_session(id, |s| {
        assert_eq!(s.frame, 1.25);
        assert_eq!(s.scene.frame, 1.25);
        assert!(s.observing);
        assert!(!s.root.join("exports").exists());
        Ok(())
    })
    .unwrap();
    assert!(geometry::hit_candidates(id, 32., 32.).is_ok());
    session::with_session(id, |s| {
        s.engine
            .apply(Command::SetLayerSampling {
                object: 7,
                sampling: LayerSampling {
                    continuous_rasterization: true,
                    ..Default::default()
                },
            })
            .unwrap();
        Ok(())
    })
    .unwrap();
    // Continuous vector rasterization is already representable by the plan builder.
    assert!(export::render_plan_info(id).is_ok());
    assert!(
        composition::frame_bundle(id, motion_core::MAIN_COMPOSITION, 2., &mut bytes).unwrap() > 0
    );
    session::close(id);
}

#[test]
fn declared_sampling_sources_agree_with_model_and_unreferenced_blur_is_not_a_boundary() {
    let caps = motion_host::render_sampling::capabilities();
    let contents = [
        Content::Solid { color: [1.; 4] },
        Content::Image { asset: 1 },
        Content::Text {
            text: "a".into(),
            font: "system".into(),
            style: motion_core::TextStyle::new(32., 256),
            color: [1.; 4],
            raster_asset: 1,
        },
        fixture(false).layers.remove(0).content,
        Content::Video {
            video: VideoClip::new(1),
        },
        Content::Composition {
            clip: motion_core::CompositionClip {
                composition: "comp-child".into(),
                source_start_frame: 0,
                volume: 1.,
                muted: false,
            },
        },
        Content::Audio {
            audio: motion_core::AudioClip::new(1),
        },
        Content::Null,
        Content::Adjustment,
        Content::Camera {
            camera: Box::new(motion_core::Camera::new(64, 64)),
        },
    ];
    for content in contents {
        let kind = serde_json::to_value(&content).unwrap()["kind"].clone();
        for (property, sampling) in [
            (
                "frame_blend",
                LayerSampling {
                    frame_blend: FrameBlend::Linear,
                    ..Default::default()
                },
            ),
            (
                "motion_blur",
                LayerSampling {
                    motion_blur: Some(Shutter::default()),
                    ..Default::default()
                },
            ),
            (
                "continuous_rasterization",
                LayerSampling {
                    continuous_rasterization: true,
                    ..Default::default()
                },
            ),
        ] {
            assert_eq!(
                caps[property]["sources"]
                    .as_array()
                    .unwrap()
                    .contains(&kind),
                sampling.validate_content(&content).is_ok(),
                "{kind} {property}"
            );
        }
    }
    let mut project = fixture(false);
    let mut child = project.layers[0].clone();
    child.id = 8;
    child.sampling.motion_blur = Some(Shutter::default());
    project.compositions.push(motion_core::Composition {
        id: "comp-child".into(),
        name: "Child".into(),
        width: 64,
        height: 64,
        fps: 60,
        frames: 120,
        work_area: motion_core::WorkArea::full(120),
        background: [0.; 4],
        camera: project.camera.clone(),
        layers: vec![child],
        expressions: vec![],
    });
    project.validate().unwrap();
    assert!(motion_host::render_sampling::require_single_time(&project, "probe").is_ok());
    project.layers[0].content = Content::Composition {
        clip: motion_core::CompositionClip {
            composition: "comp-child".into(),
            source_start_frame: 0,
            volume: 1.,
            muted: false,
        },
    };
    project.validate().unwrap();
    assert!(
        motion_host::render_sampling::require_single_time(&project, "probe")
            .unwrap_err()
            .to_string()
            .contains("motion blur requires")
    );
}

#[test]
fn malformed_sampling_and_rejected_edits_reach_default_file_and_error_stderr_only() {
    const CHILD: &str = "MOTION_SAMPLING_FAILURE_CHILD";
    const CAUSE: &str = "shutter sample count must be within 2..=16";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let project_root = root.join("project");
        prepare_files(&project_root, false);
        motion_core::storage::save(&project_root, &fixture(false)).unwrap();
        let engine =
            crate::engine::Engine::start(project_root, motion_desktop_media::platform(), 8 << 30)
                .unwrap();
        let before = engine.state().unwrap();
        let command = Command::SetLayerSampling {
            object: 7,
            sampling: LayerSampling {
                motion_blur: Some(Shutter {
                    samples: 1,
                    ..Default::default()
                }),
                ..Default::default()
            },
        };
        assert!(engine
            .edit_at_revision(
                serde_json::to_string(&command).unwrap(),
                before["revision"].as_u64().unwrap()
            )
            .unwrap_err()
            .contains(CAUSE));
        let after = engine.state().unwrap();
        assert_eq!(before["project"], after["project"]);
        assert_eq!(before["revision"], after["revision"]);
        let mut bad = before.clone();
        bad["project"]["layers"][0]["sampling"] =
            serde_json::json!({"motion_blur":{"frames":0.5,"samples":"broken"}});
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for _ in 0..120 {
            for state in [&bad, &before] {
                let mut list = PaintList::default();
                let mut actions = vec![];
                let columns = ColumnRects::with_sampling(
                    &list.styles(),
                    0.,
                    640.,
                    Default::default(),
                    Visibility::default(),
                    true,
                );
                row(
                    &mut list,
                    &mut atlas,
                    &columns,
                    &state["project"]["layers"][0],
                    state,
                    60.,
                    22.,
                    &Input::default(),
                    &mut actions,
                );
                assert!(actions.is_empty());
            }
        }
        assert!(!engine.failed());
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::layer_sampling::tests::malformed_sampling_and_rejected_edits_reach_default_file_and_error_stderr_only"])
        .env(CHILD,directory.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let file = std::fs::read_to_string(directory.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        file.contains("logging initialized level=INFO")
            && file.contains("start #")
            && file.contains("edit_checked")
    );
    assert!(file.contains(CAUSE) && stderr.contains(CAUSE) && !stdout.contains(CAUSE));
    assert_eq!(file.matches("site=sampling.settings count=1 ").count(), 1);
    assert_eq!(file.matches("site=sampling.settings count=60 ").count(), 1);
    assert!(file.contains("object=7 revision=") && file.contains("invalid type"));
    assert_eq!(stderr.matches("site=sampling.settings").count(), 1);
    assert!(!stdout.contains("site=sampling.settings"));
}

#[cfg(target_os = "windows")]
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn sampling_switches_use_native_main_and_floating_routes_with_bilingual_screens() {
    const CHILD: &str = "MOTION_TIMELINE_SAMPLING_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","panels::layer_sampling::tests::sampling_switches_use_native_main_and_floating_routes_with_bilingual_screens"])
            .env(CHILD,"1").output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use std::{sync::Arc, time::Duration};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let event_loop = winit::event_loop::EventLoop::builder()
        .with_any_thread(true)
        .build()
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let screenshots = std::env::var_os("MOTION_SAMPLING_UI_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| directory.path().join("screens"));
    std::fs::create_dir_all(&screenshots).unwrap();
    for (base_language, locale) in [("zh", Locale::Zh), ("en", Locale::En)] {
        for ratio in [1f32, 1.25, 1.5, 2.] {
            let language = if ratio == 1. {
                base_language.to_owned()
            } else {
                format!("{base_language}-dpi{}", (ratio * 100.) as u32)
            };
            let root = directory.path().join(&language);
            prepare_files(&root, true);
            let mut project = fixture(true);
            project.layers[0].name = if locale == Locale::Zh {
                "视频图层"
            } else {
                "Video layer"
            }
            .into();
            let mut vector = fixture(false).layers.remove(0);
            vector.id = 8;
            vector.name = if locale == Locale::Zh {
                "矢量图层"
            } else {
                "Vector layer"
            }
            .into();
            project.layers.push(vector);
            motion_core::storage::save(&root, &project).unwrap();
            let engine =
                crate::engine::Engine::start(root, motion_desktop_media::platform(), 8 << 30)
                    .unwrap();
            #[allow(deprecated)]
            let window = Arc::new(
                event_loop
                    .create_window(
                        winit::window::Window::default_attributes()
                            .with_visible(false)
                            .with_inner_size(winit::dpi::PhysicalSize::new(
                                (1600. * ratio) as u32,
                                (1000. * ratio) as u32,
                            )),
                    )
                    .unwrap(),
            );
            engine
                .attach_async(window, (1600. * ratio) as u32, (1000. * ratio) as u32)
                .unwrap()
                .recv_timeout(Duration::from_secs(30))
                .unwrap()
                .unwrap();
            for floating in [false, true] {
                let size = if floating {
                    [900., 600.]
                } else {
                    [1600., 1000.]
                };
                if floating {
                    #[allow(deprecated)]
                    let window = Arc::new(
                        event_loop
                            .create_window(
                                winit::window::Window::default_attributes()
                                    .with_visible(false)
                                    .with_inner_size(winit::dpi::PhysicalSize::new(
                                        (900. * ratio) as u32,
                                        (600. * ratio) as u32,
                                    )),
                            )
                            .unwrap(),
                    );
                    engine
                        .float_window(1, window, (900. * ratio) as u32, (600. * ratio) as u32)
                        .unwrap();
                }
                let mut state = engine.state().unwrap();
                let before = state["project"].clone();
                let mut panels = Panels::default();
                panels.locale = locale;
                panels.focused = Some(Panel::Timeline);
                panels.select_only(Some(7));
                let sheet = PaintList::default().styles();
                let mut atlas = TextAtlas::from_family(
                    sheet.font_family(),
                    sheet.base_font_size(),
                    locale == Locale::Zh,
                )
                .unwrap();
                atlas.set_pixel_ratio(ratio);
                let layout = dock::editor_layout();
                let body = if floating {
                    Rect::new(0., 30., size[0], size[1] - 52.)
                } else {
                    crate::panels::frames(&layout, size)
                        .into_iter()
                        .find(|f| f.panel == Panel::Timeline)
                        .unwrap()
                        .content
                };
                let render =
                    |panels: &mut Panels, atlas: &mut TextAtlas, state: &Value, input: &Input| {
                        let mut paint = PaintList::default();
                        let actions = if floating {
                            crate::panels::floating_chrome(
                                &mut paint,
                                atlas,
                                Panel::Timeline,
                                size,
                                panels,
                                state,
                                input,
                            )
                        } else {
                            crate::panels::chrome(
                                &mut paint, atlas, size, &layout, panels, state, input,
                            )
                        };
                        (paint, actions)
                    };
                let capture = |paint: PaintList, atlas: &TextAtlas, suffix: &str| {
                    let config = atlas.config();
                    let path = screenshots.join(format!(
                        "{language}-timeline-{}-{suffix}.png",
                        if floating { "floating" } else { "main" }
                    ));
                    let report = engine
                        .capture_ui(
                            crate::engine::UiFrame {
                                paint,
                                atlas: Some(crate::engine::AtlasUpload {
                                    pixels: atlas.pixel_data().to_vec(),
                                    width: config.width,
                                    height: config.height,
                                }),
                                logical_size: size,
                                composition: None,
                                composition_image: None,
                                window: u64::from(floating),
                                node_preview: None,
                            },
                            path.clone(),
                        )
                        .unwrap();
                    let physical = [(size[0] * ratio) as u32, (size[1] * ratio) as u32];
                    assert_eq!(report["width"], physical[0]);
                    assert_eq!(report["height"], physical[1]);
                    let pixels = image::open(path).unwrap().into_rgba8();
                    assert_eq!(pixels.dimensions(), (physical[0], physical[1]));
                    pixels
                };
                let initial_pixels = capture(
                    render(&mut panels, &mut atlas, &state, &Input::default()).0,
                    &atlas,
                    "disabled",
                );
                let mut clicked_cells = Vec::new();
                for (object, index) in [(7, 0), (7, 1), (8, 1), (8, 2)] {
                    let (paint, actions) =
                        render(&mut panels, &mut atlas, &state, &Input::default());
                    assert!(actions.is_empty());
                    let columns = ColumnRects::with_output_switches(
                        &paint.styles(),
                        body.min[0],
                        640f32.min(body.width() * 0.52),
                        panels.timeline.column_visibility,
                        panels.timeline.sampling_visibility,
                        true,
                        super::super::playback_switches::available(&state),
                    );
                    let slot = columns.sampling[index].unwrap();
                    let rows = crate::panels::timeline::rows(&state);
                    let position = rows.iter().position(|r| r.id == object).unwrap();
                    let at = [
                        slot.x + slot.width * 0.5,
                        body.min[1]
                            + crate::panels::timeline::header_height(&sheet)
                            + sheet.metric("row.height", 22.) * (position as f32 + 0.5),
                    ];
                    clicked_cells.push(slot.rect(
                        body.min[1] + crate::panels::timeline::header_height(&sheet) + sheet.metric("row.height", 22.) * position as f32,
                        sheet.metric("row.height", 22.),
                    ));
                    let (_, actions) = render(&mut panels, &mut atlas, &state, &press(at));
                    assert_eq!(
                        actions.len(),
                        1,
                        "{language} floating={floating} object={object} index={index}"
                    );
                    for action in actions {
                        let Action::EditChecked { command, revision } = action else {
                            panic!("unexpected action: {action:?}")
                        };
                        state = engine
                            .edit_at_revision(serde_json::to_string(&command).unwrap(), revision)
                            .unwrap();
                    }
                    assert!(render(
                        &mut panels,
                        &mut atlas,
                        &state,
                        &Input {
                            mouse: at,
                            released: Some((MouseButton::Left, at)),
                            ..Default::default()
                        }
                    )
                    .1
                    .is_empty());
                }
                for (object, index) in [(7, 0), (7, 1), (8, 1), (8, 2)] {
                    let layer = state["project"]["layers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|l| l["id"] == object)
                        .unwrap();
                    let setting: LayerSampling =
                        serde_json::from_value(layer["sampling"].clone()).unwrap();
                    assert!(match index {
                        0 => setting.frame_blend == FrameBlend::Linear,
                        1 => setting.motion_blur.is_some(),
                        _ => setting.continuous_rasterization,
                    });
                }
                let (paint, actions) = render(&mut panels, &mut atlas, &state, &Input::default());
                assert!(actions.is_empty());
                let enabled_pixels = capture(paint, &atlas, "enabled");
                for cell in clicked_cells {
                    let mut changed = 0;
                    for y in
                        (cell.min[1] * ratio).ceil() as u32..(cell.max[1] * ratio).floor() as u32
                    {
                        for x in (cell.min[0] * ratio).ceil() as u32
                            ..(cell.max[0] * ratio).floor() as u32
                        {
                            changed += usize::from(
                                initial_pixels.get_pixel(x, y) != enabled_pixels.get_pixel(x, y),
                            );
                        }
                    }
                    assert!(changed > 0, "native checkbox must visibly update: {language} floating={floating} cell={cell:?}");
                }
                if !floating {
                    panels.menu = Some(6);
                    let (paint, actions) =
                        render(&mut panels, &mut atlas, &state, &Input::default());
                    assert!(actions.is_empty());
                    capture(paint, &atlas, "columns-menu");
                    assert!(render(&mut panels, &mut atlas, &state, &key(Key::Escape))
                        .1
                        .is_empty());
                    assert!(panels.menu.is_none());
                    panels.menu = Some(6);
                    let x = 8.
                        + [
                            T::File,
                            T::Edit,
                            T::Composition,
                            T::Layer,
                            T::Effect,
                            T::Animation,
                        ]
                        .into_iter()
                        .map(|name| atlas.measure(locale.t(name)) + 22.)
                        .sum::<f32>()
                        + 20.;
                    let (_, actions) = render(
                        &mut panels,
                        &mut atlas,
                        &state,
                        &press([
                            x,
                            metrics::TITLE_BAR + metrics::MENU_BAR + 4. + 5. * 32. + 15.,
                        ]),
                    );
                    assert_eq!(actions.len(), 1);
                    let Action::SamplingColumnVisibility(next) = actions[0] else {
                        panic!("wrong menu command")
                    };
                    assert_eq!(next, Visibility([true, false, true]));
                    panels.timeline.sampling_visibility = next;
                    let (paint, actions) =
                        render(&mut panels, &mut atlas, &state, &Input::default());
                    assert!(actions.is_empty());
                    capture(paint, &atlas, "blur-column-hidden");
                    assert_eq!(
                        engine.state().unwrap()["project"],
                        state["project"],
                        "column visibility must not edit the project"
                    );
                }
                for _ in 0..4 {
                    state = engine
                        .history(motion_host::ops::editing::HISTORY_UNDO)
                        .unwrap();
                }
                assert_eq!(state["project"], before);
            }
        }
    }
}
