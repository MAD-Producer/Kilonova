use super::*;
use crate::{
    i18n::Locale,
    panels::{animation_controls, effect_controls, Action},
};
use motion_ui::{Input, PaintList, Rect, TextAtlas};
use serde_json::json;

#[test]
fn identity_reader_rejects_missing_or_non_u64_ids_and_accepts_zero_and_full_u64_range() {
    let mut reader = Reader::default();
    let state = json!({});
    for bad in [
        Value::Null,
        json!([]),
        json!("layer"),
        json!({}),
        json!({"id":null}),
        json!({"id":"7"}),
        json!({"id":true}),
        json!({"id":-1}),
        json!({"id":7.0}),
        json!({"id":[]}),
        json!({"id":{}}),
    ] {
        assert_eq!(reader.read(&bad, &state, "identity.fixture"), None);
    }
    assert_eq!(reader.failures, 11);
    for id in [0, 7, u64::MAX] {
        assert_eq!(
            reader.read(&json!({"id":id}), &state, "identity.fixture"),
            Some(id)
        );
        assert_eq!(reader.failures, 0);
        assert_eq!(reader.last_object, Some(id));
    }
    for _ in 0..1000 {
        assert!(reader
            .read(&Value::Null, &state, "identity.fixture")
            .is_none());
    }
    assert_eq!(reader.failures, 60);
    assert_eq!(
        reader.read(&json!({"id":0}), &state, "identity.fixture"),
        Some(0)
    );
    assert_eq!(reader.failures, 0);
}

struct Renderers {
    effects: effect_controls::Controls,
    curves: animation_controls::Controls,
}
impl Renderers {
    fn new() -> Self {
        Self {
            effects: Default::default(),
            curves: Default::default(),
        }
    }
    fn draw(
        &mut self,
        layer: &Value,
        atlas: &mut TextAtlas,
        locale: Locale,
        width: f32,
    ) -> Vec<(PaintList, f32, Vec<Action>)> {
        let state = json!({"frame":0,"document_generation":2,"revision":1});
        let mut result = Vec::new();
        for effect in [true, false] {
            let mut list = PaintList::default();
            let mut y = 17.;
            let mut actions = Vec::new();
            let area = Rect::new(10., 17., width, 480.);
            if effect {
                self.effects.render(
                    &mut list,
                    atlas,
                    area,
                    &mut y,
                    layer,
                    &state,
                    &Input::default(),
                    locale,
                    &mut actions,
                );
            } else {
                self.curves.render(
                    &mut list,
                    atlas,
                    area,
                    &mut y,
                    layer,
                    &json!({}),
                    &json!({}),
                    &state,
                    &Input::default(),
                    locale,
                    &mut actions,
                    None,
                );
            }
            result.push((list, y, actions));
        }
        result
    }
}

#[test]
fn actual_property_renderers_skip_malformed_layers_without_rows_actions_or_panics_and_recover() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        for width in [220., 420.] {
            let mut controls = Renderers::new();
            let healthy = json!({"id":7,"effects":[],"locked":false});
            let healthy_draw = controls.draw(&healthy, &mut atlas, locale, width);
            let snapshot = |draw: &[(PaintList, f32, Vec<Action>)]| {
                draw.iter()
                    .map(|(paint, y, _)| {
                        format!(
                            "{:?}",
                            (&paint.shapes, &paint.colors, &paint.glyphs, &paint.order, y)
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert!(healthy_draw
                .iter()
                .all(|(paint, y, actions)| !paint.glyphs.is_empty()
                    && *y > 17.
                    && actions.is_empty()));
            let expected = snapshot(&healthy_draw);
            for bad in [
                Value::Null,
                json!([]),
                json!({}),
                json!({"id":null}),
                json!({"id":"7"}),
                json!({"id":false}),
                json!({"id":-7}),
                json!({"id":7.5}),
                json!({"id":{}}),
            ] {
                let original = bad.clone();
                for (paint, y, actions) in controls.draw(&bad, &mut atlas, locale, width) {
                    assert_eq!(y, 17.);
                    assert!(
                        paint.shapes.is_empty()
                            && paint.glyphs.is_empty()
                            && paint.order.is_empty()
                    );
                    assert!(actions.is_empty());
                }
                assert_eq!(bad, original);
                assert_eq!(
                    snapshot(&controls.draw(&healthy, &mut atlas, locale, width)),
                    expected
                );
            }
        }
    }
}

#[test]
fn actual_render_failures_are_bounded_reset_and_keep_default_file_error_stderr_and_stdout_isolation(
) {
    const CHILD: &str = "MOTION_PANEL_LAYER_ID_CHILD";
    const TEST: &str = "panels::layer_identity::tests::actual_render_failures_are_bounded_reset_and_keep_default_file_error_stderr_and_stdout_isolation";
    const SITES: [&str; 2] = ["effects.layer_id", "curve.controls.layer_id"];
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let _logger = crate::logging::init(root.clone()).unwrap();
        let path = root.join("motion-studio.log");
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut controls = Renderers::new();
        let healthy = json!({"id":7,"effects":[]});
        let read = || {
            crate::logging::flush();
            std::fs::read_to_string(&path).unwrap()
        };
        let faults = |text: &str| {
            text.lines()
                .filter(|line| {
                    SITES
                        .iter()
                        .any(|site| line.contains(&format!("site={site} ")))
                        && line.contains("ERROR")
                })
                .count()
        };
        for _ in 0..120 {
            controls.draw(&healthy, &mut atlas, Locale::En, 420.);
        }
        assert_eq!(faults(&read()), 0);
        let before_healthy = read();
        for _ in 0..120 {
            controls.draw(&healthy, &mut atlas, Locale::En, 420.);
        }
        assert_eq!(read(), before_healthy, "healthy frames must not add log IO");
        let secret = format!("do-not-log-user-text-{}", "private".repeat(1000));
        for cycle in 0..2 {
            let broken = if cycle == 0 {
                json!({"id":secret})
            } else {
                json!([])
            };
            for _ in 0..59 {
                controls.draw(&broken, &mut atlas, Locale::En, 420.);
            }
            assert_eq!(
                faults(&read()),
                cycle * 2,
                "must not escalate before occurrence 60"
            );
            controls.draw(&broken, &mut atlas, Locale::En, 420.);
            for _ in 0..120 {
                controls.draw(&broken, &mut atlas, Locale::En, 420.);
            }
            assert_eq!(
                faults(&read()),
                (cycle + 1) * 2,
                "must suppress sustained repeats"
            );
            controls.draw(&healthy, &mut atlas, Locale::En, 420.);
        }
        let mut other = Renderers::new();
        for _ in 0..60 {
            other.draw(&json!({"id":null}), &mut atlas, Locale::En, 420.);
        }
        let text = read();
        assert_eq!(
            faults(&text),
            6,
            "a fresh renderer instance must have independent counters"
        );
        assert!(!text.contains("do-not-log-user-text") && !text.contains("privateprivate"));
        for line in text.lines().filter(|line| {
            SITES
                .iter()
                .any(|site| line.contains(&format!("site={site} ")))
        }) {
            assert!(line.len() < 512);
            assert!(
                line.contains("operation=render phase=identity")
                    && line.contains("generation=Some(2) revision=Some(1)")
                    && line.contains("cause=missing or non-u64 layer ID")
            );
        }
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("MOTION_PANEL_ID_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| temp.path().to_owned());
    for level in ["info", "debug"] {
        let root = evidence.join(level);
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, &root)
            .env("MOTION_LOG", level)
            .output()
            .unwrap();
        std::fs::write(root.join("child.stdout"), &output.stdout).unwrap();
        std::fs::write(root.join("child.stderr"), &output.stderr).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        let file = std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in SITES {
            let marker = format!("site={site} ");
            assert_eq!(
                stderr.lines().filter(|line| line.contains(&marker)).count(),
                3
            );
            assert!(stderr
                .lines()
                .filter(|line| line.contains(&marker))
                .all(|line| line.contains("ERROR")));
            assert!(!String::from_utf8_lossy(&output.stdout).contains(&marker));
            assert_eq!(
                file.lines()
                    .filter(|line| line.contains(&marker)
                        && line.contains("DEBUG")
                        && line.contains("count=1 "))
                    .count(),
                if level == "debug" { 3 } else { 0 }
            );
            assert_eq!(
                file.lines()
                    .filter(|line| line.contains(&marker)
                        && line.contains("ERROR")
                        && line.contains("count=60 "))
                    .count(),
                3
            );
        }
    }
}
