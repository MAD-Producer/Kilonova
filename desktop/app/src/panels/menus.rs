use super::*;

enum Choice {
    Direct(Action),
    RecentEffect(usize),
    Text(motion_ui::widgets::TextCommand),
    Animation(super::animation_menu::Operation),
}

// Reuse the existing menu geometry for hit testing before fields process outside clicks.
pub(super) fn regions(atlas: &mut TextAtlas, locale: Locale) -> Vec<(T, Rect)> {
    let mut x = 8.0;
    [T::File, T::Edit, T::Composition, T::Layer, T::Effect,
        T::Animation, T::View, T::Window, T::Help].into_iter().map(|key| {
        let width = atlas.measure(locale.t(key)) + 22.0;
        let rect = Rect::new(x, metrics::TITLE_BAR, width, metrics::MENU_BAR);
        x += width;
        (key, rect)
    }).collect()
}

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    panels: &mut Panels,
    input: &Input,
    state: &Value,
    out: &mut Vec<Action>,
) {
    if input.focus_lost {
        panels.menu = None;
        panels.animation_menu_scope = None;
        panels.menu_scroll = 0;
        return;
    }
    let styles = list.styles();
    let colors = styles.colors();

    let locale = panels.locale;
    paint::rect(
        list,
        Rect::new(0.0, metrics::TITLE_BAR, size[0], metrics::MENU_BAR),
        colors.CHROME,
    );
    let mut starts = vec![];
    for (i, (key, r)) in regions(atlas, locale).into_iter().enumerate() {
        let name = locale.t(key);
        let x = r.min[0];
        let width = r.width();
        starts.push(x);
        if r.contains(input.mouse) || panels.menu == Some(i) {
            paint::rect(list, r, colors.SUNKEN);
        }
        label(
            list,
            atlas,
            Rect::new(
                x + 10.0,
                metrics::TITLE_BAR,
                width - 20.0,
                metrics::MENU_BAR,
            ),
            name,
            false,
        );
        if clicked(input, r) {
            let next = if panels.menu == Some(i) {
                None
            } else {
                Some(i)
            };
            if panels.menu != next {
                panels.menu_scroll = 0;
            }
            panels.menu = next;
        }
    }
    if panels.menu == Some(5) {
        let current = super::animation_menu::MenuScope::of(panels, state);
        if panels.animation_menu_scope.as_ref().is_some_and(|scope| *scope != current) {
            panels.menu = None;
            panels.menu_scroll = 0;
            panels.animation_menu_scope = None;
            return;
        }
        panels.animation_menu_scope = Some(current);
    } else {
        panels.animation_menu_scope = None;
    }
    if let Some(menu) = panels.menu {
        let selected = panels.selected.filter(|id| *id != 0);
        let frame = state["frame"].as_f64().unwrap_or(0.0).floor() as u32;
        let editable = selected.is_some_and(|id| state["project"]["layers"].as_array().into_iter().flatten()
            .any(|layer| layer["id"] == id && layer["locked"] != true)) && state["observing"] != true;
        let entries: Vec<(String, Action, bool)> = match menu {
            0 => vec![
                (
                    panels.shortcut_bindings.label(locale.t(T::NewProject), crate::shortcuts::Scope::Application, crate::shortcuts::Command::NewProject),
                    Action::New,
                    true,
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::OpenProject), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Open),
                    Action::Open,
                    true,
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::Save), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Save),
                    Action::Save,
                    true,
                ),
                (locale.t(T::ExportPackage).into(), Action::Pack, true),
                (
                    locale.t(T::ProjectPrecision).into(),
                    Action::ProjectPrecisionSettings,
                    state["observing"] != true,
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::ImportMedia), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Import),
                    Action::ImportMedia,
                    true,
                ),
                (locale.t(T::ExportPng).into(), Action::ExportPng, true),
                (
                    panels.shortcut_bindings.label(locale.t(T::ExportMovie), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Export),
                    Action::ExportMovie,
                    cfg!(feature = "ffmpeg"),
                ),
                (locale.t(T::CancelExport).into(), Action::CancelExport, true),
                (locale.t(T::CancelImport).into(), Action::CancelImport, true),
            ],
            1 => { let mut entries = vec![
                (
                    panels.shortcut_bindings.label(locale.t(T::Undo), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Undo),
                    Action::History(0),
                    super::tools::camera::history_enabled(panels,state,false),
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::Redo), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Redo),
                    Action::History(1),
                    super::tools::camera::history_enabled(panels,state,true),
                ),
                (
                    if locale == Locale::Zh { "取消未提交的插件编辑" } else { "Cancel Pending Plugin Edit" }.into(),
                    Action::Plugin(serde_json::json!({"op":"cancel_plugin_gesture"})),
                    state["preview"]["pluginEditorRecovery"]["gesture"] == true && state["observing"] != true,
                ),
                (
                    panels.shortcut_bindings.label(if locale == Locale::Zh { "偏好设置…" } else { "Preferences…" },crate::shortcuts::Scope::Application,crate::shortcuts::Command::Preferences),
                    Action::Preferences,
                    true,
                ),
            ];
                entries.splice(2..2, super::clipboard_menu::entries(panels, state));
                entries
            },
            2 => vec![
                (
                    panels.shortcut_bindings.label(locale.t(T::NewComposition), crate::shortcuts::Scope::Application, crate::shortcuts::Command::NewComposition),
                    Action::NewComposition,
                    true,
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::CompositionSettings), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Settings),
                    Action::CompositionSettings,
                    true,
                ),
                (locale.t(T::Play).into(), Action::TogglePlay, true),
                (
                    panels.shortcut_bindings.label(
                        if locale == Locale::Zh { "仅音频预览 / 停止" } else { "Audio-only preview / Stop" },
                        crate::shortcuts::Scope::Application,
                        crate::shortcuts::Command::AudioPreview,
                    ),
                    Action::ToggleAudioPreview,
                    true,
                ),
                (locale.t(T::FirstFrame).into(), Action::Seek(0.0), true),
                (
                    locale.t(T::LastFrame).into(),
                    Action::Seek(last_frame(state)),
                    true,
                ),
            ],
            3 => {
                let (lock_selection, lock_commands) = super::layer_locks::selection(state, panels);
                let unlock_commands = super::layer_locks::changes(state, None, false);
                let can_lock = !lock_commands.is_empty();
                let can_unlock = !unlock_commands.is_empty();
                let mut entries = vec![
                (panels.shortcut_bindings.label(locale.t(T::NewSolid), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Solid), Action::AddSolid, true),
                (panels.shortcut_bindings.label(locale.t(T::NewText), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Text), Action::NewText, true),
                (panels.shortcut_bindings.label(locale.t(T::NewNull), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Null), Action::AddNull, true),
                (
                    panels.shortcut_bindings.label(locale.t(T::NewAdjustment), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Adjustment),
                    Action::AddAdjustment,
                    true,
                ),
                (
                    locale.t(T::NewCamera).into(),
                    Action::Edit(Command::CreateCamera),
                    true,
                ),
                (
                    locale.t(T::NewRectangle).into(),
                    Action::AddShape(motion_core::vector::ShapeKind::Rectangle),
                    true,
                ),
                (
                    locale.t(T::NewEllipse).into(),
                    Action::AddShape(motion_core::vector::ShapeKind::Ellipse),
                    true,
                ),
                (
                    locale.t(T::SplitLayer).into(),
                    Action::Edit(Command::SplitLayerClip {
                        object: selected.unwrap_or(0),
                        frame,
                    }),
                    editable,
                ),
                (
                    panels.shortcut_bindings.label(locale.t(T::Duplicate), crate::shortcuts::Scope::Application, crate::shortcuts::Command::Duplicate),
                    Action::Edit(Command::Duplicate {
                        object: selected.unwrap_or(0),
                    }),
                    editable,
                ),
                (
                    locale.t(T::Delete).into(),
                    Action::Edit(Command::Delete {
                        object: selected.unwrap_or(0),
                    }),
                    editable,
                ),
                (
                    auxiliary::localized(locale,
                        if lock_selection { "锁定所选图层" } else { "解锁所选图层" },
                        if lock_selection { "Lock Selected Layers" } else { "Unlock Selected Layers" }).into(),
                    Action::EditBatch(lock_commands),
                    can_lock,
                ),
                (
                    auxiliary::localized(locale, "全部解锁", "Unlock All Layers").into(),
                    Action::EditBatch(unlock_commands),
                    can_unlock,
                ),
                ];
                entries.extend(super::layer_navigation::menu_entries(panels, state));
                entries
            },
            4 => vec![
                (locale.t(T::ShowEffects).into(), Action::ShowPanel(dock::DockId(2)), true),
                super::effect_removal::menu_entry(state, panels),
            ],
            5 => Vec::new(),
            6 => {
                let columns = panels.timeline.column_visibility;
                let mut entries = vec![
                    (
                        locale.t(T::ShowComposition).into(),
                        Action::ShowPanel(dock::DockId(3)),
                        true,
                    ),
                    (
                        locale.t(T::MatteColumn).into(),
                        Action::TimelineColumnVisibility(ColumnVisibility {
                            matte: !columns.matte,
                            ..columns
                        }),
                        true,
                    ),
                    (
                        locale.t(T::BlendColumn).into(),
                        Action::TimelineColumnVisibility(ColumnVisibility {
                            blend: !columns.blend,
                            ..columns
                        }),
                        true,
                    ),
                    (
                        locale.t(T::ParentColumn).into(),
                        Action::TimelineColumnVisibility(ColumnVisibility {
                            parent: !columns.parent,
                            ..columns
                        }),
                        true,
                    ),
                ];
                if super::layer_sampling::available(state) {
                    for index in 0..3 {
                        let mut next = panels.timeline.sampling_visibility;
                        next.0[index] = !next.0[index];
                        let caption = super::layer_sampling::caption(locale,index);
                        entries.push((if locale == Locale::Zh {format!("{caption}列")} else {format!("{caption} column")},
                            Action::SamplingColumnVisibility(next),true));
                    }
                }
                entries
            }
            7 => Panel::available()
                .map(|panel| {
                    (
                        locale.panel(panel.dock()).into(),
                        Action::ShowPanel(panel.dock()),
                        true,
                    )
                })
                .chain(std::iter::once((
                    locale.t(T::ResetWorkspace).into(),
                    Action::ResetWorkspace,
                    true,
                )))
                .collect(),
            _ => vec![
                (
                    locale.t(T::ExportDiagnostics).into(),
                    Action::ExportDiagnostics,
                    true,
                ),
                (
                    locale.t(T::SendDiagnostics).into(),
                    Action::ToggleDiagnostics,
                    state["desktop"]["diagnostics"]["configured"]
                        .as_bool()
                        .unwrap_or(false),
                ),
                (locale.t(T::About).into(), Action::About, true),
            ],
        };
        let entries: Vec<_> = if menu == 1 && panels.text_menu.is_some() {
            super::text_menu::entries(panels, state).into_iter()
                .map(|(text, command, enabled)| (text, Choice::Text(command), enabled)).collect()
        } else if menu == 5 {
            super::animation_menu::entries(panels, state).into_iter()
                .map(|(text, command, enabled)| (text, Choice::Animation(command), enabled)).collect()
        } else {
            entries.into_iter().map(|(text, action, enabled)| (text, Choice::Direct(action), enabled)).collect()
        };
        let recent = if menu == 4 {
            super::effect_recent::rows(panels, state)
        } else {
            Vec::new()
        };
        let row_count = entries.len() + recent.len();
        let desired_height = row_count as f32 * 32.0 + 8.0;
        let top = metrics::TITLE_BAR + metrics::MENU_BAR;
        let bottom = (size[1] - 20.0).max(top + 32.0);
        let max_height = (bottom - top).max(32.0);
        let visible_rows = (((max_height - 8.0) / 32.0).floor() as usize)
            .max(1)
            .min(row_count);
        let max_scroll = row_count.saturating_sub(visible_rows);
        if panels.menu_scroll > max_scroll {
            panels.menu_scroll = max_scroll;
        }
        let popup_height = desired_height.min(max_height);
        let minimum_width = styles.metric("menu.popup.min_width", styles.metric("context.menu.width", f32::default()));
        let popup_width = if menu == 1 || menu == 5 {
            entries.iter().map(|(text,_,_)| atlas.measure(text))
                .fold(minimum_width, |width, text| width.max(text + styles.metric("padding.panel", f32::default()) * 4.))
                .min(size[0])
        } else { minimum_width };
        let popup = Rect::new(
            starts
                .get(menu)
                .copied()
                .unwrap_or(8.0)
                .min((size[0] - popup_width).max(0.0)),
            top,
            popup_width,
            popup_height,
        );
        if popup.contains(input.mouse) {
            if input.wheel[1] < 0.0 {
                panels.menu_scroll = (panels.menu_scroll + 1).min(max_scroll);
            } else if input.wheel[1] > 0.0 {
                panels.menu_scroll = panels.menu_scroll.saturating_sub(1);
            }
        }
        // Text is drawn in a second GPU pass: remove covered labels before drawing the popup.
        list.retain_glyphs(|g| {
            g.position[0] + g.size[0] <= popup.min[0]
                || g.position[0] >= popup.max[0]
                || g.position[1] + g.size[1] <= popup.min[1]
                || g.position[1] >= popup.max[1]
        });
        let start = panels.menu_scroll;
        let mut popup_list = list.child();
        paint::rect(&mut popup_list, popup, colors.CHROME);
        paint::stroke(&mut popup_list, popup, colors.BORDER, 1.0);
        let choices = entries
            .into_iter()
            .chain(recent.into_iter().map(|(text, index, enabled)| {
                (text, Choice::RecentEffect(index), enabled)
            }));
        for (i, (text, choice, enabled)) in choices
            .enumerate()
            .skip(start)
            .take(visible_rows)
        {
            let r = Rect::new(
                popup.min[0] + 4.0,
                popup.min[1] + 4.0 + (i - start) as f32 * 32.0,
                popup.width() - 8.0,
                30.0,
            );
            if r.contains(input.mouse) {
                paint::rect(&mut popup_list, r, colors.SUNKEN);
            }
            let action = match &choice {
                Choice::Direct(action) => Some(action),
                Choice::RecentEffect(_) | Choice::Text(_) | Choice::Animation(_) => None,
            };
            let checked = match action {
                Some(Action::TimelineColumnVisibility(next)) => {
                    let current = panels.timeline.column_visibility;
                    if next.matte != current.matte { current.matte }
                    else if next.blend != current.blend { current.blend }
                    else { current.parent }
                }
                Some(Action::ToggleDiagnostics) => state["desktop"]["diagnostics"]["enabled"] == true,
                Some(Action::SamplingColumnVisibility(next)) => panels.timeline.sampling_visibility.0.iter()
                    .zip(next.0).find(|(current,next)| **current != *next).is_some_and(|(current,_)|*current),
                _ => false,
            };
            let slot = if matches!(action, Some(Action::TimelineColumnVisibility(_) | Action::SamplingColumnVisibility(_) | Action::ToggleDiagnostics)) {
                styles.metric("icon.slot", 20.)
            } else { 0. };
            if checked {
                motion_ui::icons::draw(&mut popup_list,
                    Rect::new(r.max[0] - slot, r.min[1], slot, r.height()),
                    motion_ui::icons::Icon::Check, styles.metric("checkbox.size", 14.),
                    if enabled { colors.TEXT } else { styles.color("disabled") });
            }
            label(
                &mut popup_list,
                atlas,
                Rect::new(r.min[0] + 8.0, r.min[1], r.width() - 16.0 - slot, r.height()),
                &text,
                !enabled,
            );
            if clicked(input, r) && enabled {
                let action = match choice {
                    Choice::Text(command) => {
                        super::text_menu::apply(panels, state, command, input.text_clipboard.as_ref());
                        None
                    }
                    Choice::Animation(command) => {
                        super::animation_menu::apply(panels, state, command, out);
                        None
                    }
                    Choice::Direct(action) => Some(action),
                    Choice::RecentEffect(index) => super::effect_recent::apply(index, panels, state),
                };
                out.extend(action);
                panels.menu = None;
                panels.menu_scroll = 0;
            }
        }
        popup_list.clip_to(popup);
        list.append(popup_list);
        if input.pressed.is_some_and(|(_, p)| {
            !popup.contains(p) && p[1] >= metrics::TITLE_BAR + metrics::MENU_BAR
        }) {
            panels.menu = None;
            panels.menu_scroll = 0;
        }
    }
}
