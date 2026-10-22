use super::*;
use crate::engine::Engine;
use motion_core::{Ease, Keyframe, Track};

struct Fixture {
    // Stop the worker before deleting its project directory.
    _engine: Engine,
    _folder: tempfile::TempDir,
    state: Value,
    catalogue: std::sync::Arc<Value>,
    scalar: u64,
    curves: u64,
}

fn fixture() -> Fixture {
    let folder = tempfile::tempdir().unwrap();
    let engine = Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine
        .edit(
            serde_json::to_string(&Command::Add {
                layer: motion_core::Layer::solid(
                    7,
                    "Track cache",
                    [100.; 2],
                    [960., 540., 0.],
                    [1.; 4],
                ),
            })
            .unwrap(),
        )
        .unwrap();
    let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    for name in ["brightness_contrast", "curves"] {
        let package = catalogue["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|package| {
                package["manifest"]["effects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|effect| effect["id"] == name)
            })
            .unwrap();
        engine
            .plugin(
                serde_json::json!({
                    "op": "add", "object": 7,
                    "plugin": package["manifest"]["id"],
                    "version": package["manifest"]["version"],
                    "hash": package["hash"], "effect": name,
                })
                .to_string(),
            )
            .unwrap();
    }
    let state = engine.state().unwrap();
    let effect_id = |name: &str| {
        state["project"]["layers"][0]["effects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|effect| effect["effect"] == name)
            .unwrap()["id"]
            .as_u64()
            .unwrap()
    };
    let scalar = effect_id("brightness_contrast");
    let curves = effect_id("curves");
    Fixture {
        _engine: engine,
        _folder: folder,
        state,
        catalogue: catalogue.into(),
        scalar,
        curves,
    }
}

fn parameter<'a>(state: &'a Value, effect: u64, param: &str) -> &'a Value {
    &state["project"]["layers"][0]["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"].as_u64() == Some(effect))
        .unwrap()["params"][param]
}

fn key<T>(frame: i32, value: T) -> Keyframe<T> {
    Keyframe {
        frame,
        value,
        ease: Ease::Linear,
        curve: None,
        spatial: None,
    }
}

fn seed(state: &mut Value, scalar: u64, curves: u64, version: i32) {
    for (effect, param) in [(scalar, "p0001"), (curves, "p0001"), (curves, "p0002")] {
        let mut value: EffectParam =
            serde_json::from_value(parameter(state, effect, param).clone()).unwrap();
        if effect == scalar {
            let start = [10. + version as f32, 0., 0., 0.];
            let end = [30. + version as f32, 0., 0., 0.];
            value.track = Track::constant(start);
            value.track.keys = vec![key(4 + version, start), key(16 + version, end)];
        } else if param == "p0001" {
            let curve = value.curve.as_mut().unwrap();
            curve.value = motion_core::CurveObject::default();
            curve.value.channels[0].insert(1, [0.5, 0.2 + version as f32 * 0.1]);
            curve.keys = vec![key(4 + version, curve.value.clone())];
        } else {
            // The built-in channel selector is discrete and non-animatable.
            let channel = [1. + version as f32, 0., 0., 0.];
            value.track = Track::constant(channel);
        }
        let instance = state["project"]["layers"][0]["effects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|item| item["id"].as_u64() == Some(effect))
            .unwrap();
        instance["params"][param] = serde_json::to_value(value).unwrap();
    }
}

fn draw(controls: &mut Controls, atlas: &mut TextAtlas, state: &Value, input: &Input) {
    let mut actions = Vec::new();
    let mut y = 0.;
    controls.render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 440., 1200.),
        &mut y,
        &state["project"]["layers"][0],
        state,
        input,
        Locale::En,
        &mut actions,
    );
    assert!(
        actions.is_empty(),
        "passive rendering must not edit the project"
    );
}

fn assert_cached(controls: &mut Controls, state: &Value, scalar: u64, curves: u64) {
    for (effect, param) in [(scalar, "p0001"), (curves, "p0001"), (curves, "p0002")] {
        let cached = controls
            .tracks
            .get_or_try_insert(7, effect, param, || -> Result<EffectParam, ()> {
                panic!("render did not populate effect={effect} param={param}")
            })
            .unwrap();
        assert_eq!(
            serde_json::to_value(cached).unwrap(),
            *parameter(state, effect, param),
            "cached values and keys must match effect={effect} param={param}",
        );
    }
}

#[test]
fn render_reloads_reused_ids_for_each_snapshot_identity_component() {
    let fixture = fixture();
    let mut initial = fixture.state.clone();
    initial["frame"] = 8.into();
    seed(&mut initial, fixture.scalar, fixture.curves, 0);
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for identity in ["root", "composition", "revision"] {
        let mut controls = Controls {
            catalogue: fixture.catalogue.clone(),
            ..Default::default()
        };
        draw(&mut controls, &mut atlas, &initial, &Input::default());
        assert_cached(&mut controls, &initial, fixture.scalar, fixture.curves);

        let mut changed = initial.clone();
        seed(&mut changed, fixture.scalar, fixture.curves, 1);
        changed[identity] = if identity == "revision" {
            (initial[identity].as_u64().unwrap() + 1).into()
        } else {
            format!("{}-other", initial[identity].as_str().unwrap()).into()
        };
        draw(&mut controls, &mut atlas, &changed, &Input::default());
        assert_cached(&mut controls, &changed, fixture.scalar, fixture.curves);
    }
}

#[test]
fn same_snapshot_resamples_the_displayed_scalar_at_the_current_local_frame() {
    let fixture = fixture();
    let mut state = fixture.state.clone();
    seed(&mut state, fixture.scalar, fixture.curves, 0);
    state["project"]["layers"][0]["timeline"]["offset_frame"] = 2.into();
    state["frame"] = 6.into();
    let mut controls = Controls {
        catalogue: fixture.catalogue.clone(),
        ..Default::default()
    };
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    draw(&mut controls, &mut atlas, &state, &Input::default());
    let rect = controls.test_rects[&(7, fixture.scalar, "p0001".into(), 0)];
    let hover = Input {
        mouse: [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ],
        ..Default::default()
    };
    // Check the real numeric row's readout, rather than sampling the cache in
    // the assertion. Frame-only changes must not retain a previous sample.
    for (frame, shown) in [
        (6., "10.000"),
        (12., "20.000"),
        (18., "30.000"),
        (10.5, "17.500"),
    ] {
        state["frame"] = frame.into();
        draw(&mut controls, &mut atlas, &state, &hover);
        assert_eq!(
            controls
                .drag_readout
                .as_ref()
                .map(|(_, text)| text.as_str()),
            Some(shown)
        );
    }
}
