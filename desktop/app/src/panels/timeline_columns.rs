//! Editable timeline cells with one modal dropdown and typed core commands.
use super::*;
use motion_core::compositing::{BlendMode, MatteMode, TrackMatte};
use motion_ui::widgets::{Dropdown, DropdownItem};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Column {
    Blend,
    Parent,
    Matte,
}
struct Popup {
    object: u64,
    column: Column,
    rect: Rect,
    dropdown: Dropdown,
    window: Option<Panel>,
}
#[derive(Default)]
pub(crate) struct Controls {
    popup: Option<Popup>,
    focused: Option<(u64, Column)>,
    seen: bool,
    opened: bool,
    context: Option<(String, String)>,
    window: Option<Panel>,
    deferred: bool,
}
impl Controls {
    pub fn editing(&self) -> bool {
        self.popup.is_some()
    }
    pub fn editing_in(&self, window: Option<Panel>) -> bool {
        self.popup.as_ref().is_some_and(|p| p.window == window)
    }
    pub(super) fn prepare_window(&mut self, window: Option<Panel>, input: &Input) {
        if self.editing() && !self.editing_in(window) && input.pressed.is_some() {
            self.cancel();
        }
        self.window = window;
        self.deferred = true;
        if self.editing_in(window) {
            self.seen = false;
        }
    }
    pub(super) fn finish_window(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        self.deferred = false;
        if self.editing_in(self.window) {
            self.overlay(list, atlas, bounds, state, input, locale, out);
        }
    }
    pub fn cancel(&mut self) {
        self.popup = None;
        self.focused = None;
    }
    pub(super) fn begin(&mut self, state: &Value) {
        let context = (
            state["root"].as_str().unwrap_or("").to_string(),
            state["composition"].as_str().unwrap_or("").to_string(),
        );
        if self.context.as_ref() != Some(&context) {
            self.cancel();
            self.context = Some(context);
        }
        self.seen = false;
        self.opened = false;
    }
    pub(super) fn cell(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        object: u64,
        column: Column,
        caption: &str,
        input: &Input,
        enabled: bool,
    ) -> bool {
        let sheet = list.styles();
        let colors = sheet.colors();
        if self
            .popup
            .as_ref()
            .is_some_and(|p| p.object == object && p.column == column)
        {
            self.seen = true;
            if let Some(p) = &mut self.popup {
                p.rect = rect;
            }
            if !enabled {
                self.cancel();
            }
        }
        if enabled && rect.contains(input.mouse) {
            paint::rect(list, rect, colors.TEXT.with_alpha(0.06));
        }
        let focused = self.focused == Some((object, column));
        if focused {
            paint::stroke(list, rect, colors.ACCENT.with_alpha(0.65), 0.5);
        }
        ui::text(
            list,
            atlas,
            Rect::new(
                rect.min[0] + 4.,
                rect.min[1],
                (rect.width() - 22.).max(1.),
                rect.height(),
            ),
            caption,
            if enabled {
                colors.TEXT_MUTED
            } else {
                sheet.color("disabled").with_alpha(0.6)
            },
            Align::Left,
        );
        let x = rect.max[0] - 11.;
        let y = rect.min[1] + rect.height() * 0.5;
        paint::line(
            list,
            vec![[x - 3., y - 1.], [x, y + 2.], [x + 3., y - 1.]],
            if enabled {
                colors.TEXT_MUTED
            } else {
                sheet.color("disabled").with_alpha(0.4)
            },
        );
        if enabled && clicked(input, rect) {
            self.focused = Some((object, column));
            self.popup = Some(Popup {
                object,
                column,
                rect,
                dropdown: Dropdown::default(),
                window: self.window,
            });
            self.seen = true;
            self.opened = true;
            return true;
        }
        false
    }
    pub(super) fn overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if self.deferred || self.popup.as_ref().is_some_and(|p| p.window != self.window) {
            return;
        }
        if !self.seen || input.focus_lost {
            self.cancel();
            return;
        }
        let Some(popup) = &mut self.popup else {
            return;
        };
        let Some(owner) = owner(state, popup.object) else {
            self.cancel();
            return;
        };
        let mattes =
            (popup.column == Column::Matte).then(|| matte_choices(state, popup.object, locale));
        let (labels, selected, parents) = match popup.column {
            Column::Blend => {
                let current = blend(owner);
                let modes = blend_modes(locale);
                (
                    modes
                        .iter()
                        .map(|(_, label)| label.to_string())
                        .collect::<Vec<_>>(),
                    modes
                        .iter()
                        .position(|(mode, _)| *mode == current)
                        .unwrap_or(0),
                    vec![],
                )
            }
            Column::Parent => {
                let choices = super::parent_controls::menu_entries(state, popup.object, locale);
                let current = owner["parent"]["object"].as_u64();
                let selected = choices
                    .iter()
                    .position(|(id, _, _)| *id == current)
                    .unwrap_or(0);
                let labels = choices.iter().map(|(_, name, _)| name.clone()).collect();
                let parents = choices.iter().map(|(id, _, _)| *id).collect();
                (labels, selected, parents)
            }
            Column::Matte => {
                let (choices, _, selected) = mattes.clone().unwrap();
                let labels = choices.iter().map(|c| c.label.clone()).collect();
                (labels, selected, vec![])
            }
        };
        let allowed = match popup.column {
            Column::Parent => super::parent_controls::menu_entries(state, popup.object, locale)
                .into_iter()
                .map(|(_, _, enabled)| enabled)
                .collect::<Vec<_>>(),
            Column::Matte => mattes
                .as_ref()
                .map(|(choices, _, _)| choices.iter().map(|c| c.enabled).collect())
                .unwrap(),
            Column::Blend => vec![true; labels.len()],
        };
        let groups: &[usize] = match popup.column {
            Column::Blend => &[1, 4, 8, 11],
            Column::Parent => &[1],
            Column::Matte => mattes
                .as_ref()
                .map(|(_, groups, _)| groups.as_slice())
                .unwrap(),
        };
        let items: Vec<_> = labels
            .iter()
            .zip(&allowed)
            .map(|(label, enabled)| DropdownItem {
                label,
                enabled: *enabled,
            })
            .collect();

        // Let the shared widget initialize its keyboard highlight on the opening frame.
        let first = Input {
            pressed: Some((
                MouseButton::Left,
                [popup.rect.min[0] + 1., popup.rect.min[1] + 1.],
            )),
            mouse: input.mouse,
            ..Default::default()
        };
        let choice = popup.dropdown.draw_grouped(
            list,
            atlas,
            popup.rect,
            bounds,
            &items,
            selected,
            if self.opened { &first } else { input },
            true,
            true,
            groups,
            true,
        );
        if let Some(index) = choice {
            let command = match popup.column {
                Column::Blend => {
                    let mode = blend_modes(locale)[index].0;
                    if mode == blend(owner) {
                        None
                    } else {
                        Some(Command::SetLayerBlend {
                            object: popup.object,
                            mode: Some(mode),
                            space: None,
                        })
                    }
                }
                Column::Parent => {
                    let parent = parents[index];
                    if parent == owner["parent"]["object"].as_u64() {
                        None
                    } else {
                        Some(Command::Parent {
                            object: popup.object,
                            parent,
                            frame: state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32,
                        })
                    }
                }
                Column::Matte => {
                    let matte = mattes.as_ref().unwrap().0[index].matte;
                    let current: Option<TrackMatte> = observe!(
                        "timeline.matte_current",
                        serde_json::from_value(owner["track_matte"].clone()),
                        "object={}",
                        popup.object
                    )
                    .ok()
                    .flatten();
                    if matte == current {
                        None
                    } else {
                        Some(Command::SetTrackMatte {
                            object: popup.object,
                            matte,
                        })
                    }
                }
            };
            if let Some(command) = command {
                out.push(Action::Edit(command));
            }
        }
        if !popup.dropdown.open {
            self.popup = None;
            self.focused = None;
        }
    }
}
pub(super) fn owner(state: &Value, object: u64) -> Option<&Value> {
    if object == 0 {
        Some(&state["project"]["camera"])
    } else {
        state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|l| l["id"].as_u64() == Some(object))
    }
}
pub(super) fn blend(layer: &Value) -> BlendMode {
    if layer["blend"]["mode"].is_null() { return BlendMode::default(); }
    observe!("timeline.blend", serde_json::from_value(layer["blend"]["mode"].clone()), "object={} fallback=normal", layer["id"].as_u64().unwrap_or(0)).unwrap_or_default()
}
/// One track matte menu row shared by the timeline dropdown and the layer
/// menu: `matte == None` clears, otherwise applies the whole matte at once.
#[derive(Clone)]
pub(super) struct MatteChoice {
    pub label: String,
    pub matte: Option<TrackMatte>,
    pub enabled: bool,
}
/// Choices are "none", every candidate source layer in display order, then
/// mode and source-visibility toggles when a matte is set. `groups` holds the
/// divider start indices and the return's last element is the selected index.
pub(super) fn matte_choices(
    state: &Value,
    object: u64,
    locale: Locale,
) -> (Vec<MatteChoice>, Vec<usize>, usize) {
    let zh = locale == Locale::Zh;
    let mut choices = vec![MatteChoice {
        label: if zh {
            "无遮罩".into()
        } else {
            "No track matte".into()
        },
        matte: None,
        enabled: true,
    }];
    let mut groups = vec![1usize];
    let mut selected = 0usize;
    let Some(layers) = state["project"]["layers"].as_array() else {
        return (choices, groups, selected);
    };
    let current: Option<TrackMatte> = layers
        .iter()
        .find(|l| l["id"].as_u64() == Some(object))
        .and_then(|l| {
            observe!(
                "timeline.matte_choice",
                serde_json::from_value(l["track_matte"].clone()),
                "object={object}"
            )
            .ok()
        })
        .flatten();
    let mut stored: Option<Vec<motion_core::Layer>> = observe!(
        "timeline.matte_layers",
        serde_json::from_value(serde_json::json!(layers)),
        "object={object} layer_count={}",
        layers.len()
    )
    .ok();
    let camera = state["project"]["camera"]["created"] == true;
    for (index, l) in layers.iter().rev().enumerate() {
        let id = l["id"].as_u64().unwrap_or(0);
        if id == object {
            continue;
        }
        let visual = !matches!(
            l["content"]["kind"].as_str(),
            Some("null" | "audio" | "adjustment")
        );
        let matte = Some(TrackMatte {
            source: id,
            mode: current.map(|m| m.mode).unwrap_or_default(),
            hide_source: current.map_or(true, |m| m.hide_source),
        });
        let valid = stored.as_mut().is_some_and(|candidate| {
            let Some(target) = candidate.iter_mut().find(|layer| layer.id == object) else {
                return false;
            };
            target.track_matte = matte;
            motion_core::compositing::validate(candidate).is_ok()
        });
        if current.is_some_and(|m| m.source == id) {
            selected = choices.len();
        }
        choices.push(MatteChoice {
            label: format!(
                "{}. {}",
                index + 1 + usize::from(camera),
                l["name"].as_str().unwrap_or("Layer")
            ),
            matte,
            enabled: visual && valid,
        });
    }
    if let Some(m) = current {
        groups.push(choices.len());
        for (mode, zh_label, en_label) in [
            (MatteMode::Alpha, "Alpha 遮罩", "Alpha"),
            (MatteMode::AlphaInverted, "Alpha 反转遮罩", "Alpha inverted"),
            (MatteMode::Luma, "亮度遮罩", "Luma"),
            (MatteMode::LumaInverted, "亮度反转遮罩", "Luma inverted"),
        ] {
            choices.push(MatteChoice {
                label: if zh {
                    format!("模式：{zh_label}")
                } else {
                    format!("Mode: {en_label}")
                },
                matte: Some(TrackMatte { mode, ..m }),
                enabled: true,
            });
        }
        choices.push(MatteChoice {
            label: if zh {
                format!("隐藏源图层：{}", if m.hide_source { "开" } else { "关" })
            } else {
                format!("Hide source: {}", if m.hide_source { "on" } else { "off" })
            },
            matte: Some(TrackMatte {
                hide_source: !m.hide_source,
                ..m
            }),
            enabled: true,
        });
    }
    (choices, groups, selected)
}
pub(super) fn matte_name(layer: &Value, state: &Value, locale: Locale) -> String {
    let Some(source) = layer["track_matte"]["source"].as_u64() else {
        return locale.t(T::None).to_string();
    };
    let camera = state["project"]["camera"]["created"] == true;
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .enumerate()
        .find(|(_, l)| l["id"].as_u64() == Some(source))
        .map_or_else(
            || locale.t(T::None).to_string(),
            |(index, l)| {
                format!(
                    "{}. {}",
                    index + 1 + usize::from(camera),
                    l["name"].as_str().unwrap_or("Layer")
                )
            },
        )
}
pub(super) fn blend_name(layer: &Value, locale: Locale) -> &'static str {
    blend_modes(locale)
        .into_iter()
        .find(|(mode, _)| *mode == blend(layer))
        .map(|(_, label)| label)
        .unwrap_or("Normal")
}
pub(super) fn parent_name(layer: &Value, state: &Value, locale: Locale) -> String {
    let Some(id) = layer["parent"]["object"].as_u64() else {
        return locale.t(T::None).to_string();
    };
    let camera = state["project"]["camera"]["created"] == true;
    if id == 0 {
        return format!(
            "1. {}",
            if locale == Locale::Zh {
                "摄像机"
            } else {
                "Camera"
            }
        );
    }
    state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .enumerate()
        .find(|(_, l)| l["id"].as_u64() == Some(id))
        .map_or_else(
            || locale.t(T::None).to_string(),
            |(index, l)| {
                format!(
                    "{}. {}",
                    index + 1 + usize::from(camera),
                    l["name"].as_str().unwrap_or("Layer")
                )
            },
        )
}
fn blend_modes(locale: Locale) -> [(BlendMode, &'static str); 15] {
    use BlendMode::*;
    let zh = locale == Locale::Zh;
    [
        (Normal, if zh { "正常" } else { "Normal" }),
        (Darken, if zh { "变暗" } else { "Darken" }),
        (Multiply, if zh { "相乘" } else { "Multiply" }),
        (ColorBurn, if zh { "颜色加深" } else { "Color Burn" }),
        (Add, if zh { "相加" } else { "Add" }),
        (Lighten, if zh { "变亮" } else { "Lighten" }),
        (Screen, if zh { "屏幕" } else { "Screen" }),
        (ColorDodge, if zh { "颜色减淡" } else { "Color Dodge" }),
        (Overlay, if zh { "叠加" } else { "Overlay" }),
        (SoftLight, if zh { "柔光" } else { "Soft Light" }),
        (HardLight, if zh { "强光" } else { "Hard Light" }),
        (Difference, if zh { "差值" } else { "Difference" }),
        (Exclusion, if zh { "排除" } else { "Exclusion" }),
        (Subtract, if zh { "相减" } else { "Subtract" }),
        (Divide, if zh { "相除" } else { "Divide" }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(p: &motion_core::Project) -> Value {
        serde_json::json!({"project": p})
    }
    fn project() -> motion_core::Project {
        let mut p = motion_core::Project::new(640, 360, 30, 120).unwrap();
        for id in [7, 9] {
            p.layers.push(motion_core::Layer::solid(
                id,
                &format!("Layer {id}"),
                [50.; 2],
                [320., 180., 0.],
                [1.; 4],
            ));
        }
        p
    }

    #[test]
    fn matte_choices_list_sources_without_mode_rows_when_unset() {
        let p = project();
        let (choices, groups, selected) = matte_choices(&state(&p), 7, Locale::En);
        assert_eq!(selected, 0);
        assert_eq!(groups, vec![1]);
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].matte, None);
        assert_eq!(choices[1].label, "1. Layer 9");
        assert_eq!(
            choices[1].matte,
            Some(TrackMatte {
                source: 9,
                mode: MatteMode::Alpha,
                hide_source: true,
            })
        );
        assert!(choices.iter().all(|c| c.enabled));
    }

    #[test]
    fn matte_choices_add_mode_and_hide_rows_once_set() {
        let mut p = project();
        p.layers[0].track_matte = Some(TrackMatte {
            source: 9,
            mode: MatteMode::Luma,
            hide_source: true,
        });
        let (choices, groups, selected) = matte_choices(&state(&p), 7, Locale::Zh);
        assert_eq!(selected, 1);
        assert_eq!(groups, vec![1, 2]);
        assert_eq!(choices.len(), 7);
        assert_eq!(choices[2].matte.unwrap().mode, MatteMode::Alpha);
        assert_eq!(choices[4].matte.unwrap().mode, MatteMode::Luma);
        // The hide-source row toggles the flag while keeping source and mode.
        let toggle = choices[6].matte.unwrap();
        assert_eq!(toggle.source, 9);
        assert_eq!(toggle.mode, MatteMode::Luma);
        assert!(!toggle.hide_source);
        assert!(toggle != p.layers[0].track_matte.unwrap());
    }

    #[test]
    fn matte_choices_disables_cycle_and_skips_self() {
        let mut p = project();
        p.layers[1].track_matte = Some(TrackMatte {
            source: 7,
            mode: MatteMode::Alpha,
            hide_source: true,
        });
        let (choices, _, _) = matte_choices(&state(&p), 7, Locale::En);
        // Self is never listed and the layer pointing back at us is disabled.
        assert_eq!(choices.len(), 2);
        assert!(!choices[1].enabled);
    }
    #[test]
    fn matte_choices_disable_nonvisual_sources_and_excessive_dependency_depth() {
        let mut p = project();
        for id in 10..=18 {
            p.layers.push(motion_core::Layer::solid(
                id,
                "Chain",
                [50.; 2],
                [320., 180., 0.],
                [1.; 4],
            ));
        }
        for id in 10..18 {
            p.layers
                .iter_mut()
                .find(|l| l.id == id)
                .unwrap()
                .track_matte = Some(TrackMatte {
                source: id + 1,
                mode: MatteMode::Alpha,
                hide_source: true,
            });
        }
        p.layers.iter_mut().find(|l| l.id == 9).unwrap().content = motion_core::Content::Null;
        let (choices, _, _) = matte_choices(&state(&p), 7, Locale::En);
        assert!(
            !choices
                .iter()
                .find(|c| c.matte.is_some_and(|m| m.source == 9))
                .unwrap()
                .enabled
        );
        assert!(
            !choices
                .iter()
                .find(|c| c.matte.is_some_and(|m| m.source == 10))
                .unwrap()
                .enabled
        );
        assert!(
            choices
                .iter()
                .find(|c| c.matte.is_some_and(|m| m.source == 11))
                .unwrap()
                .enabled
        );
    }
    #[test]
    fn matte_dropdown_reaches_engine_and_each_change_undoes_and_persists() {
        for locale in [Locale::En, Locale::Zh] {
            let mut engine = motion_core::Engine::new(project()).unwrap();
            let before = engine.project().clone();
            let mut controls = Controls::default();
            let mut atlas = TextAtlas::from_system(12.).unwrap();
            let rect = Rect::new(20., 20., 120., 24.);
            let bounds = Rect::new(0., 0., 400., 400.);
            let snapshot = state(engine.project());
            controls.begin(&snapshot);
            controls.cell(
                &mut PaintList::default(),
                &mut atlas,
                rect,
                7,
                Column::Matte,
                "None",
                &Input {
                    mouse: [30., 30.],
                    pressed: Some((MouseButton::Left, [30., 30.])),
                    ..Default::default()
                },
                true,
            );
            let mut actions = vec![];
            controls.overlay(
                &mut PaintList::default(),
                &mut atlas,
                bounds,
                &snapshot,
                &Input::default(),
                locale,
                &mut actions,
            );
            assert!(actions.is_empty());
            for key in [Key::Down, Key::Enter] {
                controls.begin(&snapshot);
                controls.cell(
                    &mut PaintList::default(),
                    &mut atlas,
                    rect,
                    7,
                    Column::Matte,
                    "None",
                    &Input::default(),
                    true,
                );
                controls.overlay(
                    &mut PaintList::default(),
                    &mut atlas,
                    bounds,
                    &snapshot,
                    &Input {
                        keys: vec![(key, Default::default())],
                        ..Default::default()
                    },
                    locale,
                    &mut actions,
                );
            }
            assert!(matches!(
                &actions[..],
                [Action::Edit(Command::SetTrackMatte {
                    object: 7,
                    matte: Some(TrackMatte { source: 9, .. })
                })]
            ));
            for action in actions {
                let Action::Edit(command) = action else {
                    panic!()
                };
                engine.apply(command).unwrap();
            }
            let selected = engine.project().clone();
            let (choices, _, _) = matte_choices(&state(&selected), 7, locale);
            for choice in choices
                .into_iter()
                .filter(|choice| choice.matte != selected.layers[0].track_matte)
            {
                engine
                    .apply(Command::SetTrackMatte {
                        object: 7,
                        matte: choice.matte,
                    })
                    .unwrap();
                let folder = tempfile::tempdir().unwrap();
                motion_core::storage::save(folder.path(), engine.project()).unwrap();
                assert_eq!(
                    &motion_core::storage::load(folder.path()).unwrap(),
                    engine.project()
                );
                engine.undo().unwrap();
                assert_eq!(engine.project(), &selected);
            }
            engine.undo().unwrap();
            assert_eq!(engine.project(), &before);
        }
    }
}
