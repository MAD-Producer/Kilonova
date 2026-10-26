//! Expressions belong to their property, never to an independent dock panel.
use super::*;
use motion_core::{ExpressionTarget, PropertyExpression};
use motion_ui::widgets::{Checkbox, TextEditor, TextEvent};
mod expanded;
pub(crate) use expanded::Ticket;

pub(super) fn find(state: &Value, target: &ExpressionTarget) -> Option<PropertyExpression> {
    state["project"]["expressions"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|value| {
            let expression: PropertyExpression = observe!(
                "expressions.target",
                serde_json::from_value(value.clone()),
                "target={target:?}"
            )
            .ok()?;
            (expression.target == *target).then_some(expression)
        })
}

#[derive(Default)]
pub(crate) struct Controls {
    pub(super) owner: Option<Panel>,
    menu: motion_ui::widgets::Dropdown,
    menu_target: Option<(ExpressionTarget, Rect, Panel)>,
    menu_opened: bool,
    pub(super) draft: Option<ExpressionTarget>,
    target: Option<ExpressionTarget>,
    editor: TextEditor,
    loaded: String,
    toggle_focus: bool,
    expanded: Option<expanded::Editor>,
    #[cfg(test)]
    pub(super) triggers: Vec<(ExpressionTarget, Rect)>,
    #[cfg(test)]
    pub(super) rects: Vec<(ExpressionTarget, Rect, Rect)>,
}
impl Controls {
    pub(super) fn text_menu(&mut self, request: &mut super::text_menu::Request<'_>) {
        if let Some(editor) = &mut self.expanded {
            editor.text_menu(request);
            return;
        }
        request.visit(&mut self.editor);
    }
    pub fn editing(&self) -> bool {
        self.editor.active() || self.toggle_focus || self.menu.open || self.expanded.is_some()
    }
    pub fn cancel(&mut self) {
        self.menu.open = false;
        self.menu_target = None;
        self.editor.cancel();
        self.editor.text = self.loaded.clone();
        self.draft = None;
        self.toggle_focus = false;
        self.expanded = None;
    }
    pub(crate) fn open(&mut self, state: &Value, target: ExpressionTarget, owner: Panel) {
        self.cancel();
        self.loaded = find(state, &target).map_or_else(|| "value".into(), |e| e.source);
        self.editor.begin_bounded(&self.loaded, 8192);
        self.owner = Some(owner);
        self.target = Some(target.clone());
        self.draft = Some(target.clone());
        if self.loaded.contains('\n') {
            self.open_expanded(state, target, owner);
        }
    }
    pub(super) fn visible(&self, _state: &Value, target: &ExpressionTarget, owner: Panel) -> bool {
        self.owner == Some(owner) && self.draft.as_ref() == Some(target)
    }
    pub(super) fn menu_open(&self) -> bool {
        self.menu.open || self.expanded.is_some()
    }
    pub(super) fn context(
        &mut self,
        target: ExpressionTarget,
        rect: Rect,
        input: &Input,
        owner: Panel,
    ) -> bool {
        #[cfg(test)]
        {
            self.triggers.retain(|(t, _)| t != &target);
            self.triggers.push((target.clone(), rect));
        }
        if let Some((MouseButton::Right, point)) = input.pressed.filter(|(_, p)| rect.contains(*p))
        {
            self.owner = Some(owner);
            self.menu = motion_ui::widgets::Dropdown::default();
            self.menu.open = true;
            self.menu_opened = true;
            self.menu_target = Some((
                target,
                Rect::new(point[0], point[1], rect.width(), 0.),
                owner,
            ));
            return true;
        }
        false
    }
    pub(super) fn finish_menu(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        out: &mut Vec<Action>,
    ) {
        if self.expanded.is_some() {
            self.expanded_overlay(list, atlas, bounds, state, input, locale, out);
            return;
        }
        let Some((target, mut anchor, owner)) = self.menu_target.clone() else {
            return;
        };
        let styles = list.styles();
        anchor.max[0] = anchor.min[0]
            + styles
                .metric("context.menu.width", 220.)
                .min(bounds.width());
        let layer = state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l["id"].as_u64() == Some(target.object()));
        let enabled = state["observing"] != true && layer.is_some_and(|l| l["locked"] != true);
        let neutral = Input::default();
        let input = if std::mem::take(&mut self.menu_opened) {
            &neutral
        } else {
            input
        };
        let dimension = match target {
            ExpressionTarget::Property { object, property, .. }
                if matches!(property, Property::Position | Property::Rotation | Property::Scale) =>
            {
                let separated = super::animation_data::timeline(state, object)
                    .is_some_and(|t| t["properties"][super::animation_data::name(property)]["separated"] == true);
                Some((separated, object, property))
            }
            _ => None,
        };
        let mut items = vec![
            motion_ui::widgets::DropdownItem {
                label: auxiliary::localized(locale, "编辑表达式", "Edit expression"),
                enabled,
            },
            motion_ui::widgets::DropdownItem {
                label: auxiliary::localized(locale, "移除表达式", "Remove expression"),
                enabled: enabled && find(state, &target).is_some(),
            },
        ];
        if let Some((separated, _, _)) = dimension {
            items.push(motion_ui::widgets::DropdownItem {
                label: if separated {
                    auxiliary::localized(locale, "合并 XYZ（相同键与插值）", "Merge XYZ (same keys/easing)")
                } else {
                    auxiliary::localized(locale, "分离 XYZ", "Separate XYZ")
                },
                enabled,
            });
        }
        let expanded_index = items.len();
        items.push(motion_ui::widgets::DropdownItem {
            label: auxiliary::localized(locale, "在多行编辑器中打开…", "Open multiline editor…"),
            enabled,
        });
        match self
            .menu
            .draw_menu(list, atlas, anchor, bounds, &items, input, true)
        {
            Some(0) => self.open(state, target, owner),
            Some(1) => {
                out.push(Action::Edit(Command::RemoveExpression { target }));
                self.cancel();
            }
            Some(2) if dimension.is_some() => {
                if let Some((separated, object, property)) = dimension {
                    out.push(Action::Edit(if separated {
                        Command::MergeDimensions { object, property }
                    } else {
                        Command::SeparateDimensions { object, property }
                    }));
                    self.cancel();
                }
            }
            Some(index) if index == expanded_index => self.open_expanded(state, target, owner),
            _ => {}
        }
        if !self.menu.open {
            self.menu_target = None;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn row(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        caption: Rect,
        source: Rect,
        state: &Value,
        target: ExpressionTarget,
        input: &Input,
        locale: Locale,
        editable: bool,
        out: &mut Vec<Action>,
    ) {
        let mut text_input = input.clone();
        if input.composing || input.ime_consumed {
            text_input.keys.clear();
            text_input.key_repeats.clear();
        }
        let input = &text_input;
        if self.target.as_ref() == Some(&target)
            && (input.focus_lost || input.key_presses().any(|(k, _, r)| k == Key::Escape && !r))
        {
            self.cancel();
        }
        let styles = list.styles();
        let slot = styles.metric("icon.slot", 20.);
        let remove = Rect::new(
            (source.max[0] - slot).max(source.min[0]),
            source.min[1],
            slot.min(source.width()),
            source.height(),
        );
        let expand_width = if source.width() >= slot + slot { slot } else { 0. };
        let expand = Rect::new(remove.min[0] - expand_width,
            source.min[1], expand_width, source.height());
        let source = Rect::new(
            source.min[0],
            source.min[1],
            (source.width() - slot - expand_width).max(0.),
            source.height(),
        );
        let existing = find(state, &target);
        if ui::button_icon(list, remove, motion_ui::icons::Icon::Close, input, true).clicked {
            self.cancel();
        }
        if expand_width > 0. && ui::button_icon(list, expand, motion_ui::icons::Icon::Expand, input, editable).clicked {
            self.open_expanded(state, target.clone(), self.owner.unwrap_or(Panel::Timeline));
        }
        let states = if editable {
            motion_ui::style::States::default()
        } else {
            motion_ui::style::States::default().with(motion_ui::style::State::Disabled)
        };
        let value = existing.as_ref().map_or("value", |e| e.source.as_str());
        let selected = self.target.as_ref() == Some(&target);
        if selected
            && !self.editor.active()
            && (self.editor.text == self.loaded || self.editor.text == value)
        {
            self.loaded = value.into();
            self.editor.text = value.into();
        }
        if input.pressed.is_some() && selected {
            self.toggle_focus = input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && caption.contains(p));
            if self.toggle_focus {
                self.editor.cancel();
            }
        }
        let clicked_source = input
            .pressed
            .is_some_and(|(b, p)| b == MouseButton::Left && source.contains(p));
        if editable && clicked_source {
            if value.contains('\n') {
                self.open_expanded(state, target.clone(), self.owner.unwrap_or(Panel::Timeline));
            } else if !selected {
                self.open(state, target.clone(), self.owner.unwrap_or(Panel::Timeline));
            } else if !self.editor.active() {
                let draft = self.editor.text.clone();
                self.editor.begin_bounded(&draft, 8192);
            }
        }
        if selected
            && input
                .key_presses()
                .any(|(key, _, repeat)| key == Key::Tab && !repeat)
        {
            self.toggle_focus = !self.toggle_focus;
            self.editor.cancel();
        }
        if !editable && selected {
            self.editor.cancel();
            self.toggle_focus = false;
        }
        #[cfg(test)]
        {
            self.rects.retain(|(t, _, _)| t != &target);
            self.rects.push((target.clone(), caption, source));
        }
        if Checkbox::draw(
            list,
            atlas,
            caption,
            auxiliary::localized(locale, "表达式", "Expression"),
            existing.as_ref().is_some_and(|e| e.enabled),
            input,
            editable && existing.is_some(),
            selected && self.toggle_focus,
            None,
        )
        .clicked
        {
            if let Some(mut expression) = existing.clone() {
                expression.enabled = !expression.enabled;
                out.push(Action::Edit(Command::SetExpression {
                    expression,
                    frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                }));
            }
        }
        if self.target.as_ref() == Some(&target) {
            if editable && !self.toggle_focus {
                match self.editor.input(input) {
                    Some(TextEvent::Commit(_)) => {
                        let mut expression = existing.unwrap_or(PropertyExpression {
                            target: target.clone(),
                            source: String::new(),
                            enabled: true,
                            seed: 0,
                            profile: motion_core::EXPRESSION_PROFILE.into(),
                        });
                        expression.source = self.editor.text.clone();
                        out.push(Action::Edit(Command::SetExpression {
                            expression,
                            frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        }));
                        self.editor.cancel();
                    }
                    Some(TextEvent::Cancel) => {
                        self.cancel();
                    }
                    None => {}
                }
            }
            if self.editor.text.contains('\n') {
                let mut preview = TextEditor::default();
                preview.text = format!("{} …", self.editor.text.lines().next().unwrap_or(""));
                preview.draw_styled(list, atlas, source, &styles, "field.number", states);
            } else {
                self.editor.draw_styled(list, atlas, source, &styles, "field.number", states);
            }
        } else {
            let mut editor = TextEditor::default();
            editor.text = if value.contains('\n') { format!("{} …", value.lines().next().unwrap_or("")) } else { value.into() };
            editor.draw_styled(list, atlas, source, &styles, "field.number", states);
        }
    }
}

#[cfg(test)]
mod tests;
