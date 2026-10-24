use super::*;
use motion_core::{
    vector::{
        groups::{GroupItem, VectorGroup},
        ShapeKind, VectorContent, VectorSource,
    },
    Command, Content, Layer, Track,
};

fn fixture() -> motion_core::Engine {
    let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
    for id in [7, 8] {
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        vector.fill = Some(Track::constant(if id == 7 {
            [0.2, 0.4, 0.6, 1.]
        } else {
            [0.8, 0.5, 0.3, 0.7]
        }));
        vector.stroke = VectorContent::shape(ShapeKind::Line).stroke;
        vector.stroke.as_mut().unwrap().width = Track::constant(id as f32);
        let name = format!("Toolbar {id}");
        let mut layer = Layer::solid(
            id,
            &name,
            [200., 120.],
            if id == 7 {
                [250., 160., 0.]
            } else {
                [430., 210., 0.]
            },
            [1.; 4],
        );
        layer.content = Content::Vector { vector };
        project.layers.push(layer);
    }
    motion_core::Engine::new(project).unwrap()
}
fn state(engine: &motion_core::Engine) -> Value {
    serde_json::json!({"root":"toolbar-test","composition":"main","frame":0,"project":engine.project(),"revision":engine.revision()})
}
fn vector(engine: &motion_core::Engine, index: usize) -> &VectorContent {
    let Content::Vector { vector } = &engine.project().layers[index].content else {
        panic!()
    };
    vector
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            other => panic!("unexpected toolbar action {other:?}"),
        }
    }
}
fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn pointer(at: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: at,
        pressed: press.then_some((MouseButton::Left, at)),
        released: release.then_some((MouseButton::Left, at)),
        held: (!release).then_some(MouseButton::Left),
        ..Default::default()
    }
}
fn draw(
    c: &mut Controls,
    engine: &motion_core::Engine,
    atlas: &mut TextAtlas,
    ids: &[u64],
    input: &Input,
    locale: Locale,
) -> Vec<Action> {
    let state = state(engine);
    let mut out = vec![];
    let mut list = PaintList::default();
    c.prepare(&state, ids, false, input, &mut out);
    c.render(
        &mut list,
        atlas,
        Rect::new(0., 48., 900., 32.),
        &state,
        ids,
        false,
        input,
        locale,
        &mut out,
    );
    c.overlay(
        &mut list,
        atlas,
        [900., 600.],
        &state,
        ids,
        input,
        locale,
        &mut out,
    );
    out
}
#[test]
fn mixed_width_scrub_commits_one_undo_and_saves_current_style() {
    let mut engine = fixture();
    let before = engine.project().clone();
    let mut c = Controls::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    assert!(draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7, 8],
        &Input::default(),
        Locale::Zh
    )
    .is_empty());
    assert_eq!(
        Selection::read(&state(&engine), &[7, 8])
            .unwrap()
            .summary()
            .unwrap()
            .width,
        Shared::Mixed
    );
    let p = center(c.hits["width"]);
    apply(
        &mut engine,
        draw(
            &mut c,
            &fixture(),
            &mut atlas,
            &[7, 8],
            &pointer(p, true, false),
            Locale::Zh,
        ),
    );
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7, 8],
        &pointer([p[0] + 20., p[1]], false, false),
        Locale::Zh,
    );
    assert!(matches!(
        actions.as_slice(),
        [Action::History(2), Action::EditBatch(_)]
    ));
    apply(&mut engine, actions);
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7, 8],
        &pointer([p[0] + 35., p[1]], false, true),
        Locale::Zh,
    );
    apply(&mut engine, actions);
    assert_eq!(
        vector(&engine, 0).stroke.as_ref().unwrap().width.value,
        10.5
    );
    assert_eq!(
        vector(&engine, 1).stroke.as_ref().unwrap().width.value,
        10.5
    );
    assert_eq!(vector(&engine, 0).fill, vector(&fixture(), 0).fill);
    let dir = tempfile::tempdir().unwrap();
    motion_core::storage::save(dir.path(), engine.project()).unwrap();
    assert_eq!(
        &motion_core::storage::load(dir.path()).unwrap(),
        engine.project()
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
}
#[test]
fn colors_preview_across_mixed_targets_and_cancel_restores_every_channel() {
    for locale in [Locale::Zh, Locale::En] {
        let mut engine = fixture();
        let before = engine.project().clone();
        let mut c = Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        draw(
            &mut c,
            &engine,
            &mut atlas,
            &[7, 8],
            &Input::default(),
            locale,
        );
        let p = center(c.hits["fill"]);
        assert!(draw(
            &mut c,
            &engine,
            &mut atlas,
            &[7, 8],
            &pointer(p, true, false),
            locale
        )
        .is_empty());
        let p = center(c.hits["r"]);
        apply(
            &mut engine,
            draw(
                &mut c,
                &fixture(),
                &mut atlas,
                &[7, 8],
                &pointer(p, true, false),
                locale,
            ),
        );
        let actions = draw(
            &mut c,
            &engine,
            &mut atlas,
            &[7, 8],
            &pointer([p[0] + 40., p[1]], false, true),
            locale,
        );
        apply(&mut engine, actions);
        assert_ne!(engine.project(), &before);
        assert_eq!(vector(&engine, 0).fill.as_ref().unwrap().value[1], 0.4);
        assert_eq!(vector(&engine, 1).fill.as_ref().unwrap().value[1], 0.5);
        let p = center(c.hits["cancel"]);
        let actions = draw(
            &mut c,
            &engine,
            &mut atlas,
            &[7, 8],
            &pointer(p, true, false),
            locale,
        );
        assert!(matches!(actions.as_slice(), [Action::History(4)]));
        apply(&mut engine, actions);
        assert_eq!(engine.project(), &before);
        assert!(!c.editing());
    }
}
#[test]
fn width_text_input_and_escape_use_existing_number_editor_contract() {
    let mut engine = fixture();
    let before = engine.project().clone();
    let mut c = Controls::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &Input::default(),
        Locale::En,
    );
    let p = center(c.hits["width"]);
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer(p, true, false),
        Locale::En,
    );
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer(p, false, true),
        Locale::En,
    );
    assert!(c.width.text.active());
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &Input {
            text: "12.75".into(),
            ..Default::default()
        },
        Locale::En,
    );
    apply(&mut engine, actions);
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        },
        Locale::En,
    );
    apply(&mut engine, actions);
    assert_eq!(
        vector(&engine, 0).stroke.as_ref().unwrap().width.value,
        12.75
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer(p, true, false),
        Locale::En,
    );
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer([p[0] + 50., p[1]], false, false),
        Locale::En,
    );
    apply(&mut engine, actions);
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &Input {
            keys: vec![(Key::Escape, Default::default())],
            ..Default::default()
        },
        Locale::En,
    );
    apply(&mut engine, actions);
    assert_eq!(engine.project(), &before);
}
#[test]
fn grouped_paint_edits_preserve_animated_geometry_and_other_keys() {
    let mut engine = fixture();
    let mut v = vector(&engine, 0).clone();
    v.fill
        .as_mut()
        .unwrap()
        .upsert(0, [0.1, 0.2, 0.3, 1.], motion_core::Ease::Linear)
        .unwrap();
    v.fill
        .as_mut()
        .unwrap()
        .upsert(30, [0.8, 0.7, 0.6, 1.], motion_core::Ease::Linear)
        .unwrap();
    v = VectorGroup::wrap(v, [200., 120.]);
    engine
        .apply(Command::Vector {
            object: 7,
            action: motion_core::vector::VectorAction::Replace { vector: v },
        })
        .unwrap();
    let before = vector(&engine, 0).clone();
    let selection = Selection::read(&state(&engine), &[7]).unwrap();
    let mut paint = Paint::default();
    paint.fill_color[0] = Some(0.9);
    paint.width = Some(19.);
    engine
        .apply_batch(selection.commands(&paint).unwrap())
        .unwrap();
    let VectorSource::Group { group } = &vector(&engine, 0).source else {
        panic!()
    };
    let GroupItem::Geometry {
        vector: geometry, ..
    } = &group.items[0]
    else {
        panic!()
    };
    let VectorSource::Group { group: old } = &before.source else {
        panic!()
    };
    let GroupItem::Geometry { vector: old, .. } = &old.items[0] else {
        panic!()
    };
    assert_eq!(geometry.source, old.source);
    assert_eq!(
        geometry.fill.as_ref().unwrap().keys[1],
        old.fill.as_ref().unwrap().keys[1]
    );
    assert_eq!(
        geometry.fill.as_ref().unwrap().keys[0].value,
        [0.9, 0.2, 0.3, 1.]
    );
}
#[test]
fn none_modes_and_reenable_do_not_leave_duplicate_group_operators() {
    let mut engine = fixture();
    let selection = Selection::read(&state(&engine), &[7]).unwrap();
    engine
        .apply_batch(
            selection
                .commands(&Paint {
                    fill: Some(0),
                    stroke: Some(false),
                    ..Default::default()
                })
                .unwrap(),
        )
        .unwrap();
    assert!(vector(&engine, 0).fill.is_none());
    assert!(vector(&engine, 0).stroke.is_none());
    let selection = Selection::read(&state(&engine), &[7]).unwrap();
    engine
        .apply_batch(
            selection
                .commands(&Paint {
                    fill: Some(1),
                    stroke: Some(true),
                    width: Some(21.),
                    ..Default::default()
                })
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        vector(&engine, 0).fill_rule,
        motion_core::vector::FillRule::NonZero
    );
    assert_eq!(vector(&engine, 0).stroke.as_ref().unwrap().width.value, 21.);
}
#[test]
fn locked_masks_unsupported_targets_and_changed_context_do_not_edit() {
    let engine = fixture();
    let mut c = Controls::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mut value = state(&engine);
    value["project"]["layers"][0]["locked"] = true.into();
    assert!(Selection::read(&value, &[7, 8]).is_err());
    assert!(Selection::read(&value, &[]).is_err());
    value["observing"] = true.into();
    assert!(Selection::read(&value, &[8]).is_err());
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &Input::default(),
        Locale::Zh,
    );
    let p = center(c.hits["width"]);
    draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer(p, true, false),
        Locale::Zh,
    );
    let actions = draw(
        &mut c,
        &engine,
        &mut atlas,
        &[7],
        &pointer([p[0] + 20., p[1]], false, false),
        Locale::Zh,
    );
    let mut engine = engine;
    apply(&mut engine, actions);
    let before = fixture().project().clone();
    let mut out = vec![];
    c.prepare(&state(&engine), &[8], false, &Input::default(), &mut out);
    assert!(matches!(out.as_slice(), [Action::History(4)]));
    apply(&mut engine, out);
    assert_eq!(engine.project(), &before);
    let mut out = vec![];
    c.render(
        &mut PaintList::default(),
        &mut atlas,
        Rect::new(0., 48., 900., 32.),
        &state(&engine),
        &[7],
        true,
        &pointer(p, true, false),
        Locale::Zh,
        &mut out,
    );
    assert!(out.is_empty());
    assert!(!c.editing());
    assert!(c.notice.as_ref().unwrap().contains("蒙版"));
}

#[test]
fn toolbar_failures_are_bounded_at_info_and_preserve_stderr_stdout_channels() {
    const NAME:&str="panels::toolbar_style::tests::toolbar_failures_are_bounded_at_info_and_preserve_stderr_stdout_channels";
    const CHILD: &str = "MOTION_TOOLBAR_FAILURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let healthy = state(&fixture());
        for _ in 0..120 {
            assert!(Selection::read(&healthy, &[]).is_err());
            assert!(Selection::read(&healthy, &[7]).is_ok());
        }
        let log = root.join("motion-studio.log");
        assert!(!std::fs::read_to_string(&log)
            .unwrap()
            .contains("toolbar.paint"));
        let mut invalid = healthy.clone();
        invalid["project"]["layers"][0]["content"] =
            serde_json::json!({"kind":"vector","vector":17});
        for _ in 0..120 {
            assert!(Selection::read(&invalid, &[7]).is_err());
            assert!(Selection::read(&healthy, &[8]).is_ok());
        }
        // Deserialization rejects scalar separated tracks. Inject a corrupt
        // in-memory cache to exercise the defensive sampling branch without
        // relaxing that model contract or changing a production return value.
        let bad_sample = healthy.clone();
        let mut corrupt = Selection::read(&healthy, &[7]).unwrap();
        corrupt.targets[0]
            .vector
            .stroke
            .as_mut()
            .unwrap()
            .width
            .axes = Some(Box::new(motion_core::AxisTracks {
            x: Track::constant(1.),
            y: Track::constant(1.),
            z: Track::constant(1.),
        }));
        let mut sampler = Controls::default();
        sampler.selection(&healthy, &[7]).unwrap();
        sampler.cache.as_mut().unwrap().1 = Arc::new(corrupt);
        let mut atlas = TextAtlas::from_family("system-ui", 18., false).unwrap();
        for _ in 0..120 {
            sampler.render(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 48., 900., 32.),
                &bad_sample,
                &[7],
                false,
                &Input::default(),
                Locale::En,
                &mut vec![],
            );
            assert!(Selection::read(&healthy, &[8]).unwrap().summary().is_ok());
        }
        assert!(sampler
            .notice
            .as_ref()
            .unwrap()
            .contains("cannot reconstruct sampled dimensions"));
        sampler.session = Some(Session {
            selection: Arc::new(Selection::read(&healthy, &[7]).unwrap()),
            paint: Default::default(),
            begun: false,
        });
        sampler.popup = Some(Popup {
            kind: Kind::Fill,
            anchor: Rect::new(0., 0., 28., 26.),
            opened: true,
            fields: Default::default(),
            mode: Default::default(),
        });
        let mut rejected = vec![];
        sampler.overlay(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &bad_sample,
            &[7],
            &Input::default(),
            Locale::En,
            &mut rejected,
        );
        assert!(rejected.is_empty());
        assert!(!sampler.editing());
        let mut invalid = healthy;
        let mut vector =
            VectorGroup::wrap(VectorContent::shape(ShapeKind::Rectangle), [200., 120.]);
        let VectorSource::Group { group } = &mut vector.source else {
            panic!()
        };
        group.id = u64::MAX;
        let GroupItem::Geometry {
            vector: geometry, ..
        } = &mut group.items[0]
        else {
            panic!()
        };
        geometry.fill = None;
        geometry.stroke = None;
        invalid["project"]["layers"][0]["content"] =
            serde_json::json!({"kind":"vector","vector":vector});
        let mut c = Controls::default();
        c.session = Some(Session {
            selection: Arc::new(Selection::read(&invalid, &[7]).unwrap()),
            paint: Paint {
                fill: Some(1),
                ..Default::default()
            },
            begun: false,
        });
        let mut actions = vec![];
        c.preview(&mut actions);
        assert!(actions.is_empty());
        assert!(!c.editing());
        assert!(c.notice.as_ref().unwrap().contains("ID exhausted"));
        let text = std::fs::read_to_string(&log).unwrap();
        assert_eq!(
            text.lines()
                .filter(|l| l.contains("site=toolbar.paint_layer"))
                .count(),
            2
        );
        assert_eq!(
            text.lines()
                .filter(|l| l.contains("site=toolbar.paint_layer count=60"))
                .count(),
            1
        );
        assert!(text.contains("objects=[7] frame=0 phase=prepare cause=vector item ID exhausted"));
        assert_eq!(
            text.lines()
                .filter(|line| line.contains("site=toolbar.paint_sample"))
                .count(),
            2
        );
        assert!(text.contains("phase=popup_sample cause=track sample:"));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD, directory.path())
        .env_remove("MOTION_LOG")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log = std::fs::read_to_string(directory.path().join("motion-studio.log")).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    for record in log.lines().filter(|line| line.contains("ERROR")) {
        let message = record.split("motion_studio::").last().unwrap();
        assert!(
            stderr.contains(message),
            "Error missing on stderr: {message}"
        );
        assert!(!stdout.contains(message), "Error leaked to stdout");
    }
}

#[cfg(all(target_os = "windows", feature = "gpu-tests"))]
mod native;

#[test]
fn popup_owns_shortcuts_and_its_outside_click_without_timeline_seek() {
    let engine = fixture();
    let state = state(&engine);
    let mut panels = crate::panels::Panels {
        locale: Locale::En,
        ..Default::default()
    };
    panels.select_only(Some(7));
    let layout = motion_ui::dock::Node::dock(motion_ui::dock::Panel::Timeline.dock());
    let mut atlas = TextAtlas::from_family("system-ui", 18., false).unwrap();
    let mut frame = |panels: &mut crate::panels::Panels, input: &Input| {
        crate::panels::chrome(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &layout,
            panels,
            &state,
            input,
        )
    };
    assert!(frame(&mut panels, &Input::default()).is_empty());
    let p = center(panels.toolbar_style.hits["fill"]);
    assert!(frame(&mut panels, &pointer(p, true, false)).is_empty());
    for key in [Key::Space, Key::Delete, Key::A] {
        let actions = frame(
            &mut panels,
            &Input {
                keys: vec![(
                    key,
                    motion_ui::input::Modifiers {
                        control: key == Key::A,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            },
        );
        assert!(actions.is_empty(), "popup leaked key {key:?}: {actions:?}");
    }
    let actions = frame(&mut panels, &pointer([850., 400.], true, false));
    assert!(
        actions.is_empty(),
        "popup closing click leaked to timeline: {actions:?}"
    );
    assert!(!panels.toolbar_style.editing());
}

#[test]
fn snapshot_cache_is_revision_scoped_and_released_when_selection_disappears() {
    let mut c = Controls::default();
    let mut engine = fixture();
    let value = state(&engine);
    let first = c.selection(&value, &[7]).unwrap();
    let again = c.selection(&value, &[7]).unwrap();
    assert!(Arc::ptr_eq(&first, &again));
    engine
        .apply(Command::Flags {
            object: 7,
            visible: true,
            locked: true,
        })
        .unwrap();
    assert!(c.selection(&state(&engine), &[7]).is_err());
    assert!(c.cache.is_none());
    assert!(c.selection(&state(&engine), &[8]).is_ok());
    assert!(c.selection(&state(&engine), &[]).is_err());
    assert!(c.cache.is_none());
}
