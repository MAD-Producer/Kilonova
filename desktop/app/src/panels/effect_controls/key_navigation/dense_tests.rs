//! Exercise the real row diamond and its bounded typed editing transaction.
use super::*;
use motion_core::{
    CurveObject, CurveTrack, Ease, EffectInstance, HIGH_PRECISION_CURVE_SAMPLES as N, Keyframe,
    Layer, Project, color_curves::ColorInterpolation,
};
use motion_effects::image_contract::ChannelPrecision;

const PROBE: usize = 2049;
fn source(height: f32, channel_tables: bool) -> CurveObject {
    let x = PROBE as f32 / (N - 1) as f32;
    let mut value = CurveObject::default();
    value.interpolation[1] = ColorInterpolation::Linear;
    value.channels[1] = vec![
        [0., 0.],
        [x - 0.0001, 0.],
        [x, height],
        [x + 0.0001, 0.],
        [1., 0.],
    ];
    if channel_tables {
        value.channel_luts = std::array::from_fn(|channel| {
            Some(
                (0..N)
                    .map(|i| {
                        if channel == 1 {
                            if i == PROBE { height } else { 0. }
                        } else {
                            i as f32 / (N - 1) as f32
                        }
                    })
                    .collect(),
            )
        });
    }
    value
}
fn fixture(
    precision: ChannelPrecision,
    animated: bool,
    channel_tables: bool,
    offset: u32,
) -> (Project, String) {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project.working_precision = precision;
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "curves")
        .unwrap();
    let parameter = definition
        .params
        .iter()
        .find(|p| p.kind == motion_effects::ParamKind::Curve)
        .unwrap()
        .id
        .clone();
    let mut effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [64.; 2],
    );
    let key = |frame, height| Keyframe {
        auto_bezier: false,
        frame,
        value: source(height, channel_tables),
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    };
    effect.params.get_mut(&parameter).unwrap().curve = Some(CurveTrack {
        value: source(1., channel_tables),
        keys: if animated {
            vec![key(0, 1.), key(10, 0.5)]
        } else {
            vec![]
        },
    });
    let mut layer = Layer::solid(7, "Dense key probe", [64.; 2], [32., 32., 0.], [1.; 4]);
    layer.timeline = Some(motion_core::LayerTimeline {
        in_frame: offset as i32,
        out_frame: offset as i32 + 30,
        offset_frame: offset as i32,
    });
    layer.effects.push(effect);
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    (project, parameter)
}
fn click(project: &Project, parameter: &str, offset: u32, locked: bool) -> Vec<Action> {
    let state = serde_json::json!({"root":"fixture","composition":"comp-main","frame":offset+5,"project":project});
    let mut paint = PaintList::default();
    let sheet = paint.styles();
    let row = Rect::new(0., 0., 360., sheet.metric("effect.row.height", 26.));
    let rail = navigation::rail(&sheet, row);
    let point = [
        rail.min[0] + sheet.metric("icon.slot", 20.) * 1.5,
        (rail.min[1] + rail.max[1]) * 0.5,
    ];
    let input = Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    };
    let mut channel = None;
    let mut actions = Vec::new();
    parameter_navigation(
        &mut channel,
        &mut paint,
        row,
        &state,
        &input,
        7,
        1,
        parameter,
        &project.layers[0].effects[0].params[parameter],
        5.,
        locked,
        &mut actions,
    );
    actions
}
fn commands(actions: Vec<Action>) -> Vec<Command> {
    let mut actions = actions.into_iter();
    let Some(Action::EditBatch(commands)) = actions.next() else {
        panic!("row must emit its existing atomic batch");
    };
    assert!(actions.next().is_none());
    assert_eq!(commands.len(), 2);
    commands
}

#[test]
fn diamond_keeps_sub_preview_detail_in_all_precisions_and_roundtrips_one_history_step() {
    for precision in [
        ChannelPrecision::Unorm8,
        ChannelPrecision::Unorm16,
        ChannelPrecision::Float16,
        ChannelPrecision::Float32,
    ] {
        for animated in [false, true] {
            for offset in [0, 7] {
                let (project, parameter) = fixture(precision, animated, false, offset);
                let original = project.layers[0].effects[0].params[&parameter]
                    .curve
                    .as_ref()
                    .unwrap();
                assert!(original.sample(5.).unwrap().iter().all(|row| row[0] == 0.));
                let batch = commands(click(&project, &parameter, offset, false));
                let Command::Effect {
                    action: EffectAction::SetCurveObject { frame, value, .. },
                    ..
                } = &batch[1]
                else {
                    panic!("missing curve edit");
                };
                assert_eq!(*frame, offset + 5);
                let table = value.sampled_lut.as_ref().unwrap().clone();
                assert_eq!(table.len(), N);
                assert_eq!(table[PROBE][0], if animated { 0.75 } else { 1. });
                assert_eq!(table[PROBE - 1][0], 0.);
                assert_eq!(table[PROBE + 1][0], 0.);
                let mut engine = motion_core::Engine::new(project.clone()).unwrap();
                engine.apply_batch(batch).unwrap();
                let edited = engine.snapshot();
                let curve = edited.layers[0].effects[0].params[&parameter]
                    .curve
                    .as_ref()
                    .unwrap();
                assert_eq!(
                    curve.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
                    if animated { vec![0, 5, 10] } else { vec![5] }
                );
                assert_eq!(curve.sampled_curve(5.).unwrap().high_precision_lut(), table);
                let roundtrip: Project =
                    serde_json::from_slice(&serde_json::to_vec(&edited).unwrap()).unwrap();
                assert_eq!(roundtrip, edited);
                assert!(engine.undo().unwrap());
                assert_eq!(engine.snapshot(), project);
                assert!(!engine.can_undo());
                assert!(engine.redo().unwrap());
                assert_eq!(engine.snapshot(), edited);
            }
        }
    }
}

#[test]
fn dense_channel_tables_freeze_without_duplicate_payload_through_the_actual_worker() {
    let (project, parameter) = fixture(ChannelPrecision::Float32, true, true, 7);
    let batch = commands(click(&project, &parameter, 7, false));
    let Command::Effect {
        action: EffectAction::SetCurveObject { value, .. },
        ..
    } = &batch[1]
    else {
        panic!("missing curve edit");
    };
    assert!(value.channel_luts.iter().all(Option::is_none));
    assert_eq!(value.sampled_lut.as_ref().unwrap()[PROBE][0], 0.75);
    let text = serde_json::to_string(&batch).unwrap();
    assert!(
        text.len() <= crate::engine::REQUEST_LIMIT,
        "request bytes={}",
        text.len()
    );
    let directory = tempfile::tempdir().unwrap();
    motion_core::storage::save(directory.path(), &project).unwrap();
    let engine = crate::engine::Engine::start(
        directory.path().into(),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    let changed = engine.edit(text).unwrap();
    let restored: Project = serde_json::from_value(changed["project"].clone()).unwrap();
    let curve = restored.layers[0].effects[0].params[&parameter]
        .curve
        .as_ref()
        .unwrap();
    assert_eq!(
        curve.keys.iter().map(|k| k.frame).collect::<Vec<_>>(),
        vec![0, 5, 10]
    );
    assert_eq!(
        curve.sampled_curve(5.).unwrap().high_precision_lut()[PROBE][0],
        0.75
    );
    assert_eq!(restored.working_precision, ChannelPrecision::Float32);
}

#[test]
fn locked_or_malformed_curve_diamond_cannot_emit_an_edit() {
    let (mut project, parameter) = fixture(ChannelPrecision::Float16, false, false, 0);
    assert!(click(&project, &parameter, 0, true).is_empty());
    project.layers[0].effects[0]
        .params
        .get_mut(&parameter)
        .unwrap()
        .curve
        .as_mut()
        .unwrap()
        .value
        .sampled_lut = Some(vec![[0.; 4]; 257]);
    assert!(click(&project, &parameter, 0, false).is_empty());
}
