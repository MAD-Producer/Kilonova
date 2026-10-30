use super::*;
use crate::panels::{
    self,
    animation_controls::{SelectedTrack, TrackSelection},
};

#[test]
fn optional_row_class_uses_required_framework_selection_in_custom_themes() {
    let mut document: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../ui/styles/dark.json"
    )))
    .unwrap();
    let classes = document["styles"].as_object_mut().unwrap();
    // This custom theme omits optional property rows, but shared menus now
    // require their own row class. Give it a required framework ancestor so
    // pruning optional row descendants does not invalidate the theme itself.
    classes.get_mut("context_menu.row").unwrap()["extends"] = "widget".into();
    let mut removed = vec!["row".to_owned()];
    classes.remove("row");
    loop {
        let children: Vec<_> = classes
            .iter()
            .filter(|(_, value)| {
                value["extends"]
                    .as_str()
                    .is_some_and(|parent| removed.iter().any(|name| name == parent))
            })
            .map(|(name, _)| name.clone())
            .collect();
        if children.is_empty() {
            break;
        }
        for child in children {
            classes.remove(&child);
            removed.push(child);
        }
    }
    assert!(!classes.contains_key("row"));
    assert!(!classes.contains_key("timeline.row"));
    assert!(classes.contains_key("context_menu.row"));
    let theme = std::sync::Arc::new(
        motion_ui::style::StyleSheet::from_json(&serde_json::to_vec(&document).unwrap()).unwrap(),
    );
    let expected = theme
        .resolve("selection", States::default())
        .unwrap()
        .background;
    let mut paint = PaintList {
        stylesheet: Some(theme),
        ..Default::default()
    };
    panels::animation_controls::highlight_row(&mut paint, Rect::new(0., 0., 360., 22.), true);
    assert!(paint.colors.contains(&expected));
}

fn draw(
    controls: &mut Panels,
    state: &Value,
    panel: Panel,
    floating: bool,
    input: &Input,
    atlas: &mut TextAtlas,
) -> (PaintList, Vec<Action>) {
    let mut paint = PaintList::default();
    let actions = if floating {
        panels::floating_chrome(
            &mut paint,
            atlas,
            panel,
            [1000., 800.],
            controls,
            state,
            input,
        )
    } else {
        panels::chrome(
            &mut paint,
            atlas,
            [1000., 800.],
            &dock::Node::dock(panel.dock()),
            controls,
            state,
            input,
        )
    };
    (paint, actions)
}

#[test]
fn real_property_names_and_values_share_selection_and_theme_highlight_in_all_hosts() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for camera in [false, true] {
        let authored = if camera {
            let mut e = Engine::new(Project::new(640, 480, 30, 120).unwrap()).unwrap();
            e.apply(Command::CreateCamera).unwrap();
            e
        } else {
            let mut project = fixture().project().clone();
            project.layers[0]
                .masks
                .push(motion_core::masks::LayerMask::new(
                    1,
                    motion_core::vector::VectorPath {
                        tangents: Default::default(), id: 1,
                        closed: true,
                        nodes: [[0., 0.], [80., 0.], [40., 80.]]
                            .into_iter()
                            .enumerate()
                            .map(|(i, point)| motion_core::vector::PathNode {
                                id: i as u64 + 1,
                                geometry: motion_core::Track::constant([
                                    point[0], point[1], 0., 0., 0., 0.,
                                ]),
                            })
                            .collect(),
                    },
                ));
            Engine::new(project).unwrap()
        };
        let object = authored.project().layers[0].id;
        let root = tempfile::tempdir().unwrap();
        motion_core::storage::save(root.path(), authored.project()).unwrap();
        let host = crate::engine::Engine::start(
            root.path().into(),
            motion_desktop_media::platform(),
            8 << 30,
        )
        .unwrap();
        host.seek(12.).unwrap();
        let state = host.state().unwrap();
        let properties = if camera {
            vec![Property::Position, Property::Target, Property::Fov]
        } else {
            vec![Property::Anchor, Property::Position, Property::Opacity]
        };
        for locale in [Locale::En, Locale::Zh] {
            for panel in [Panel::Properties, Panel::Timeline] {
                for floating in [false, true] {
                    for property in properties.iter().copied() {
                        for number in [false, true] {
                            let mut controls = Panels {
                                locale,
                                focused: Some(panel),
                                ..Default::default()
                            };
                            controls.select_only(Some(object));
                            controls.timeline.expanded.insert(object);
                            let track = SelectedTrack {
                                object,
                                property,
                                axis: None,
                            };
                            controls
                                .animation_controls
                                .begin_row_probe(object, property, None);
                            draw(
                                &mut controls,
                                &state,
                                panel,
                                floating,
                                &Input::default(),
                                &mut atlas,
                            );
                            let rect = controls
                                .animation_controls
                                .probed_row(number.then_some(0))
                                .expect("actual target row must be painted in this host");
                            let point = [
                                (rect.min[0] + rect.max[0]) * 0.5,
                                (rect.min[1] + rect.max[1]) * 0.5,
                            ];
                            let (_, actions) = draw(
                                &mut controls,
                                &state,
                                panel,
                                floating,
                                &Input {
                                    mouse: point,
                                    pressed: Some((MouseButton::Left, point)),
                                    ..Default::default()
                                },
                                &mut atlas,
                            );
                            assert_eq!(mutations(&actions), 0, "camera={camera} {locale:?} {panel:?} float={floating} {property:?} number={number}: {actions:?}");
                            assert_eq!(
                                controls
                                    .animation_controls
                                    .track_selection(&state, Some(object)),
                                TrackSelection::Current(track)
                            );
                            let (paint, _) = draw(
                                &mut controls,
                                &state,
                                panel,
                                floating,
                                &Input::default(),
                                &mut atlas,
                            );
                            let selected = paint
                                .styles()
                                .resolve("row", States::default().with(State::Selected))
                                .unwrap()
                                .background;
                            assert!(paint.shapes.iter().zip(&paint.colors).any(|(shape, color)| {
                                *color == selected && matches!(shape, motion_ui::paint::Shape::Procedural { area, .. } if area.contains(point))
                            }), "selected row must cover the actual input target: camera={camera} {panel:?} {property:?} number={number}");
                            draw(
                                &mut controls,
                                &state,
                                panel,
                                floating,
                                &Input {
                                    keys: vec![(Key::Escape, Default::default())],
                                    ..Default::default()
                                },
                                &mut atlas,
                            );
                            assert_eq!(host.state().unwrap()["project"], state["project"]);
                        }
                    }
                }
            }
        }
    }
}
