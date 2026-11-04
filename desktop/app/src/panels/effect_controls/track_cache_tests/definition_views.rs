use super::*;

fn snapshot(
    controls: &mut Controls,
    atlas: &mut TextAtlas,
    state: &Value,
    locale: Locale,
    width: f32,
) -> String {
    let layer = &state["project"]["layers"][0];
    let mut paint = PaintList::default();
    let mut y = 17.;
    let mut actions = Vec::new();
    let height = controls.height(layer, &paint.styles(), locale, width);
    controls.render(
        &mut paint,
        atlas,
        Rect::new(10., 17., width, 1200.),
        &mut y,
        layer,
        state,
        &Input {
            mouse: [-100., -100.],
            ..Default::default()
        },
        locale,
        &mut actions,
    );
    assert!(actions.is_empty());
    assert!(!paint.glyphs.is_empty());
    let hits: std::collections::BTreeMap<_, _> = controls.test_rects.iter().collect();
    format!(
        "{:?}",
        (
            paint.shapes,
            paint.colors,
            paint.glyphs,
            paint.order,
            y,
            height,
            hits
        )
    )
}

#[test]
fn actual_definition_views_preserve_full_draw_height_hits_and_refresh_current_catalogue() {
    let fixture = fixture();
    let mut state = fixture.state.clone();
    state["frame"] = 8.into();
    seed(&mut state, fixture.scalar, fixture.curves, 0);
    let original_state = state.clone();
    let mut evidence = std::collections::BTreeMap::new();
    for locale in [Locale::En, Locale::Zh] {
        for width in [220., 420.] {
            for collapsed in [false, true] {
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let mut controls = Controls {
                    catalogue: fixture.catalogue.clone(),
                    ..Default::default()
                };
                if collapsed {
                    controls.collapsed_effects.insert(fixture.scalar);
                }
                snapshot(&mut controls, &mut atlas, &state, locale, width);
                let old = snapshot(&mut controls, &mut atlas, &state, locale, width);
                assert_eq!(
                    snapshot(&mut controls, &mut atlas, &state, locale, width),
                    old
                );
                let mut replacement = (*fixture.catalogue).clone();
                for package in replacement["packages"].as_array_mut().unwrap() {
                    for effect in package["manifest"]["effects"].as_array_mut().unwrap() {
                        effect["name"] = "替换定义".into();
                        effect["english_name"] = "Replacement definition".into();
                    }
                }
                let replacement = std::sync::Arc::new(replacement);
                let generation = std::sync::Arc::downgrade(&replacement);
                controls.refresh_catalogue(replacement);
                let changed = snapshot(&mut controls, &mut atlas, &state, locale, width);
                assert_ne!(
                    changed, old,
                    "replacement must reach the actual rendered text"
                );
                controls.refresh_catalogue(fixture.catalogue.clone());
                assert!(
                    generation.upgrade().is_none(),
                    "draws must not retain catalogue history"
                );
                assert_eq!(
                    snapshot(&mut controls, &mut atlas, &state, locale, width),
                    old
                );
                assert_eq!(state, original_state);
                evidence.insert(format!("{locale:?}-{width}-{collapsed}"), (old, changed));
            }
        }
    }
    evidence.extend(timeline_snapshots());
    if let Some(path) = std::env::var_os("MOTION_DEFINITION_VIEW_EVIDENCE") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

fn timeline_snapshots() -> std::collections::BTreeMap<String, (String, String)> {
    use crate::panels::{effect_timeline::tests, types::Panels};
    use motion_ui::dock::{Node, Panel};
    let (engine, catalogue) = tests::fixture();
    let state = tests::state(&engine);
    let mut evidence = std::collections::BTreeMap::new();
    for locale in [Locale::En, Locale::Zh] {
        for width in [900., 1400.] {
            for floating in [false, true] {
                let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                let mut panels = tests::setup(&catalogue, locale);
                let mut draw = |panels: &mut Panels| {
                    let mut paint = PaintList::default();
                    let input = Input {
                        mouse: [-100., -100.],
                        ..Default::default()
                    };
                    let actions = if floating {
                        crate::panels::floating_chrome(
                            &mut paint,
                            &mut atlas,
                            Panel::Timeline,
                            [width, 850.],
                            panels,
                            &state,
                            &input,
                        )
                    } else {
                        crate::panels::chrome(
                            &mut paint,
                            &mut atlas,
                            [width, 850.],
                            &Node::dock(Panel::Timeline.dock()),
                            panels,
                            &state,
                            &input,
                        )
                    };
                    assert!(actions.is_empty());
                    assert!(!paint.glyphs.is_empty());
                    let hits: std::collections::BTreeMap<_, _> =
                        panels.effect_timeline.rects.iter().collect();
                    format!(
                        "{:?}",
                        (paint.shapes, paint.colors, paint.glyphs, paint.order, hits)
                    )
                };
                draw(&mut panels);
                let old = draw(&mut panels);
                assert_eq!(draw(&mut panels), old);
                let mut replacement = catalogue.clone();
                for package in replacement["packages"].as_array_mut().unwrap() {
                    for effect in package["manifest"]["effects"].as_array_mut().unwrap() {
                        effect["name"] = "替换定义".into();
                        effect["english_name"] = "Replacement definition".into();
                    }
                }
                panels.effect_controls.refresh_catalogue(replacement.into());
                let changed = draw(&mut panels);
                assert_ne!(changed, old);
                panels
                    .effect_controls
                    .refresh_catalogue(catalogue.clone().into());
                assert_eq!(draw(&mut panels), old);
                assert_eq!(tests::state(&engine), state);
                evidence.insert(
                    format!("timeline-{locale:?}-{width}-{floating}"),
                    (old, changed),
                );
            }
        }
    }
    evidence
}
