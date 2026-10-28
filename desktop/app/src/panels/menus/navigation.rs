use super::*;
use motion_ui::widgets::menu::{self, Entry, Metrics, plain_key};
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(in crate::panels) struct Navigation {
    menu: Option<usize>,
    context: Option<(String, String)>,
    highlighted: usize,
    child: bool,
    child_highlighted: usize,
    child_scroll: usize,
    mouse: Option<[f32; 2]>,
    #[cfg(test)]
    pub(in crate::panels) cards: Vec<Rect>,
}

fn hint(choice: &Choice, panels: &Panels, root: usize, index: usize) -> String {
    use crate::shortcuts::{Command as S, Scope};
    let command = match choice {
        Choice::Animation(super::super::animation_menu::Operation::Shortcut(command)) => {
            Some((Scope::Animation, *command))
        }
        Choice::Animation(super::super::animation_menu::Operation::TimingSettings) => {
            Some((Scope::Animation, S::KeyframeTiming))
        }
        Choice::Direct(action) => Some((
            Scope::Application,
            match action {
                Action::New => S::NewProject,
                Action::Open => S::Open,
                Action::Save => S::Save,
                Action::ImportMedia => S::Import,
                Action::ExportMovie => S::Export,
                Action::History(0) => S::Undo,
                Action::History(1) => S::Redo,
                Action::Preferences => S::Preferences,
                Action::NewComposition => S::NewComposition,
                Action::CompositionSettings => S::Settings,
                Action::TogglePlay => S::Play,
                Action::ToggleAudioPreview => S::AudioPreview,
                Action::Seek(_) if root == 2 && index == 4 => S::FirstFrame,
                Action::Seek(_) if root == 2 && index == 5 => S::LastFrame,
                Action::AddSolid => S::Solid,
                Action::NewText => S::Text,
                Action::AddNull => S::Null,
                Action::AddAdjustment => S::Adjustment,
                Action::Edit(Command::Duplicate { .. }) => S::Duplicate,
                Action::Edit(Command::Delete { .. }) => S::Delete,
                Action::OrderLayers { order, .. } => {
                    return panels
                        .shortcut_bindings
                        .hint(Scope::Animation, order.shortcut());
                }
                Action::LayerClipboard(intent)
                | Action::KeyframeClipboard { intent, .. }
                | Action::CurveClipboard { intent, .. }
                | Action::PathClipboard { intent, .. } => {
                    use crate::layer_clipboard::controller::Intent;
                    return panels.shortcut_bindings.hint(
                        Scope::Animation,
                        match intent {
                            Intent::Copy => S::CopySelection,
                            Intent::Cut => S::CutSelection,
                            Intent::Paste { in_place: false } => S::PasteSelection,
                            Intent::Paste { in_place: true } => S::PasteInPlace,
                        },
                    );
                }
                _ => return String::new(),
            },
        )),
        Choice::Text(command) => return command.binding().1.into(),
        _ => None,
    };
    command
        .map(|(scope, command)| panels.shortcut_bindings.hint(scope, command))
        .unwrap_or_default()
}
fn checked(choice: &Choice, panels: &Panels, state: &Value) -> bool {
    match choice {
        Choice::Direct(Action::TimelineColumnVisibility(next)) => {
            let now = panels.timeline.column_visibility;
            if next.matte != now.matte {
                now.matte
            } else if next.blend != now.blend {
                now.blend
            } else {
                now.parent
            }
        }
        Choice::Direct(Action::SamplingColumnVisibility(next)) => panels
            .timeline
            .sampling_visibility
            .0
            .iter()
            .zip(next.0)
            .find(|(now, next)| **now != *next)
            .is_some_and(|(now, _)| *now),
        Choice::Direct(Action::ToggleDiagnostics) => {
            state["desktop"]["diagnostics"]["enabled"] == true
        }
        Choice::Direct(Action::Workspace(preset)) => *preset == panels.workspace_preset,
        _ => false,
    }
}
fn apply(choice: Choice, panels: &mut Panels, state: &Value, input: &Input, out: &mut Vec<Action>) {
    match choice {
        Choice::Direct(action) => out.push(action),
        Choice::Text(command) => {
            super::super::text_menu::apply(panels, state, command, input.text_clipboard.as_ref())
        }
        Choice::Animation(command) => {
            super::super::animation_menu::apply(panels, state, command, out)
        }
        Choice::RecentEffect(index) => {
            out.extend(super::super::effect_recent::apply(index, panels, state))
        }
        Choice::Workspaces => {}
    }
}
fn workspaces(panels: &Panels) -> Vec<Entry<Choice>> {
    let mut entries: Vec<_> = crate::workspace::Preset::ALL
        .into_iter()
        .map(|preset| {
            let mut entry = Entry::new(
                preset.title(panels.locale).into(),
                Choice::Direct(Action::Workspace(preset)),
                true,
            );
            entry.checked = preset == panels.workspace_preset;
            entry
        })
        .collect();
    if let Some(last) = entries.last_mut() {
        last.divider_after = true;
    }
    entries.push(Entry::new(
        if panels.locale == Locale::Zh {
            "管理工作区…"
        } else {
            "Manage workspaces…"
        }
        .into(),
        Choice::Direct(Action::ManageWorkspace),
        true,
    ));
    entries.push(Entry::new(
        panels.locale.t(T::ResetWorkspace).into(),
        Choice::Direct(Action::ResetWorkspace),
        true,
    ));
    entries
}

pub(super) fn draw(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    starts: &[f32],
    root: usize,
    choices: Vec<(String, Choice, bool)>,
    panels: &mut Panels,
    input: &Input,
    state: &Value,
    out: &mut Vec<Action>,
) {
    let context = (
        state["root"].as_str().unwrap_or("").to_owned(),
        state["composition"].as_str().unwrap_or("").to_owned(),
    );
    let mut navigation = std::mem::take(&mut panels.menu_navigation);
    if navigation.menu == Some(root)
        && navigation
            .context
            .as_ref()
            .is_some_and(|old| *old != context)
    {
        panels.menu = None;
        panels.menu_scroll = 0;
        return;
    }
    if navigation.menu != Some(root) {
        navigation = Navigation {
            menu: Some(root),
            context: Some(context),
            ..Default::default()
        };
    }
    let entry_count = choices.len();
    let entries: Vec<_> = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, choice, enabled))| {
            let choice = if matches!(choice, Choice::Direct(Action::ResetWorkspace)) && root == 7 {
                Choice::Workspaces
            } else {
                choice
            };
            let label = if matches!(choice, Choice::Workspaces) {
                if panels.locale == Locale::Zh {
                    "工作区"
                } else {
                    "Workspaces"
                }
                .into()
            } else {
                label
            };
            let mut entry = Entry::new(label, choice, enabled);
            entry.hint = hint(&entry.choice, panels, root, index);
            entry.checked = checked(&entry.choice, panels, state);
            entry.submenu = matches!(entry.choice, Choice::Workspaces);
            entry.divider_after = match root {
                0 => matches!(index, 2 | 4 | 5),
                1 => index == 1 || index + 2 == entry_count,
                2 => matches!(index, 1 | 3),
                3 => matches!(index, 6 | 9 | 11),
                4 => index == 1,
                7 => entry.submenu,
                _ => false,
            };
            entry
        })
        .collect();
    let styles = list.styles();
    let metrics = Metrics::application(&styles);
    let top = metrics::TITLE_BAR + metrics::MENU_BAR;
    let bounds = Rect::new(
        0.,
        top,
        size[0].max(0.),
        (size[1] - styles.metric("menu.window.bottom_inset", 20.) - top).max(0.),
    );
    let card = menu::popup(
        [starts.get(root).copied().unwrap_or_default(), top],
        bounds,
        metrics.width(&entries, &styles, atlas),
        metrics.height(entries.len(), bounds),
        false,
    );
    let hover =
        navigation.mouse != Some(input.mouse) || input.pressed.is_some() || input.wheel[1] != 0.;
    navigation.mouse = Some(input.mouse);
    navigation.highlighted = navigation.highlighted.min(entries.len().saturating_sub(1));
    if plain_key(input, Key::Tab, false)
        || plain_key(input, Key::Escape, false) && !navigation.child
    {
        panels.menu = None;
        panels.menu_scroll = 0;
        return;
    }
    let mut child_entries = if root == 7 {
        workspaces(panels)
    } else {
        Vec::new()
    };
    let child_card = menu::child(
        card,
        metrics.row_rect(
            card,
            navigation.highlighted.saturating_sub(panels.menu_scroll),
        ),
        bounds,
        metrics.width(&child_entries, &styles, atlas),
        metrics.height(child_entries.len(), bounds),
    );
    let mut parent_input = input.clone();
    if navigation.child && child_card.contains(input.mouse) {
        parent_input.mouse = [f32::MIN, f32::MIN];
        parent_input.pressed = None;
        parent_input.wheel = [0.; 2];
    }
    let response = menu::draw(
        list,
        atlas,
        card,
        metrics,
        &entries,
        &parent_input,
        !navigation.child,
        hover,
        &mut navigation.highlighted,
        &mut panels.menu_scroll,
    );
    let was_child = navigation.child;
    let mut opened = false;
    let mut chosen = None;
    if navigation.child
        && (plain_key(input, Key::Left, false) || plain_key(input, Key::Escape, false))
    {
        navigation.child = false;
    } else {
        let branch = response
            .chosen
            .or(response.hovered)
            .filter(|i| entries[*i].enabled && entries[*i].submenu)
            .or_else(|| {
                (!navigation.child
                    && plain_key(input, Key::Right, false)
                    && entries
                        .get(navigation.highlighted)
                        .is_some_and(|e| e.enabled && e.submenu))
                .then_some(navigation.highlighted)
            });
        if branch.is_some() {
            opened = !navigation.child;
            navigation.child = true;
            if opened {
                navigation.child_highlighted = 0;
                navigation.child_scroll = 0;
            }
        } else if response.hovered.is_some() {
            navigation.child = false;
        }
        if let Some(index) = response.chosen.filter(|i| !entries[*i].submenu) {
            chosen = entries.into_iter().nth(index).map(|e| e.choice);
        }
    }
    if !was_child
        && !navigation.child
        && chosen.is_none()
        && !opened
        && (plain_key(input, Key::Left, false) || plain_key(input, Key::Right, false))
    {
        panels.menu = Some(
            (root
                + if plain_key(input, Key::Left, false) {
                    8
                } else {
                    1
                })
                % 9,
        );
        panels.menu_scroll = 0;
        return;
    }
    #[cfg(test)]
    {
        navigation.cards = vec![card];
    }
    if navigation.child {
        let child_card = menu::child(
            card,
            metrics.row_rect(
                card,
                navigation.highlighted.saturating_sub(panels.menu_scroll),
            ),
            bounds,
            metrics.width(&child_entries, &styles, atlas),
            metrics.height(child_entries.len(), bounds),
        );
        #[cfg(test)]
        {
            navigation.cards.push(child_card);
        }
        let neutral = Input::default();
        let response = menu::draw(
            list,
            atlas,
            child_card,
            metrics,
            &child_entries,
            if opened { &neutral } else { input },
            !opened,
            hover,
            &mut navigation.child_highlighted,
            &mut navigation.child_scroll,
        );
        if let Some(index) = response.chosen {
            chosen = child_entries.drain(..).nth(index).map(|e| e.choice);
        }
    }
    let outside = input.pressed.is_some_and(|(_, point)| {
        point[1] >= top
            && !card.contains(point)
            && !(navigation.child && child_card.contains(point))
    });
    if let Some(choice) = chosen {
        apply(choice, panels, state, input, out);
        panels.menu = None;
        panels.menu_scroll = 0;
    } else if outside {
        panels.menu = None;
        panels.menu_scroll = 0;
    } else {
        panels.menu_navigation = navigation;
    }
}
