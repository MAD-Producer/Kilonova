use super::*;
use crate::engine::Engine;
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};
use motion_core::{Ease, Layer, LayerTimeline, Project, Track};
use motion_ui::dock::{Node, Panel};
const SIZE: [f32; 2] = [1000., 720.];

pub(in crate::panels) fn fixture(reverse: bool) -> (tempfile::TempDir, Engine) {
    let folder = tempfile::tempdir().unwrap();
    let mut p = Project::new(640, 360, 30, 60).unwrap();
    let mut layer = Layer::solid(
        7,
        "Mapped anchor",
        [100., 100.],
        [320., 180., 0.],
        [0.2, 0.5, 0.8, 1.],
    );
    layer.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 60,
        offset_frame: 2,
    });
    layer.time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(
            TimeStretch::new(3, 2).unwrap(),
            if reverse {
                TimeDirection::Reverse
            } else {
                TimeDirection::Forward
            },
            0.,
            if reverse { 0. } else { -6. },
        )
        .unwrap(),
        remap: None,
    });
    layer.transform.rotation = Track::constant([0., 0., 25.]);
    for (f, a) in [(-6, [0.2, 0.8]), (-3, [0.4, 0.6]), (0, [0.6, 0.4])] {
        layer.transform.anchor.upsert(f, a, Ease::Linear).unwrap();
        layer
            .transform
            .position
            .upsert(f, [320., 180., 0.], Ease::Linear)
            .unwrap();
    }
    layer.transform.position.separate().unwrap();
    p.layers.push(layer);
    motion_core::storage::save(folder.path(), &p).unwrap();
    let engine = Engine::start(
        folder.path().to_owned(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine.seek(6.5).unwrap();
    (folder, engine)
}
pub(in crate::panels) fn apply(engine: &Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::History(op) => {
                engine.history(op).unwrap();
            }
            Action::Edit(command) => {
                engine
                    .edit(serde_json::to_string(&command).unwrap())
                    .unwrap();
            }
            other => panic!("unexpected anchor action: {other:?}"),
        }
    }
}
fn pointer(point: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: point,
        held: (!release).then_some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, point)),
        released: release.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
fn draw(
    panels: &mut Panels,
    engine: &Engine,
    atlas: &mut TextAtlas,
    host: Panel,
    floating: bool,
    input: Input,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let state = engine.state().unwrap();
    let actions = if floating {
        super::super::floating_chrome(&mut paint, atlas, host, SIZE, panels, &state, &input)
    } else {
        super::super::chrome(
            &mut paint,
            atlas,
            SIZE,
            &Node::dock(host.dock()),
            panels,
            &state,
            &input,
        )
    };
    (paint, actions)
}
fn point(host: Panel, floating: bool) -> [f32; 2] {
    let styles = PaintList::default().styles();
    let mut area = if floating {
        Rect::new(0., 30., SIZE[0], SIZE[1] - 52.)
    } else {
        super::super::frames(&Node::dock(host.dock()), SIZE)
            .into_iter()
            .find(|f| f.panel == host)
            .unwrap()
            .content
    };
    if host == Panel::Timeline {
        area.max[0] = area.min[0] + super::super::GUTTER.min(area.width() * 0.52);
    }
    let row = styles.metric("row.height", 22.);
    let y = area.min[1]
        + if host == Panel::Timeline {
            super::super::timeline::header_height(&styles) + row * 2.
        } else {
            styles.metric("padding.panel", 8.) + row * 3.
        };
    let rect = if host == Panel::Timeline {
        super::super::timeline::property_layout(&styles, area, y, row, 0, 2).number
    } else {
        inspector_layout(&styles, area, y, 0, 2).number
    };
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn ready(host: Panel, locale: Locale) -> Panels {
    let mut p = Panels {
        locale,
        focused: Some(host),
        ..Default::default()
    };
    p.select_only(Some(7));
    p.timeline.expanded.insert(7);
    p
}
fn active(p: &Panels, host: Panel) -> bool {
    if host == Panel::Timeline {
        p.timeline_anchor.editing()
    } else {
        p.properties.anchor.editing()
    }
}

#[test]
fn mapped_anchor_numeric_drag_uses_exact_time_and_cancels_on_clock_change_in_all_hosts() {
    for locale in [Locale::Zh, Locale::En] {
        for host in [Panel::Properties, Panel::Timeline] {
            for floating in [false, true] {
                for reverse in [false, true] {
                    let (_folder, engine) = fixture(reverse);
                    let before = engine.state().unwrap();
                    let mut p = ready(host, locale);
                    let mut atlas =
                        TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                    let at = point(host, floating);
                    assert!(draw(
                        &mut p,
                        &engine,
                        &mut atlas,
                        host,
                        floating,
                        Input::default()
                    )
                    .1
                    .is_empty());
                    apply(
                        &engine,
                        draw(
                            &mut p,
                            &engine,
                            &mut atlas,
                            host,
                            floating,
                            pointer(at, true, false),
                        )
                        .1,
                    );
                    assert!(active(&p, host), "{host:?} {floating} {locale:?}");
                    let end = [at[0] + 20., at[1]];
                    let actions = draw(
                        &mut p,
                        &engine,
                        &mut atlas,
                        host,
                        floating,
                        pointer(end, false, false),
                    )
                    .1;
                    assert!(
                        actions.iter().any(
                            |a| matches!(a,Action::Edit(Command::AnchorAt{frame,..}) if *frame==6.5)
                        ),
                        "{actions:?}"
                    );
                    apply(&engine, actions);
                    assert_ne!(engine.state().unwrap()["project"], before["project"]);
                    engine.seek(11.).unwrap();
                    let actions = draw(
                        &mut p,
                        &engine,
                        &mut atlas,
                        host,
                        floating,
                        pointer(end, false, true),
                    )
                    .1;
                    assert!(
                        matches!(actions.as_slice(), [Action::History(4)]),
                        "{actions:?}"
                    );
                    apply(&engine, actions);
                    let after = engine.state().unwrap();
                    assert_eq!(after["project"], before["project"]);
                    assert_eq!(
                        after["history"]["undoEntries"],
                        before["history"]["undoEntries"]
                    );
                    assert!(!active(&p, host));
                }
            }
        }
    }
}

#[test]
fn continuous_anchor_uses_timed_compensation_instead_of_global_baking() {
    let (_folder, engine) = fixture(false);
    let mut s = engine.state().unwrap();
    let mut project: Project = serde_json::from_value(s["project"].clone()).unwrap();
    let anchor = &mut project.layers[0].transform.anchor;
    anchor.spatial_animation = Some(Box::new(
        motion_model::SpatialAnimation::from_track(anchor).unwrap(),
    ));
    anchor.keys.clear();
    let ids: Vec<_> = anchor
        .spatial_animation
        .as_ref()
        .unwrap()
        .keys()
        .iter()
        .map(|k| k.id)
        .collect();
    let mut core = motion_core::Engine::new(project).unwrap();
    let before = core.snapshot();
    s["project"] = serde_json::to_value(&before).unwrap();
    let command = anchor_command(&s, 7, [0.7, 0.2]);
    assert!(matches!(command, Command::AnchorAt {frame,..} if frame==6.5));
    core.apply(command).unwrap();
    let anchor = &core.project().layers[0].transform.anchor;
    assert_eq!(anchor.sample(-3.).unwrap(), [0.7, 0.2]);
    assert_eq!(
        anchor
            .spatial_animation
            .as_ref()
            .unwrap()
            .keys()
            .iter()
            .map(|k| k.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert!(core.undo().unwrap());
    assert_eq!(core.snapshot(), before);
}

#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires native GPU windows; run explicitly"]
fn native_mapped_anchor_controls_render_cancel_commit_and_undo_in_two_windows() {
    use std::{path::PathBuf, sync::Arc, time::Duration};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root = PathBuf::from(
        std::env::var_os("MOTION_ANCHOR_TIME_NATIVE_ARTIFACTS").expect("artifact directory"),
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
    for (locale, host, floating, reverse) in [
        (Locale::Zh, Panel::Properties, false, false),
        (Locale::En, Panel::Timeline, true, true),
    ] {
        let (_folder, engine) = fixture(reverse);
        let before = engine.state().unwrap();
        crate::native_test_support::receive(
            &mut events,
            &engine
                .attach_async(windows[0].clone(), SIZE[0] as u32, SIZE[1] as u32)
                .unwrap(),
            Duration::from_secs(30),
        )
        .unwrap()
        .unwrap();
        engine
            .float_window(1, windows[1].clone(), SIZE[0] as u32, SIZE[1] as u32)
            .unwrap();
        let mut p = ready(host, locale);
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let tag = format!("{host:?}-{locale:?}-{floating}");
        let mut capture = |p: &mut Panels, atlas: &mut TextAtlas, name: &str| {
            let (paint, actions) = draw(p, &engine, atlas, host, floating, Input::default());
            assert!(actions.is_empty());
            let cfg = atlas.config();
            let path = root.join(format!("{tag}-{name}.png"));
            crate::native_test_support::capture_ui(
                &mut events,
                &engine,
                crate::engine::UiFrame {
                    views: None,
                    inspection: Default::default(),
                    paint,
                    atlas: Some(crate::engine::AtlasUpload {
                        pixels: atlas.pixel_data().to_vec(),
                        width: cfg.width,
                        height: cfg.height,
                    }),
                    logical_size: SIZE,
                    composition: None,
                    composition_image: None,
                    window: u64::from(floating),
                    node_preview: None,
                },
                path.clone(),
            )
            .unwrap();
            assert_eq!(
                image::open(path).unwrap().into_rgba8().dimensions(),
                (SIZE[0] as u32, SIZE[1] as u32)
            );
        };
        capture(&mut p, &mut atlas, "before");
        let at = point(host, floating);
        let end = [at[0] + 20., at[1]];
        for commit in [false, true] {
            apply(
                &engine,
                draw(
                    &mut p,
                    &engine,
                    &mut atlas,
                    host,
                    floating,
                    pointer(at, true, false),
                )
                .1,
            );
            apply(
                &engine,
                draw(
                    &mut p,
                    &engine,
                    &mut atlas,
                    host,
                    floating,
                    pointer(end, false, false),
                )
                .1,
            );
            if commit {
                apply(
                    &engine,
                    draw(
                        &mut p,
                        &engine,
                        &mut atlas,
                        host,
                        floating,
                        pointer(end, false, true),
                    )
                    .1,
                );
            } else {
                apply(
                    &engine,
                    draw(
                        &mut p,
                        &engine,
                        &mut atlas,
                        host,
                        floating,
                        Input {
                            focus_lost: true,
                            ..Default::default()
                        },
                    )
                    .1,
                );
                assert_eq!(engine.state().unwrap()["project"], before["project"]);
            }
        }
        let after = engine.state().unwrap();
        assert_ne!(after["project"], before["project"]);
        capture(&mut p, &mut atlas, "committed");
        assert_eq!(engine.history(0).unwrap()["project"], before["project"]);
        assert_eq!(engine.history(1).unwrap()["project"], after["project"]);
        std::fs::write(
            root.join(format!("{tag}-after.json")),
            serde_json::to_vec_pretty(&after).unwrap(),
        )
        .unwrap();
        engine.close_window(1).unwrap();
    }
}
