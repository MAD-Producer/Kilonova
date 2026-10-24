//! Path ownership precedes layer/keyframe clipboard routing without owning edits.
use super::{Action, Panels};
use crate::layer_clipboard::{controller::Intent, PathSource};
use motion_core::{PathDestination, PathSelection};
use motion_ui::dock::Panel;
use serde_json::Value;

#[derive(Clone, Debug)]
pub(crate) struct Selection {
    source: PathSource,
    selections: Vec<PathSelection>,
    target: PathDestination,
    writable: bool,
}
impl Selection {
    pub(super) fn new(
        state: &Value,
        selections: Vec<PathSelection>,
        target: PathDestination,
        writable: bool,
    ) -> Self {
        Self {
            source: PathSource::of(state),
            selections,
            target,
            writable,
        }
    }
    pub(super) fn enabled(&self, state: &Value, intent: Intent, available: bool) -> bool {
        self.source == PathSource::of(state)
            && state["observing"] != true
            && match intent {
                Intent::Copy => !self.selections.is_empty(),
                Intent::Cut => self.writable && !self.selections.is_empty(),
                Intent::Paste { .. } => self.writable && available,
            }
    }
    pub(super) fn action(&self, intent: Intent) -> Action {
        Action::PathClipboard {
            intent,
            source: self.source.clone(),
            selections: self.selections.clone(),
            target: self.target.clone(),
        }
    }
}
pub(super) fn selection(panels: &Panels, state: &Value) -> Option<Selection> {
    if !matches!(
        panels.focused,
        Some(Panel::Composition | Panel::Masks | Panel::Vector)
    ) {
        return None;
    }
    // Selecting a destination cancels old point gestures and their private focus.
    // A focused composition still owns path paste while the path tool is active.
    if panels.focused != Some(Panel::Composition) && !panels.path_controls.wants_keyboard() {
        return None;
    }
    panels
        .path_controls
        .path_clipboard_selection(state, panels.selected)
}
pub(super) fn entries(
    panels: &Panels,
    state: &Value,
    selection: &Selection,
    allowed: bool,
) -> Vec<(String, Action, bool)> {
    use crate::{
        i18n::Locale,
        shortcuts::{Command, Scope},
    };
    let zh = panels.locale == Locale::Zh;
    [
        (
            Command::CopySelection,
            Intent::Copy,
            if zh { "复制路径" } else { "Copy Paths" },
        ),
        (
            Command::CutSelection,
            Intent::Cut,
            if zh { "剪切路径" } else { "Cut Paths" },
        ),
        (
            Command::PasteSelection,
            Intent::Paste { in_place: false },
            if zh {
                "在播放头粘贴路径"
            } else {
                "Paste Paths at Playhead"
            },
        ),
        (
            Command::PasteInPlace,
            Intent::Paste { in_place: true },
            if zh {
                "原位粘贴路径"
            } else {
                "Paste Paths in Place"
            },
        ),
    ]
    .into_iter()
    .map(|(command, intent, label)| {
        (
            panels
                .shortcut_bindings
                .label(label, Scope::Animation, command),
            selection.action(intent),
            allowed
                && !panels.clipboard_busy
                && selection.enabled(state, intent, panels.path_clipboard_available),
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
