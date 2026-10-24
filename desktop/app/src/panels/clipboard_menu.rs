use super::{Action, Locale, Panels};
use crate::{
    layer_clipboard::controller::Intent,
    shortcuts::{Command, Scope},
};
use serde_json::Value;

pub(super) fn entries(panels: &Panels, state: &Value) -> Vec<(String, Action, bool)> {
    let current_keys = if panels.clipboard_menu_allowed.is_none() { super::keyframe_clipboard::selection(panels, state) } else { None };
    let keys = panels.clipboard_menu_keys.as_ref().filter(|_| panels.clipboard_menu_allowed.is_some()).or(current_keys.as_ref());
    let allowed = panels
        .clipboard_menu_allowed
        .unwrap_or_else(|| !super::keyboard::layout_shortcuts_blocked(panels) && state["observing"] != true
            && (keys.is_some() || super::keyboard::layer_clipboard_allowed(panels, state)));
    let selected = panels.selection_ids();
    let copy = allowed && !panels.clipboard_busy && !selected.is_empty();
    let cut = copy
        && selected.iter().all(|id| {
            state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|layer| layer["id"] == *id && layer["locked"] != true)
        });
    let paste = allowed && !panels.clipboard_busy && panels.clipboard_available;
    let zh = panels.locale == Locale::Zh;
    let keyframes = keys.is_some();
    [
        (
            Command::CopySelection,
            if keyframes { if zh { "复制关键帧" } else { "Copy Keyframes" } } else if zh { "复制图层" } else { "Copy Layers" },
            Intent::Copy,
            copy,
        ),
        (
            Command::CutSelection,
            if keyframes { if zh { "剪切关键帧" } else { "Cut Keyframes" } } else if zh { "剪切图层" } else { "Cut Layers" },
            Intent::Cut,
            cut,
        ),
        (
            Command::PasteSelection,
            if keyframes { if zh { "在播放头粘贴关键帧" } else { "Paste Keyframes at Playhead" } } else if zh {
                "在播放头粘贴图层"
            } else {
                "Paste Layers at Playhead"
            },
            Intent::Paste { in_place: false },
            paste,
        ),
        (
            Command::PasteInPlace,
            if keyframes { if zh { "原位粘贴关键帧" } else { "Paste Keyframes in Place" } } else if zh {
                "原位粘贴图层"
            } else {
                "Paste Layers in Place"
            },
            Intent::Paste { in_place: true },
            paste,
        ),
    ]
    .into_iter()
    .map(|(command, label, intent, enabled)| {
        (
            panels
                .shortcut_bindings
                .label(label, Scope::Animation, command),
            keys.map(|keys| keys.action(intent)).unwrap_or(Action::LayerClipboard(intent)),
            keys.map(|keys| allowed && !panels.clipboard_busy && keys.enabled(state, intent, panels.keyframe_clipboard_available)).unwrap_or(enabled),
        )
    })
    .chain(std::iter::once((
        if zh {
            "取消剪贴板操作"
        } else {
            "Cancel Clipboard Operation"
        }
        .into(),
        Action::CancelLayerClipboard,
        panels.clipboard_busy,
    )))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_ui::{
        dock::Panel,
        input::{Input, Key, Modifiers},
    };
    fn fixture() -> (Panels, Value) {
        let mut project = motion_core::Project::new(64, 64, 30, 120).unwrap();
        project.layers.push(motion_core::Layer::solid(
            1, "Layer", [32.; 2], [0.; 3], [1.; 4],
        ));
        let mut panels = Panels {
            focused: Some(Panel::Timeline),
            clipboard_available: true,
            ..Default::default()
        };
        panels.select_only(Some(1));
        (
            panels,
            serde_json::json!({"root":"fixture", "project":project,"frame":0}),
        )
    }
    #[test]
    fn menu_enablement_distinguishes_copy_locked_cut_empty_paste_busy_and_captured_owner() {
        let (mut panels, mut state) = fixture();
        assert_eq!(
            entries(&panels, &state)
                .iter()
                .map(|e| e.2)
                .collect::<Vec<_>>(),
            [true, true, true, true, false]
        );
        state["project"]["layers"][0]["locked"] = true.into();
        assert_eq!(
            entries(&panels, &state)
                .iter()
                .map(|e| e.2)
                .collect::<Vec<_>>(),
            [true, false, true, true, false]
        );
        panels.select_only(None);
        panels.clipboard_available = false;
        assert!(entries(&panels, &state).iter().all(|e| !e.2));
        panels.clipboard_busy = true;
        assert_eq!(
            entries(&panels, &state)
                .iter()
                .map(|e| e.2)
                .collect::<Vec<_>>(),
            [false, false, false, false, true]
        );
        panels.clipboard_busy = false;
        panels.clipboard_available = true;
        panels.clipboard_menu_allowed = Some(false);
        assert!(entries(&panels, &state).iter().all(|e| !e.2));
    }
    #[test]
    fn layer_shortcuts_defer_to_editors_graphs_nodes_menus_ime_and_repeat() {
        let (mut panels, state) = fixture();
        let input = Input {
            keys: vec![(
                Key::C,
                Modifiers {
                    control: true,
                    ..Default::default()
                },
            )],
            ..Default::default()
        };
        let route = |p: &mut Panels, input: &Input, node| {
            let mut out = vec![];
            super::super::keyboard::route(input, p, &state, node, None, &mut out);
            out.into_iter()
                .filter(|a| matches!(a, Action::LayerClipboard(_)))
                .count()
        };
        assert_eq!(route(&mut panels, &input, false), 1);
        panels.timeline.timeline_search = true;
        assert_eq!(route(&mut panels, &input, false), 0);
        panels.timeline.timeline_search = false;
        panels.timeline.graph_mode = true;
        assert_eq!(route(&mut panels, &input, false), 0);
        panels.timeline.graph_mode = false;
        panels.focused = Some(Panel::Nodes);
        assert_eq!(route(&mut panels, &input, true), 0);
        panels.focused = Some(Panel::Timeline);
        panels.menu = Some(1);
        assert_eq!(route(&mut panels, &input, false), 0);
        panels.menu = None;
        assert_eq!(
            route(
                &mut panels,
                &Input {
                    composing: true,
                    ..input.clone()
                },
                false
            ),
            0
        );
        assert_eq!(
            route(
                &mut panels,
                &Input {
                    focus_lost: true,
                    ..input.clone()
                },
                false
            ),
            0
        );
        assert_eq!(
            route(
                &mut panels,
                &Input {
                    key_repeats: vec![true],
                    ..input
                },
                false
            ),
            0
        );
    }
}
