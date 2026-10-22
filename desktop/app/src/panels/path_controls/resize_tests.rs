use super::*;
use motion_core::vector::{
    groups::{GroupItem, VectorGroup},
    ShapeKind, SHAPES,
};

fn state(engine: &motion_core::Engine, frame: f64) -> Value {
    serde_json::json!({"root":"resize-fixture","composition":"comp-main","frame":frame,"revision":engine.revision(),
        "project":engine.project(),"source_transforms":[{"id":7,"mvp":DMat4::from_scale(glam::DVec3::new(2./640.,2./480.,1.)).to_cols_array()}]})
}
fn draw(
    controls: &mut Controls,
    engine: &motion_core::Engine,
    frame: f64,
    atlas: &mut TextAtlas,
    input: Input,
) -> Vec<Action> {
    let mut out = vec![];
    controls.draw(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 640., 480.),
        &state(engine, frame),
        Some(7),
        &input,
        Locale::Zh,
        &mut out,
    );
    out
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for a in actions {
        match a {
            Action::Edit(c) => engine.apply(c).unwrap(),
            Action::EditBatch(commands) => {
                engine.apply_batch(commands).unwrap();
            }
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            _ => panic!("unexpected {a:?}"),
        }
    }
}
fn layer(vector: VectorContent) -> Layer {
    let mut l = Layer::solid(7, "Parametric geometry", [160., 100.], [0.; 3], [1.; 4]);
    l.content = Content::Vector { vector };
    l
}
fn engine(layer: Layer) -> motion_core::Engine {
    let mut project = motion_core::Project::new(640, 480, 30, 120).unwrap();
    project.version = motion_model::CURRENT_PROJECT_VERSION;
    project.layers.push(layer);
    motion_core::Engine::new(project).unwrap()
}
fn resize(
    engine: &mut motion_core::Engine,
    atlas: &mut TextAtlas,
    frame: f64,
    path: &Value,
    corner: usize,
) -> [f32; 2] {
    let base = layer_projection(&state(engine, frame), 7, Rect::new(0., 0., 640., 480.)).unwrap();
    let p = source_geometry::projection(&base, path).unwrap();
    let g = geometry(&path["nodes"][corner]).unwrap();
    let sign = match corner {
        0 => [-1., -1.],
        1 => [1., -1.],
        2 => [1., 1.],
        _ => [-1., 1.],
    };
    let start = p.screen([g[0], g[1]]).unwrap();
    let release = p
        .screen([g[0] + sign[0] * 20., g[1] + sign[1] * 10.])
        .unwrap();
    let mut controls = Controls {
        tool: Tool::Pen,
        ..Default::default()
    };
    let out = draw(
        &mut controls,
        engine,
        frame,
        atlas,
        Input {
            mouse: start,
            pressed: Some((MouseButton::Left, start)),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    apply(engine, out);
    assert!(controls.drag.as_ref().unwrap().bounds.is_some());
    let out = draw(
        &mut controls,
        engine,
        frame,
        atlas,
        Input {
            mouse: [620., 450.],
            released: Some((MouseButton::Left, release)),
            ..Default::default()
        },
    );
    assert_eq!(
        out.iter()
            .filter(|a| matches!(a, Action::EditBatch(commands) if commands.len()==2))
            .count(),
        1
    );
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Edit(Command::Vector {
            action: VectorAction::SetNode { .. } | VectorAction::ConvertToPath { .. },
            ..
        })
    )));
    apply(engine, out);
    assert!(!controls.gesture_active());
    let after = engine.project().clone();
    assert!(draw(
        &mut controls,
        engine,
        frame,
        atlas,
        Input {
            mouse: [600., 400.],
            held: Some(MouseButton::Left),
            ..Default::default()
        }
    )
    .is_empty());
    assert_eq!(engine.project(), &after);
    sign
}

#[test]
fn shape_corners_resize_width_height_keep_opposite_corner_and_preserve_parameters() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for (kind, _, _, _) in SHAPES {
        for corner in 0..4 {
            let vector = VectorContent::shape(kind);
            let mut engine = engine(layer(vector.clone()));
            let before = engine.project().clone();
            let path = source_geometry::paths(&engine.project().layers[0], 0.)
                .unwrap()
                .remove(0);
            let sign = resize(&mut engine, &mut atlas, 0., &path, corner);
            let Content::Vector { vector: v } = &engine.project().layers[0].content else {
                panic!()
            };
            let VectorSource::Group { group } = &v.source else {
                panic!("dimensions use the existing geometry tracks")
            };
            let GroupItem::Geometry {
                vector: inner,
                size,
                position,
                ..
            } = &group.items[0]
            else {
                panic!()
            };
            assert_eq!(
                **inner, vector,
                "shape parameters and paints remain authored: {kind:?}"
            );
            let size = size.sample(0.).unwrap();
            let position = position.sample(0.).unwrap();
            for i in 0..2 {
                assert!((size[i] - [180., 110.][i]).abs() < 0.002);
                assert!((position[i] - sign[i] * [10., 5.][i]).abs() < 0.002);
                assert!(
                    (position[i] - sign[i] * size[i] * 0.5
                        + sign[i] * before.layers[0].size[i] * 0.5)
                        .abs()
                        < 0.002
                );
            }
            assert_eq!(engine.project().layers[0].size, before.layers[0].size);
            assert_eq!(
                engine.project().layers[0].transform,
                before.layers[0].transform
            );
            let changed = engine.project().clone();
            let root = tempfile::tempdir().unwrap();
            motion_core::storage::save(root.path(), engine.project()).unwrap();
            assert_eq!(motion_core::storage::load(root.path()).unwrap(), changed);
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
            engine.redo().unwrap();
            assert_eq!(engine.project(), &changed);
        }
    }
}

#[test]
fn nested_geometry_uses_its_transform_owner_and_animated_local_time() {
    let shape = VectorContent::shape(ShapeKind::RoundedRectangle);
    let mut wrapped = VectorGroup::wrap(shape, [160., 100.]);
    let VectorSource::Group { group } = &mut wrapped.source else {
        panic!()
    };
    group.id = 10;
    group.transform.rotation = Track::constant(31.);
    group.transform.scale = Track::constant([130., 75.]);
    group.transform.skew = Track::constant(12.);
    let GroupItem::Geometry {
        id, size, position, ..
    } = &mut group.items[0]
    else {
        panic!()
    };
    *id = 32;
    size.upsert(10, [160., 100.], motion_core::Ease::Linear)
        .unwrap();
    size.upsert(30, [200., 140.], motion_core::Ease::Linear)
        .unwrap();
    *position = Track::constant([30., 20.]);
    let inner = group.clone();
    group.id = 1;
    group.transform = Default::default();
    group.transform.position = Track::constant([15., -10.]);
    group.items = vec![GroupItem::Group { group: inner }];
    let mut l = layer(wrapped);
    l.timeline = Some(motion_core::LayerTimeline {
        in_frame: 12,
        out_frame: 112,
        offset_frame: 12,
    });
    let mut engine = engine(l);
    let before = engine.project().clone();
    let path = source_geometry::paths(&engine.project().layers[0], 30.)
        .unwrap()
        .remove(0);
    assert_eq!(path["item"], 32);
    let base = layer_projection(&state(&engine, 30.), 7, Rect::new(0., 0., 640., 480.)).unwrap();
    let projection = source_geometry::projection(&base, &path).unwrap();
    let fixed = projection.screen([-88., -58.]).unwrap(); // local frame 18, sampled size 176×116
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    resize(&mut engine, &mut atlas, 30., &path, 2);
    let after = source_geometry::paths(&engine.project().layers[0], 30.)
        .unwrap()
        .remove(0);
    let p = source_geometry::projection(&base, &after).unwrap();
    let g = geometry(&after["nodes"][0]).unwrap();
    assert!(distance(p.screen([g[0], g[1]]).unwrap(), fixed) < 0.003);
    let Content::Vector {
        vector:
            VectorContent {
                source: VectorSource::Group { group },
                ..
            },
    } = &engine.project().layers[0].content
    else {
        panic!()
    };
    let GroupItem::Group { group } = &group.items[0] else {
        panic!()
    };
    let GroupItem::Geometry { size, .. } = &group.items[0] else {
        panic!()
    };
    assert_eq!(
        size.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        vec![10, 18, 30]
    );
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn grouped_path_grips_keep_owner_identity_before_repeaters_and_trim() {
    use motion_core::vector::groups::Repeater;
    let path = VectorPath {
        id: 1,
        closed: false,
        nodes: vec![
            PathNode {
                id: 11,
                geometry: Track::constant([0., 0., -20., 0., 20., 0.]),
            },
            PathNode {
                id: 42,
                geometry: Track::constant([40., 60., 0., 0., 0., 0.]),
            },
        ],
    };
    let mut vector = VectorGroup::wrap(new_vector(path), [160., 100.]);
    let VectorSource::Group { group } = &mut vector.source else {
        panic!()
    };
    let mut second = group.items[0].clone();
    let GroupItem::Geometry { id, position, .. } = &mut second else {
        panic!()
    };
    *id = 42;
    *position = Track::constant([170., 0.]);
    group.items.push(second);
    group.items.push(GroupItem::Repeater {
        id: 50,
        name: "Copies".into(),
        repeater: Repeater::default(),
    });
    group.transform.rotation = Track::constant(17.);
    let mut engine = engine(layer(vector));
    let before = engine.project().clone();
    let paths = source_geometry::paths(&engine.project().layers[0], 0.).unwrap();
    assert_eq!(
        paths.len(),
        2,
        "generated copies must not create fake node addresses"
    );
    let base = layer_projection(&state(&engine, 0.), 7, Rect::new(0., 0., 640., 480.)).unwrap();
    let p = source_geometry::projection(&base, &paths[1]).unwrap();
    let start = p.screen([0., 0.]).unwrap();
    let release = p.screen([12., 8.]).unwrap();
    let mut controls = Controls {
        tool: Tool::Pen,
        ..Default::default()
    };
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let out = draw(
        &mut controls,
        &engine,
        0.,
        &mut atlas,
        Input {
            mouse: start,
            pressed: Some((MouseButton::Left, start)),
            held: Some(MouseButton::Left),
            ..Default::default()
        },
    );
    apply(&mut engine, out);
    assert_eq!(controls.drag.as_ref().unwrap().item, Some(42));
    let out = draw(
        &mut controls,
        &engine,
        0.,
        &mut atlas,
        Input {
            mouse: [630., 450.],
            released: Some((MouseButton::Left, release)),
            ..Default::default()
        },
    );
    apply(&mut engine, out);
    let after = source_geometry::paths(&engine.project().layers[0], 0.).unwrap();
    assert_eq!(
        after[0], paths[0],
        "same inner path/node IDs must not select the first geometry"
    );
    let g = geometry(&after[1]["nodes"][0]).unwrap();
    assert!((g[0] - 12.).abs() < 0.002 && (g[1] - 8.).abs() < 0.002);
    let Content::Vector {
        vector:
            VectorContent {
                source: VectorSource::Group { group },
                ..
            },
    } = &engine.project().layers[0].content
    else {
        panic!()
    };
    let Content::Vector {
        vector:
            VectorContent {
                source: VectorSource::Group { group: old },
                ..
            },
    } = &before.layers[0].content
    else {
        panic!()
    };
    assert_eq!(group.items[2], old.items[2], "repeater is preserved");
    engine.undo().unwrap();
    assert_eq!(engine.project(), &before);
}

#[test]
fn invalid_authored_geometry_reports_default_level_file_and_stderr_and_recovers() {
    const CHILD: &str = "MOTION_SOURCE_GEOMETRY_FAILURE_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let mut wrapped =
            VectorGroup::wrap(VectorContent::shape(ShapeKind::Rectangle), [160., 100.]);
        let valid = engine(layer(wrapped.clone()));
        let good = state(&valid, 0.);
        let VectorSource::Group { group } = &mut wrapped.source else {
            panic!()
        };
        group.transform.position = Track::constant([40000., 0.]);
        let mut bad = good.clone();
        bad["project"]["layers"][0] = serde_json::to_value(layer(wrapped)).unwrap();
        let mut controls = Controls {
            tool: Tool::Pen,
            ..Default::default()
        };
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut draw = |state: &Value| {
            let mut actions = vec![];
            controls.draw(
                &mut PaintList::default(),
                &mut atlas,
                Rect::new(0., 0., 640., 480.),
                state,
                Some(7),
                &Input::default(),
                Locale::Zh,
                &mut actions,
            );
            assert!(actions.is_empty());
        };
        let errors = || {
            std::fs::read_to_string(root.join("motion-studio.log"))
                .unwrap()
                .lines()
                .filter(|l| l.contains("ERROR") && l.contains("site=path.source_geometry "))
                .count()
        };
        for _ in 0..59 {
            draw(&bad);
        }
        assert_eq!(errors(), 0);
        draw(&bad);
        assert_eq!(errors(), 1);
        for _ in 0..80 {
            draw(&bad);
        }
        assert_eq!(errors(), 1);
        draw(&good);
        for _ in 0..59 {
            draw(&bad);
        }
        assert_eq!(errors(), 1);
        draw(&bad);
        assert_eq!(errors(), 2);
        let log = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        assert!(log.contains("object=7 frame=0.0 phase=authored_grips reason=sampled group vector outside valid range"),"{log}");
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let out=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::path_controls::resize_tests::invalid_authored_geometry_reports_default_level_file_and_stderr_and_recovers"])
        .env_remove("MOTION_LOG").env(CHILD,root.path()).output().unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        stderr
            .lines()
            .filter(|l| l.contains("site=path.source_geometry "))
            .count(),
        2,
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("persistent degradation"));
}

#[test]
fn rejected_second_dimension_command_cannot_publish_a_partial_resize() {
    let mut engine=engine(layer(VectorContent::shape(ShapeKind::Rectangle)));
    let before=engine.project().clone();
    let path=source_geometry::paths(&engine.project().layers[0],0.).unwrap().remove(0);
    let bounds=source_geometry::Bounds::from_path(&path,3).unwrap();
    let mut actions=bounds.edit(7,None,0,[20.,10.]).unwrap();
    let Action::EditBatch(mut commands)=actions.remove(0) else {panic!("resize requires atomic batch")};
    let Command::Vector {action:VectorAction::SetGroupParameter {item,..},..}=&mut commands[1] else {panic!()};
    *item=999;
    let error=engine.apply_batch(commands).unwrap_err();
    assert!(error.to_string().contains("missing"),"original cause: {error}");
    assert_eq!(engine.project(),&before);
    engine.begin_gesture().unwrap();engine.end_gesture(false).unwrap();
}
