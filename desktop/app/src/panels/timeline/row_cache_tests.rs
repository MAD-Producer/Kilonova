use super::*;
use motion_core::{Engine, Layer, Project};
use std::sync::Arc;

fn draw(panels: &mut Panels, atlas: &mut TextAtlas, state: &Value, area: Rect) -> PaintList {
    let mut paint = PaintList::default();
    let mut actions = Vec::new();
    render(
        &mut paint,
        atlas,
        area,
        panels,
        state,
        &Input::default(),
        &mut actions,
    );
    assert!(actions.is_empty());
    paint
}

#[test]
fn warm_and_rebuilt_rows_paint_identically_and_identity_changes_refresh_names() {
    let mut project = Project::new(640, 480, 30, 120).unwrap();
    project.layers = vec![
        Layer::solid(1, "Title", [100.; 2], [50., 60., 0.], [1.; 4]),
        Layer::solid(2, "Backdrop", [200.; 2], [100., 120., 0.], [1.; 4]),
    ];
    let mut engine = Engine::new(project).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for [width, height] in [[900., 600.], [1600., 1000.]] {
            let mut panels = Panels {
                locale,
                ..Default::default()
            };
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let area = Rect::new(0., 0., width, height);
            let mut state = serde_json::json!({
                "root":"project-a", "composition":"main", "revision":1,
                "frame":0, "project":engine.project(),
                "timeline_layers":engine.project().timeline_layers(0.).unwrap(),
            });
            draw(&mut panels, &mut atlas, &state, area);
            let cached = panels.timeline.rows_cache.as_ref().unwrap().1.clone();
            assert_eq!(cached.iter().map(|r| r.id).collect::<Vec<_>>(), [2, 1]);
            let warm = draw(&mut panels, &mut atlas, &state, area);
            assert!(Arc::ptr_eq(
                &cached,
                &panels.timeline.rows_cache.as_ref().unwrap().1
            ));
            panels.timeline.rows_cache = None;
            let rebuilt = draw(&mut panels, &mut atlas, &state, area);
            assert!(!Arc::ptr_eq(
                &cached,
                &panels.timeline.rows_cache.as_ref().unwrap().1
            ));
            assert!(!warm.glyphs.is_empty());
            assert_eq!(warm.order, rebuilt.order);
            assert_eq!(
                format!("{:?}", warm.shapes),
                format!("{:?}", rebuilt.shapes)
            );
            assert_eq!(
                format!("{:?}", warm.colors),
                format!("{:?}", rebuilt.colors)
            );
            assert_eq!(
                format!("{:?}", warm.glyphs),
                format!("{:?}", rebuilt.glyphs)
            );

            for (field, identity, name) in [
                ("root", serde_json::json!("project-b"), "Root refresh"),
                (
                    "composition",
                    serde_json::json!("other"),
                    "Composition refresh",
                ),
                ("revision", serde_json::json!(2), "Revision refresh"),
            ] {
                engine
                    .apply(Command::Rename {
                        object: 1,
                        name: name.into(),
                    })
                    .unwrap();
                state[field] = identity;
                state["project"] = serde_json::to_value(engine.project()).unwrap();
                state["timeline_layers"] =
                    serde_json::to_value(engine.project().timeline_layers(0.).unwrap()).unwrap();
                let old = panels.timeline.rows_cache.as_ref().unwrap().1.clone();
                draw(&mut panels, &mut atlas, &state, area);
                let current = &panels.timeline.rows_cache.as_ref().unwrap().1;
                assert!(!Arc::ptr_eq(&old, current));
                assert_eq!(current.iter().find(|r| r.id == 1).unwrap().name, name);
            }
        }
    }
}
