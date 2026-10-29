use super::*;
use crate::viewer_navigation::Navigation;

fn state(guides: Vec<Guide>) -> Value {
    serde_json::json!({"root":"viewer-guide-fixture", "document_generation":4, "revision":7,
        "composition":"comp-main", "project":{"composition_id":"comp-main", "width":640, "height":480, "guides":guides}})
}
pub(super) fn draw(
    controls: &mut Controls,
    state: &Value,
    input: &Input,
    window: Option<Panel>,
    navigation: Navigation,
    locale: Locale,
    atlas: &mut TextAtlas,
) -> (bool, Vec<Action>) {
    let viewport = Rect::new(30., 70., 800., 600.);
    let mapping = navigation.mapping(viewport, [640., 480.]).unwrap();
    let mut out = vec![];
    let owned = controls.prepare(window, state, input);
    let neutral = Input::default();
    let consumed = controls.viewport(
        &mut PaintList::default(),
        atlas,
        viewport,
        mapping,
        [640., 480.],
        state,
        if owned { &neutral } else { input },
        locale,
        true,
        &mut out,
    );
    (owned || consumed, out)
}
pub(super) fn press(point: [f32; 2]) -> Input {
    Input {
        pressed: Some((MouseButton::Left, point)),
        mouse: point,
        held: Some(MouseButton::Left),
        ..Default::default()
    }
}
pub(super) fn release(point: [f32; 2]) -> Input {
    // The coalesced release position is authoritative, not an older mouse sample.
    Input {
        released: Some((MouseButton::Left, point)),
        mouse: [-100., -100.],
        ..Default::default()
    }
}
fn result(actions: Vec<Action>) -> Vec<Guide> {
    assert_eq!(actions.len(), 1);
    let Action::EditorAction { commands, .. } = actions.into_iter().next().unwrap() else {
        panic!("guide edit must retain the worker source guard")
    };
    assert_eq!(commands.len(), 1);
    let Command::InComposition {
        composition,
        command,
    } = commands.into_iter().next().unwrap()
    else {
        panic!("guide edit must be explicitly scoped")
    };
    assert_eq!(composition, "comp-main");
    let Command::SetGuides { guides } = *command else {
        panic!("unexpected edit")
    };
    guides
}

#[test]
fn ruler_creation_uses_composition_pixels_in_both_locales_windows_and_zoom_levels() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let viewport = Rect::new(30., 70., 800., 600.);
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Composition)] {
            for zoom in [None, Some(0.25), Some(1.), Some(4.)] {
                let mut navigation = Navigation::default();
                if let Some(zoom) = zoom {
                    assert!(navigation.zoom_at(viewport, [640., 480.], [430., 370.], zoom));
                }
                assert!(navigation.pan(viewport, [640., 480.], [13., -17.]));
                let mapping = navigation.mapping(viewport, [640., 480.]).unwrap();
                let state = state(vec![]);
                let before = state.clone();
                let mut controls = Controls::default();
                controls.visibility.rulers = true;
                let (owned, actions) = draw(
                    &mut controls,
                    &state,
                    &press([38., 370.]),
                    window,
                    navigation,
                    locale,
                    &mut atlas,
                );
                assert!(owned && actions.is_empty() && controls.dragging());
                let point = mapping.screen([333.25, 240.]);
                let (_, actions) = draw(
                    &mut controls,
                    &state,
                    &Input {
                        mouse: point,
                        held: Some(MouseButton::Left),
                        modifiers: motion_ui::input::Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    window,
                    navigation,
                    locale,
                    &mut atlas,
                );
                assert!(actions.is_empty(), "preview must not write history");
                let (_, actions) = draw(
                    &mut controls,
                    &state,
                    &Input {
                        modifiers: motion_ui::input::Modifiers {
                            alt: true,
                            ..Default::default()
                        },
                        ..release(point)
                    },
                    window,
                    navigation,
                    locale,
                    &mut atlas,
                );
                let guides = result(actions);
                assert_eq!(guides.len(), 1);
                assert_eq!(guides[0].axis, GuideAxis::X);
                assert!((guides[0].position - 333.25).abs() < 0.001);
                assert!(!controls.dragging());
                assert_eq!(state, before);
            }
        }
    }
}

#[test]
fn guide_drag_cancellation_stale_context_and_foreign_input_never_commit() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let navigation = Navigation::default();
    let viewport = Rect::new(30., 70., 800., 600.);
    let mapping = navigation.mapping(viewport, [640., 480.]).unwrap();
    let original = state(vec![Guide {
        id: 1,
        axis: GuideAxis::X,
        position: 200.,
    }]);
    let point = mapping.screen([200., 240.]);
    for reason in 0..10 {
        let mut controls = Controls::default();
        let mut state = original.clone();
        assert!(
            draw(
                &mut controls,
                &state,
                &press(point),
                None,
                navigation,
                Locale::En,
                &mut atlas
            )
            .0
        );
        assert!(controls.dragging());
        let mut nav = navigation;
        let mut input = release(mapping.screen([230., 240.]));
        match reason {
            0 => {
                input = Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                }
            }
            1 => {
                input = Input {
                    focus_lost: true,
                    ..Default::default()
                }
            }
            2 => state["root"] = "another-document".into(),
            3 => state["document_generation"] = 5.into(),
            4 => state["composition"] = "child".into(),
            5 => state["revision"] = 8.into(),
            6 => {
                assert!(nav.pan(viewport, [640., 480.], [1., 0.]));
            }
            7 => input = release([0., 0.]),
            8 => {
                assert!(controls.prepare(Some(Panel::Properties), &state, &press([20., 20.])));
            }
            9 => {
                assert!(!controls.prepare(Some(Panel::Properties), &state, &Input::default()));
                assert!(controls.dragging());
                input = Input {
                    focus_lost: true,
                    ..Default::default()
                };
            }
            _ => unreachable!(),
        }
        let (_, actions) = draw(
            &mut controls,
            &state,
            &input,
            None,
            nav,
            Locale::En,
            &mut atlas,
        );
        assert!(actions.is_empty(), "reason={reason}");
        assert!(!controls.dragging(), "reason={reason}");
    }
}

#[test]
fn guide_move_delete_and_capacity_are_single_bounded_edits() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let nav = Navigation::default();
    let mapping = nav
        .mapping(Rect::new(30., 70., 800., 600.), [640., 480.])
        .unwrap();
    let original = state(vec![Guide {
        id: u64::MAX,
        axis: GuideAxis::X,
        position: 200.,
    }]);
    let mut controls = Controls::default();
    controls.visibility.rulers = true;
    draw(
        &mut controls,
        &original,
        &press(mapping.screen([200., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    let (_, actions) = draw(
        &mut controls,
        &original,
        &release(mapping.screen([234., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    let moved = result(actions);
    assert_eq!(moved[0].id, u64::MAX);
    assert_eq!(moved[0].position, 234.);
    draw(
        &mut controls,
        &original,
        &press(mapping.screen([200., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    let (_, actions) = draw(
        &mut controls,
        &original,
        &release([38., 370.]),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    assert!(result(actions).is_empty());
    assert_eq!(
        next_id(&[Guide {
            id: u64::MAX,
            axis: GuideAxis::X,
            position: 0.
        }]),
        Some(1)
    );
    let full = state(
        (1..=MAX_GUIDES as u64)
            .map(|id| Guide {
                id,
                axis: GuideAxis::X,
                position: id as f64,
            })
            .collect(),
    );
    let (owned, actions) = draw(
        &mut controls,
        &full,
        &press([38., 370.]),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    assert!(owned && actions.is_empty() && !controls.dragging());
}

#[test]
fn grid_and_snapping_share_the_origin_and_screen_tolerance_after_navigation() {
    let viewport = Rect::new(30., 70., 800., 600.);
    for zoom in [0.01, 0.25, 1., 64.] {
        let mut nav = Navigation::default();
        assert!(nav.zoom_at(viewport, [640., 480.], [430., 370.], zoom));
        assert!(nav.pan(viewport, [640., 480.], [43., -27.]));
        let mapping = nav.mapping(viewport, [640., 480.]).unwrap();
        let step = grid_step(mapping, 100., 24.).unwrap();
        assert!(step * mapping.scale >= 24.);
        for axis in 0..2 {
            let ticks = ticks(mapping, viewport, axis, step);
            assert!(ticks.len() <= MAX_GRID_LINES);
            for value in ticks {
                assert!((value / step - (value / step).round()).abs() < 1e-9);
            }
        }
        // Isolate this guide from composition edges even at one-percent zoom.
        let guide = Guide {
            id: 3,
            axis: GuideAxis::X,
            position: -10_000.,
        };
        let target = guide.position + 5. / zoom;
        assert_eq!(
            snap_axis(
                target,
                GuideAxis::X,
                std::slice::from_ref(&guide),
                [640., 480.],
                mapping,
                6.,
                None,
                None
            ),
            guide.position
        );
        let outside = guide.position + 7. / zoom;
        assert_ne!(
            snap_axis(
                outside,
                GuideAxis::X,
                std::slice::from_ref(&guide),
                [640., 480.],
                mapping,
                6.,
                None,
                None
            ),
            guide.position
        );
        let point = mapping.screen([333.25, 240.]);
        let back = mapping.local(point);
        assert!((back[0] - 333.25).abs() < 0.01);
    }
    let mapping = Navigation::default()
        .mapping(viewport, [640., 480.])
        .unwrap();
    for invalid in [0., -1., f64::NAN, f64::INFINITY] {
        assert!(grid_step(mapping, invalid, 24.).is_none());
    }
    assert!(ticks(mapping, Rect::default(), 0, 100.).is_empty());
}

#[test]
fn passive_paints_preserve_preview_and_same_revision_guide_replacement_cancels_it() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let nav = Navigation::default();
    let mapping = nav
        .mapping(Rect::new(30., 70., 800., 600.), [640., 480.])
        .unwrap();
    let mut state = state(vec![Guide {
        id: 1,
        axis: GuideAxis::X,
        position: 200.,
    }]);
    let mut controls = Controls::default();
    draw(
        &mut controls,
        &state,
        &press(mapping.screen([200., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    draw(
        &mut controls,
        &state,
        &Input {
            mouse: mapping.screen([234., 240.]),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    assert_eq!(controls.drag.as_ref().unwrap().guide.position, 234.);
    for window in [None, Some(Panel::Properties), None] {
        assert!(draw(
            &mut controls,
            &state,
            &Input::default(),
            window,
            nav,
            Locale::En,
            &mut atlas
        )
        .1
        .is_empty());
        assert_eq!(controls.drag.as_ref().unwrap().guide.position, 234.);
    }
    state["project"]["guides"][0]["position"] = 250.into();
    assert!(draw(
        &mut controls,
        &state,
        &release(mapping.screen([270., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas
    )
    .1
    .is_empty());
    assert!(!controls.dragging());
}

#[test]
fn second_press_cancels_lost_release_without_starting_another_edit() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let nav = Navigation::default();
    let mapping = nav
        .mapping(Rect::new(30., 70., 800., 600.), [640., 480.])
        .unwrap();
    let state = state(vec![Guide {
        id: 1,
        axis: GuideAxis::X,
        position: 200.,
    }]);
    let mut controls = Controls::default();
    draw(
        &mut controls,
        &state,
        &press(mapping.screen([200., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    let (owned, actions) = draw(
        &mut controls,
        &state,
        &press(mapping.screen([240., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    assert!(owned && actions.is_empty() && !controls.dragging());
    assert!(draw(
        &mut controls,
        &state,
        &release(mapping.screen([250., 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas
    )
    .1
    .is_empty());
}

#[test]
fn ruler_creation_snaps_to_the_composition_center_within_six_screen_pixels() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let viewport = Rect::new(30., 70., 800., 600.);
    let mut nav = Navigation::default();
    assert!(nav.zoom_at(viewport, [640., 480.], [430., 370.], 0.25));
    let mapping = nav.mapping(viewport, [640., 480.]).unwrap();
    let state = state(vec![]);
    let mut controls = Controls::default();
    controls.visibility.rulers = true;
    draw(
        &mut controls,
        &state,
        &press([38., 370.]),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    let (_, actions) = draw(
        &mut controls,
        &state,
        &release(mapping.screen([333.25, 240.])),
        None,
        nav,
        Locale::En,
        &mut atlas,
    );
    assert_eq!(result(actions)[0].position, 320.);
}

#[test]
fn overlay_menu_keeps_native_window_ownership_and_consumes_its_closing_frame() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let state = state(vec![]);
    for window in [None, Some(Panel::Composition)] {
        let mut controls = Controls::default();
        controls.prepare(window, &state, &Input::default());
        controls.open([100., 400.], &state);
        let mut out = vec![];
        controls.menu(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 900., 600.),
            &state,
            &press([120., 200.]),
            Locale::En,
            false,
            "Ctrl+Shift+H",
            &mut out,
        );
        assert!(controls.menu.is_some() && out.is_empty()); // Opening frame never toggles a row.
        let other = if window.is_none() {
            Some(Panel::Composition)
        } else {
            None
        };
        assert!(!controls.prepare(other, &state, &Input::default()));
        assert!(controls.menu.is_some());
        assert!(controls.prepare(other, &state, &press([120., 200.])));
        assert!(!controls.editing() && out.is_empty());
        controls.prepare(window, &state, &Input::default());
        controls.open([100., 400.], &state);
        controls.menu.as_mut().unwrap().opened = false;
        assert!(controls.prepare(window, &state, &press([10., 10.])));
        controls.menu(
            &mut PaintList::default(),
            &mut atlas,
            Rect::new(0., 0., 900., 600.),
            &state,
            &press([10., 10.]),
            Locale::Zh,
            false,
            "Ctrl+Shift+H",
            &mut out,
        );
        assert!(!controls.editing() && out.is_empty());
    }
}

#[test]
fn channel_dropdown_selects_explicitly_and_keeps_window_state_independent() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let state = state(vec![]);
    for locale in [Locale::Zh, Locale::En] {
        for window in [None, Some(Panel::Composition)] {
            let mut controls = Controls::default();
            let bounds = Rect::new(0., 0., 900., 600.);
            let styles = PaintList::default().styles();
            let row = styles.metric("row.height", 22.);
            let padding = styles.metric("padding.panel", 8.);
            let anchor_y = 400. - padding - row;
            controls.prepare(window, &state, &Input::default());
            controls.open([100., 400.], &state);
            let mut actions = vec![];
            let mut draw_menu = |controls: &mut Controls, input: &Input| {
                controls.prepare(window, &state, input);
                controls.menu(&mut PaintList::default(), &mut atlas, bounds, &state,
                    input, locale, false, "", &mut actions);
            };
            draw_menu(&mut controls, &Input::default());
            draw_menu(&mut controls, &press([100. + padding + 5., anchor_y + row * 0.5]));
            assert!(controls.channel_dropdown.open);
            // The five-item dropdown opens above its anchor; its last row is Alpha.
            draw_menu(&mut controls, &press([100. + padding + 5., anchor_y - row * 0.5]));
            assert_eq!(controls.channel(window.is_some()), motion_render::ViewChannel::Alpha);
            assert_eq!(controls.channel(window.is_none()), motion_render::ViewChannel::Color);
            assert!(!controls.channel_dropdown.open);
            assert!(actions.is_empty());
            assert_eq!(state["revision"], 7);
        }
    }
}

#[test]
fn handle_shortcut_respects_locale_window_repeat_ime_and_text_ownership() {
    use crate::shortcuts::{Command as Shortcut, Scope};
    let state = state(vec![]);
    let modifiers = motion_ui::input::Modifiers {
        control: true,
        shift: true,
        alt: false,
    };
    for locale in [Locale::En, Locale::Zh] {
        for window in [None, Some(Panel::Composition)] {
            let mut panels = Panels::default();
            panels.locale = locale;
            panels.focused = Some(Panel::Composition);
            assert_eq!(
                panels
                    .shortcut_bindings
                    .resolve(Scope::Viewer, Key::H, modifiers, false),
                Some(Shortcut::ToggleViewerHandles)
            );
            let mut out = vec![];
            let input = Input {
                keys: vec![(Key::H, modifiers)],
                ..Default::default()
            };
            assert!(super::super::keyboard::route(
                &input,
                &mut panels,
                &state,
                false,
                window,
                &mut out
            ));
            assert!(!panels.viewer_guides.visibility.handles && out.is_empty());
            for input in [
                Input {
                    key_repeats: vec![true],
                    ..input.clone()
                },
                Input {
                    composing: true,
                    ..input.clone()
                },
                Input {
                    ime_consumed: true,
                    ..input.clone()
                },
                Input {
                    focus_lost: true,
                    ..input.clone()
                },
            ] {
                super::super::keyboard::route(&input, &mut panels, &state, false, window, &mut out);
                assert!(!panels.viewer_guides.visibility.handles && out.is_empty());
            }
            panels.project.search = true;
            super::super::keyboard::route(&input, &mut panels, &state, false, window, &mut out);
            assert!(!panels.viewer_guides.visibility.handles && out.is_empty());
        }
    }
}

#[test]
fn malformed_guide_attempts_preserve_causes_at_default_info_without_stdout() {
    const CHILD: &str = "MOTION_VIEWER_GUIDE_FAULT_CHILD";
    const NAME:&str="panels::viewer_guides::tests::malformed_guide_attempts_preserve_causes_at_default_info_without_stdout";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        let nav = Navigation::default();
        let healthy = state(vec![]);
        let mut broken = healthy.clone();
        broken["project"]["guides"] =
            serde_json::json!([{"id":1,"axis":"PRIVATE_UNTRUSTED_PAYLOAD","position":100}]);
        let mut controls = Controls::default();
        controls.visibility.rulers = true;
        let count = || {
            crate::logging::flush();
            std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .lines()
                .filter(|line| line.contains("operation=guide_edit"))
                .count()
        };
        for _ in 0..59 {
            assert!(draw(
                &mut controls,
                &broken,
                &press([38., 370.]),
                None,
                nav,
                Locale::En,
                &mut atlas
            )
            .1
            .is_empty());
        }
        assert_eq!(count(), 0);
        for _ in 0..120 {
            draw(
                &mut controls,
                &broken,
                &Input::default(),
                None,
                nav,
                Locale::En,
                &mut atlas,
            );
        }
        assert_eq!(count(), 0);
        for _ in 0..121 {
            assert!(draw(
                &mut controls,
                &broken,
                &press([38., 370.]),
                None,
                nav,
                Locale::En,
                &mut atlas
            )
            .1
            .is_empty());
        }
        assert_eq!(count(), 1);
        draw(
            &mut controls,
            &healthy,
            &Input::default(),
            None,
            nav,
            Locale::En,
            &mut atlas,
        );
        for _ in 0..59 {
            draw(
                &mut controls,
                &broken,
                &press([38., 370.]),
                None,
                nav,
                Locale::En,
                &mut atlas,
            );
        }
        assert_eq!(count(), 1);
        draw(
            &mut controls,
            &broken,
            &press([38., 370.]),
            None,
            nav,
            Locale::En,
            &mut atlas,
        );
        assert_eq!(count(), 2);
        controls.cancel();
        for _ in 0..60 {
            draw(
                &mut controls,
                &broken,
                &press([38., 370.]),
                None,
                nav,
                Locale::En,
                &mut atlas,
            );
        }
        assert_eq!(count(), 3);
        let text = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for line in text
            .lines()
            .filter(|line| line.contains("operation=guide_edit"))
        {
            assert!(
                line.contains("ERROR")
                    && line.contains("cause=guide axis must be x or y")
                    && line.contains("count=60")
                    && line.contains("revision=Some(7)")
                    && line.contains("phase=begin")
            );
        }
        assert!(!text.contains("PRIVATE_UNTRUSTED_PAYLOAD"));
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let root = std::env::var_os("MOTION_VIEWER_GUIDE_FAULT_ARTIFACTS")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temporary.path().into());
    std::fs::create_dir_all(&root).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env_remove("MOTION_LOG")
        .env(CHILD, &root)
        .output()
        .unwrap();
    std::fs::write(root.join("stdout.log"), &output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"), &output.stderr).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.contains("operation=guide_edit"))
            .count(),
        3
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("operation=guide_edit"));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn guide_metadata_and_viewer_overlay_paint_leave_headless_scene_capture_rgba_unchanged() {
    let mut project = motion_core::Project::new(64, 64, 30, 24).unwrap();
    project.layers.push(motion_core::Layer::solid(
        1,
        "Original test shape",
        [32., 20.],
        [31., 30., 0.],
        [0.8, 0.2, 0.1, 1.],
    ));
    let mut renderer = pollster::block_on(motion_render::Renderer::headless()).unwrap();
    let mut capture = |project: &motion_core::Project| {
        let mut scene = motion_render::Scene::new(project).unwrap();
        scene
            .sample(project, 0., None, &motion_core::ExpressionEvaluator)
            .unwrap();
        let target = renderer.capture_target(64, 64).unwrap();
        renderer.capture(&scene, &target).unwrap().0
    };
    let before = capture(&project);
    assert_eq!(before.len(), 64 * 64 * 4);
    project.guides = vec![
        Guide {
            id: 1,
            axis: GuideAxis::X,
            position: 23.5,
        },
        Guide {
            id: 2,
            axis: GuideAxis::Y,
            position: 42.,
        },
    ];
    project.validate().unwrap();
    let mut controls = Controls::default();
    controls.visibility = Visibility {
        safe: true,
        grid: true,
        rulers: true,
        guides: true,
        handles: false,
        locked: false,
    };
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let mut paint = PaintList::default();
    let view = Rect::new(0., 0., 640., 640.);
    let mapping = Navigation::default().mapping(view, [64., 64.]).unwrap();
    controls.paint(
        &mut paint,
        &mut atlas,
        view,
        mapping,
        [64., 64.],
        &project.guides,
        Locale::En,
    );
    assert!(
        !paint.shapes.is_empty(),
        "overlays must actually generate viewer geometry"
    );
    let after = capture(&project);
    assert_eq!(before, after);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("guide-capture.png");
    image::save_buffer(&path, &after, 64, 64, image::ColorType::Rgba8).unwrap();
    assert_eq!(image::open(path).unwrap().to_rgba8().as_raw(), &before);
}

#[test]
fn ruler_corner_capacity_and_readonly_feedback_do_not_fall_through_or_count_normal_branches_as_faults(
) {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let nav = Navigation::default();
    for locale in [Locale::En, Locale::Zh] {
        let healthy = state(vec![]);
        let mut controls = Controls::default();
        controls.visibility.rulers = true;
        let (owned, actions) = draw(
            &mut controls,
            &healthy,
            &press([32., 72.]),
            None,
            nav,
            locale,
            &mut atlas,
        );
        assert!(owned && actions.is_empty() && controls.dragging());
        controls.cancel();
        let full = state(
            (1..=MAX_GUIDES as u64)
                .map(|id| Guide {
                    id,
                    axis: GuideAxis::X,
                    position: id as f64,
                })
                .collect(),
        );
        let (owned, actions) = draw(
            &mut controls,
            &full,
            &press([38., 370.]),
            None,
            nav,
            locale,
            &mut atlas,
        );
        assert!(owned && actions.is_empty() && !controls.dragging());
        assert!(controls.take_notice(locale).unwrap().contains("512"));
        assert_eq!(controls.invalid_attempts, 0);
        let mut observing = healthy.clone();
        observing["observing"] = true.into();
        for _ in 0..120 {
            assert!(draw(
                &mut controls,
                &observing,
                &press([38., 370.]),
                None,
                nav,
                locale,
                &mut atlas
            )
            .1
            .is_empty());
        }
        assert_eq!(controls.invalid_attempts, 0);
        assert!(controls.take_notice(locale).is_some());
        let mut missing = healthy.clone();
        missing["root"] = "".into();
        let (owned, actions) = draw(
            &mut controls,
            &missing,
            &press([38., 370.]),
            None,
            nav,
            locale,
            &mut atlas,
        );
        assert!(owned && actions.is_empty() && !controls.dragging());
        assert!(controls.take_notice(locale).is_some());
    }
}
