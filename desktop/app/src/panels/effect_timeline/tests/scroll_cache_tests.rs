use super::*;

#[test]
fn group_boundaries_keep_different_row_layouts_from_sharing_the_scroll_cache() {
    for locale in [Locale::En, Locale::Zh] {
        for width in [900., 1600.] {
            for height in [180., 240.] {
                for reversed in [false, true] {
                    let (original, catalogue) = fixture();
                    let mut project = original.project().clone();
                    let child = Project::new(100, 100, 30, 180).unwrap();
                    project.compositions.push(motion_core::Composition {
                        id: "comp-source".into(),
                        name: "Nested source".into(),
                        width: child.width,
                        height: child.height,
                        fps: child.fps,
                        frames: child.frames,
                        background: child.background,
                        camera: child.camera,
                        layers: child.layers,
                        expressions: child.expressions,
                    });
                    project.layers[0].content = motion_core::Content::Composition {
                        clip: motion_core::CompositionClip::new("comp-source".into()),
                    };
                    let engine = Engine::new(project).unwrap();
                    let before = engine.project().clone();
                    let state = state(&engine);
                    let mut panels = setup(&catalogue, locale);
                    let mut atlas =
                        TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
                    let area = Rect::new(0., 0., width, height);
                    let configure = |panels: &mut Panels, expanded: bool| {
                        panels.timeline.expanded_effects.clear();
                        panels.timeline.collapsed_audio.clear();
                        if expanded {
                            panels.timeline.expanded_effects.insert(7);
                        } else {
                            panels.timeline.collapsed_audio.insert(7);
                        }
                    };
                    let draw = |panels: &mut Panels, atlas: &mut TextAtlas| {
                        let mut actions = Vec::new();
                        timeline::render(
                            &mut PaintList::default(),
                            atlas,
                            area,
                            panels,
                            &state,
                            &Input::default(),
                            &mut actions,
                        );
                        assert!(actions.is_empty());
                    };
                    configure(&mut panels, !reversed);
                    draw(&mut panels, &mut atlas);
                    let initial = panels.timeline.scroll_extent_cache.unwrap();
                    configure(&mut panels, reversed);
                    panels.timeline.scroll = 10000.;
                    draw(&mut panels, &mut atlas);
                    let transition = panels.timeline.scroll_extent_cache.unwrap();
                    let transition_scroll = panels.timeline.scroll;
                    panels.timeline.scroll_extent_cache = None;
                    draw(&mut panels, &mut atlas);
                    let fresh = panels.timeline.scroll_extent_cache.unwrap();
                    println!("locale={locale:?} size={width}x{height} reversed={reversed} initial={initial:?} transition={transition:?} fresh={fresh:?}");
                    assert_ne!(
                        initial.1, fresh.1,
                        "fixture must have different row extents"
                    );
                    assert_eq!(
                        transition.1, fresh.1,
                        "cached extent must match a fresh projection"
                    );
                    assert_ne!(
                        initial.0, transition.0,
                        "different layout groups require distinct keys"
                    );
                    assert!(transition_scroll <= fresh.1);
                    assert_eq!(engine.project(), &before);
                }
            }
        }
    }
}
