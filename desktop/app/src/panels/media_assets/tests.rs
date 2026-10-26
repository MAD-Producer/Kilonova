use super::*;
use motion_core::{Asset, Content, Layer, Project};
use motion_ui::input::Modifiers;

fn state() -> Value {
    let mut project = Project::new(640, 360, 30, 120).unwrap();
    for id in 1..=3 {
        project.assets.push(Asset {
            id,
            path: format!("assets/source-{id}.png"),
            width: 32,
            height: 32,
        });
    }
    let mut layer = Layer::solid(7, "参考图层 Reference", [32.; 2], [100., 100., 0.], [1.; 4]);
    layer.content = Content::Image { asset: 1 };
    project.layers.push(layer);
    serde_json::json!({"root":"fixture-project","document_generation":17,"revision":3,"project":project,"frame":0})
}
fn point(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}
fn press(rect: Rect, button: MouseButton) -> Input {
    let point = point(rect);
    Input {
        mouse: point,
        pressed: Some((button, point)),
        ..Default::default()
    }
}
fn draw(panels: &mut Panels, state: &Value, owner: Option<Panel>, input: Input) -> Vec<Action> {
    let mut paint = PaintList::default();
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    if let Some(panel) = owner {
        floating_chrome(
            &mut paint,
            &mut atlas,
            panel,
            [600., 420.],
            panels,
            state,
            &input,
        )
    } else {
        chrome(
            &mut paint,
            &mut atlas,
            [900., 600.],
            &Node::dock(Panel::Project.dock()),
            panels,
            state,
            &input,
        )
    }
}
fn open(panels: &mut Panels, state: &Value, owner: Option<Panel>, id: u64) {
    assert!(draw(panels, state, owner, Input::default()).is_empty());
    let row = panels
        .media_assets
        .rows
        .iter()
        .find(|(target, _)| target.id == id)
        .unwrap()
        .1;
    assert!(draw(panels, state, owner, press(row, MouseButton::Right)).is_empty());
    assert!(panels.media_assets.open());
}

#[test]
fn shared_asset_menus_preserve_orphan_identity_and_localized_command_semantics() {
    for locale in [Locale::Zh, Locale::En] {
        for owner in [None, Some(Panel::Project), Some(Panel::Media)] {
            let state = state();
            let mut panels = Panels::default();
            panels.locale = locale;
            panels.select_only(Some(7));
            open(&mut panels, &state, owner, 2);
            assert_eq!(
                panels.media_assets.selected,
                Some(AssetRef {
                    kind: AssetKind::Image,
                    id: 2
                })
            );
            assert_eq!(
                panels.selected,
                Some(7),
                "asset selection must not invent a layer"
            );
            let remove = panels.media_assets.menu_rows[3];
            let actions = draw(&mut panels, &state, owner, press(remove, MouseButton::Left));
            assert!(
                matches!(actions.as_slice(),[Action::MediaAsset {source,intent:Intent::Remove}] if source.target.id==2 && source.revision==3 && source.generation==17),
                "{actions:?}"
            );
            assert!(!panels.media_assets.open());
            open(&mut panels, &state, owner, 1);
            let remove = panels.media_assets.menu_rows[3];
            assert!(draw(&mut panels, &state, owner, press(remove, MouseButton::Left)).is_empty());
            assert!(
                panels.media_assets.open(),
                "referenced removal remains disabled"
            );
            let usages = panels.media_assets.menu_rows[2];
            assert!(draw(&mut panels, &state, owner, press(usages, MouseButton::Left)).is_empty());
            draw(&mut panels, &state, owner, Input::default());
            let usage = panels.media_assets.menu_rows[1];
            let actions = draw(&mut panels, &state, owner, press(usage, MouseButton::Left));
            assert!(
                matches!(actions.as_slice(),[Action::MediaAsset {intent:Intent::RevealUsage(usage),..}] if usage.object==7),
                "{actions:?}"
            );
        }
    }
}

#[test]
fn asset_overlay_owns_keys_closing_frame_and_cross_window_pointer_input() {
    for owner in [None, Some(Panel::Project), Some(Panel::Media)] {
        let state = state();
        let mut panels = Panels::default();
        panels.select_only(Some(7));
        open(&mut panels, &state, owner, 2);
        for key in [Key::Space, Key::Delete, Key::F9] {
            assert!(draw(
                &mut panels,
                &state,
                owner,
                Input {
                    keys: vec![(key, Modifiers::default())],
                    ..Default::default()
                }
            )
            .is_empty());
        }
        assert!(panels.media_assets.open());
        for input in [
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                composing: true,
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Escape, Modifiers::default())],
                ime_consumed: true,
                ..Default::default()
            },
            Input {
                keys: vec![(Key::Enter, Modifiers::default())],
                key_repeats: vec![true],
                ..Default::default()
            },
        ] {
            assert!(draw(&mut panels, &state, owner, input).is_empty());
            assert!(panels.media_assets.open());
        }
        let other = if owner.is_none() {
            Some(Panel::Media)
        } else {
            None
        };
        assert!(draw(&mut panels, &state, other, Input::default()).is_empty());
        assert!(
            panels.media_assets.open(),
            "neutral repaint in another window must retain the popup"
        );
        assert!(draw(
            &mut panels,
            &state,
            other,
            Input {
                mouse: [2.; 2],
                pressed: Some((MouseButton::Left, [2.; 2])),
                ..Default::default()
            }
        )
        .is_empty());
        assert!(!panels.media_assets.open());
        open(&mut panels, &state, owner, 2);
        assert!(draw(
            &mut panels,
            &state,
            owner,
            Input {
                keys: vec![(Key::Escape, Modifiers::default())],
                ..Default::default()
            }
        )
        .is_empty());
        assert!(!panels.media_assets.open());
        assert_eq!(panels.selected, Some(7));
    }
}

#[test]
fn revision_or_document_change_rejects_menu_input_and_resets_only_document_selection() {
    let mut state = state();
    let mut panels = Panels::default();
    open(&mut panels, &state, None, 2);
    let remove = panels.media_assets.menu_rows[3];
    state["revision"] = serde_json::json!(4);
    assert!(draw(&mut panels, &state, None, press(remove, MouseButton::Left)).is_empty());
    assert!(!panels.media_assets.open());
    assert_eq!(panels.media_assets.selected.unwrap().id, 2);
    open(&mut panels, &state, None, 2);
    state["document_generation"] = serde_json::json!(18);
    draw(&mut panels, &state, None, Input::default());
    assert!(!panels.media_assets.open());
    assert_eq!(panels.media_assets.selected, None);
}

#[cfg(all(target_os = "windows"))]
mod native;
