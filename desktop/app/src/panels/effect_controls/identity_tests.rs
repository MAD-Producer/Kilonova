use super::*;

fn controls() -> Controls {
    Controls {
        catalogue: serde_json::json!({"packages":[{"hash":"fixture-hash","manifest":{
            "id":"local.fixture.identity","version":"1","effects":[{
                "id":"fixture","name":"标识测试","english_name":"Identity fixture","params":[]
            }]
        }}]})
        .into(),
        ..Default::default()
    }
}

fn valid_effect() -> Value {
    serde_json::json!({"id":11,"plugin":"local.fixture.identity","version":"1",
        "hash":"fixture-hash","effect":"fixture","enabled":true,"params":{}})
}

fn malformed_effects() -> Vec<Value> {
    let mut values = vec![Value::Null];
    let mut missing = valid_effect();
    missing.as_object_mut().unwrap().remove("id");
    values.push(missing);
    for id in [
        Value::Null,
        serde_json::json!("11"),
        serde_json::json!(-1),
        serde_json::json!(11.0),
        serde_json::json!(false),
        serde_json::json!([]),
    ] {
        let mut effect = valid_effect();
        effect["id"] = id;
        values.push(effect);
    }
    values
}

fn layer(effects: Vec<Value>) -> Value {
    serde_json::json!({"id":7,"name":"Identity fixture","effects":effects})
}

fn draw(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    layer: &Value,
    locale: Locale,
    input: &Input,
) -> (PaintList, f32, Vec<Action>) {
    let mut paint = PaintList::default();
    let mut y = 20.;
    let mut actions = Vec::new();
    controls.render(
        &mut paint,
        atlas,
        Rect::new(20., 20., 360., 560.),
        &mut y,
        layer,
        &serde_json::json!({"frame":0,"revision":0}),
        input,
        locale,
        &mut actions,
    );
    (paint, y, actions)
}

fn snapshot(paint: PaintList, y: f32) -> String {
    format!(
        "{:?}",
        (paint.shapes, paint.colors, paint.glyphs, paint.order, y)
    )
}

#[test]
fn malformed_effect_ids_do_not_reserve_rows_or_change_the_valid_effect_paint() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let healthy = layer(vec![valid_effect()]);
        let (paint, y, actions) = draw(
            &mut controls(),
            &mut atlas,
            &healthy,
            locale,
            &Input::default(),
        );
        assert!(actions.is_empty() && !paint.glyphs.is_empty());
        let styles = paint.styles();
        let height = controls().height(&healthy, &styles, locale, 360.);
        let expected = snapshot(paint, y);
        for malformed in malformed_effects() {
            let mixed = layer(vec![malformed, valid_effect()]);
            let original = mixed.clone();
            let mut controls = controls();
            assert_eq!(controls.height(&mixed, &styles, locale, 360.), height);
            let (paint, y, actions) =
                draw(&mut controls, &mut atlas, &mixed, locale, &Input::default());
            assert!(actions.is_empty());
            assert_eq!(snapshot(paint, y), expected);
            assert_eq!(mixed, original);
        }
    }
}

#[test]
fn surviving_effect_context_keeps_its_identity_and_original_array_index() {
    let mut controls = controls();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let mixed = layer(vec![Value::Null, valid_effect()]);
    draw(
        &mut controls,
        &mut atlas,
        &mixed,
        Locale::En,
        &Input::default(),
    );
    let title = controls.test_rects[&(7, 11, "__title".into(), 0)];
    let point = [
        (title.min[0] + title.max[0]) * 0.5,
        (title.min[1] + title.max[1]) * 0.5,
    ];
    let (_, _, actions) = draw(
        &mut controls,
        &mut atlas,
        &mixed,
        Locale::En,
        &Input {
            mouse: point,
            pressed: Some((MouseButton::Right, point)),
            ..Default::default()
        },
    );
    assert!(actions.is_empty());
    assert_eq!(
        controls.context_menu.as_ref().unwrap().instance,
        Some((11, 1))
    );
}

#[test]
fn real_invalid_id_diagnostics_are_bounded_visible_at_default_level_and_reset_on_recovery() {
    const CHILD: &str = "MOTION_EFFECT_INSTANCE_ID_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let path = root.join("motion-studio.log");
        let mut controls = controls();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let _logger = crate::logging::init(root.clone()).unwrap();
        crate::logging::flush();
        let startup = std::fs::read_to_string(&path).unwrap();
        assert_eq!(startup.lines().count(), 1);
        assert!(startup.contains("INFO") && startup.contains("logging initialized level=INFO"));
        let healthy = layer(vec![valid_effect()]);
        let broken = layer(vec![Value::Null, valid_effect()]);
        let empty = layer(Vec::new());
        let mut render = |value: &Value| {
            let (_, _, actions) = draw(
                &mut controls,
                &mut atlas,
                value,
                Locale::En,
                &Input::default(),
            );
            assert!(actions.is_empty());
        };
        for _ in 0..120 {
            render(&healthy);
        }
        crate::logging::flush();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), startup);
        for cycle in 0..3 {
            for _ in 0..59 {
                render(&broken);
            }
            crate::logging::flush();
            let before = std::fs::read_to_string(&path).unwrap();
            assert_eq!(before.strip_prefix(&startup).unwrap().lines().count(), cycle);
            render(&broken);
            for _ in 0..120 {
                render(&broken);
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let records = text.strip_prefix(&startup).unwrap().lines().collect::<Vec<_>>();
            assert_eq!(records.len(), cycle + 1);
            assert!(records.iter().all(|line| line.contains("ERROR")
                && line.contains("site=effects.instance_id count=60")
                && line.contains("object=7 effect_index=0 phase=identity")
                && line.contains("cause=missing or non-u64 effect instance ID")));
            render(if cycle == 0 { &healthy } else { &empty });
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::effect_controls::identity_tests::real_invalid_id_diagnostics_are_bounded_visible_at_default_level_and_reset_on_recovery"])
        .env_remove("MOTION_LOG").env(CHILD,temp.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        stderr
            .lines()
            .filter(|line| line.contains("site=effects.instance_id"))
            .count(),
        3
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("effects.instance_id"));
}
