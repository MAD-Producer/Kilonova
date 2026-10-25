//! Searchable parent choices freeze the source identity and multi-layer scope.
use super::*;
use motion_ui::{
    input::Key,
    style::{box_style, text_style, State, States},
    widgets::{Checkbox, TextEditor},
};

type Identity = [Value; 4];
fn identity(state: &Value) -> Identity {
    ["root", "composition", "document_generation", "revision"].map(|key| state[key].clone())
}
pub(in crate::panels) enum Decision {
    Open,
    Close,
    Apply(Vec<Command>),
}
pub(in crate::panels) struct Chooser {
    identity: Identity,
    anchor: u64,
    targets: Vec<u64>,
    editor: TextEditor,
    highlighted: usize,
    scroll: usize,
    opened: bool,
    choices: Option<(Locale, Vec<(u64, String)>)>,
    #[cfg(test)]
    pub(crate) rows: Vec<(Option<u64>, Rect)>,
    #[cfg(test)]
    pub(crate) option: Option<Rect>,
}
impl Chooser {
    pub(in crate::panels) fn new(state: &Value, anchor: u64, selected: &[u64]) -> Option<Self> {
        let mut editor = TextEditor::default();
        editor.begin_at_end("");
        Some(Self {
            identity: identity(state),
            anchor,
            targets: targets(state, anchor, selected)?,
            editor,
            highlighted: 0,
            scroll: 0,
            opened: true,
            choices: None,
            #[cfg(test)]
            rows: Vec::new(),
            #[cfg(test)]
            option: None,
        })
    }
    pub(in crate::panels) fn text_menu(
        &mut self,
        request: &mut super::super::text_menu::Request<'_>,
    ) {
        request.visit(&mut self.editor);
        self.bound_query();
    }
    fn bound_query(&mut self) {
        if let Some((end, _)) = self.editor.text.char_indices().nth(128) {
            let text = self.editor.text[..end].to_owned();
            self.editor.begin_at_end(&text);
        }
    }
    pub(in crate::panels) fn current(&self, state: &Value, selected: &[u64]) -> bool {
        self.identity == identity(state)
            && targets(state, self.anchor, selected).as_ref() == Some(&self.targets)
    }
    pub(super) fn commands(
        &self,
        state: &Value,
        parent: Option<u64>,
        mode: ParentMode,
    ) -> Vec<Command> {
        self.targets
            .iter()
            .filter_map(|object| {
                let owner = super::super::timeline_columns::owner(state, *object)?;
                let current = owner["parent"]["object"].as_u64();
                let same_bind = owner["parent"].is_null()
                    || owner["parent"]["bind"]
                        == serde_json::json!(glam::Mat4::IDENTITY.to_cols_array_2d());
                if current == parent && (mode == ParentMode::PreserveWorld || same_bind) {
                    return None;
                }
                let command = Command::ParentWithMode {
                    object: *object,
                    parent,
                    frame: state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32,
                    mode,
                };
                Some(
                    if let Some(composition) = state["composition"]
                        .as_str()
                        .or_else(|| state["project"]["composition_id"].as_str())
                    {
                        Command::InComposition {
                            composition: composition.into(),
                            command: Box::new(command),
                        }
                    } else {
                        command
                    },
                )
            })
            .collect()
    }
    pub(in crate::panels) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        anchor: Rect,
        bounds: Rect,
        state: &Value,
        selected: &[u64],
        mode: &mut ParentMode,
        input: &Input,
        locale: Locale,
        suspended: bool,
    ) -> Decision {
        if !self.current(state, selected) || input.focus_lost {
            return Decision::Close;
        }
        let neutral = Input::default();
        let input = if std::mem::take(&mut self.opened) || suspended {
            &neutral
        } else {
            input
        };
        let key = |key| {
            !input.composing
                && !input.ime_consumed
                && input.keys.iter().any(|(actual, modifiers)| {
                    *actual == key && !modifiers.control && !modifiers.alt
                })
        };
        if key(Key::Escape) {
            return Decision::Close;
        }
        let before = self.editor.text.clone();
        let mut bounded = input.clone();
        bounded.text = bounded.text.chars().take(128).collect();
        self.editor.input(&bounded);
        self.bound_query();
        let query = self.editor.text.to_lowercase();
        let zh = locale == Locale::Zh;
        let mut entries = Vec::new();
        let none = locale.t(T::None).to_owned();
        if query.is_empty() || none.to_lowercase().contains(&query) {
            entries.push((None, none));
        }
        if self
            .choices
            .as_ref()
            .is_none_or(|(language, _)| *language != locale)
        {
            self.choices = Some((locale, candidates_for(state, &self.targets, locale)));
        }
        entries.extend(
            self.choices
                .as_ref()
                .into_iter()
                .flat_map(|(_, choices)| choices.iter())
                .filter(|(id, name)| {
                    query.is_empty()
                        || name.to_lowercase().contains(&query)
                        || id.to_string() == query
                })
                .map(|(id, name)| (Some(*id), name.clone())),
        );
        if self.editor.text != before {
            self.highlighted = 0;
            self.scroll = 0;
        }
        self.highlighted = self.highlighted.min(entries.len().saturating_sub(1));
        if !entries.is_empty() && (key(Key::Up) || key(Key::Down)) {
            self.highlighted = if key(Key::Down) {
                (self.highlighted + 1) % entries.len()
            } else {
                (self.highlighted + entries.len() - 1) % entries.len()
            };
        }
        let styles = list.styles();
        let padding = styles.metric("padding.panel", f32::default());
        let row = styles.metric("row.height", f32::default());
        if row <= 0. || bounds.height() < row * 3. + padding * 2. {
            return Decision::Close;
        }
        let title_visible = bounds.height() >= row * 4. + padding * 2.;
        let header = row * if title_visible { 3. } else { 2. };
        let option_label = if zh {
            "保持当前帧世界变换"
        } else {
            "Keep world transform at this frame"
        };
        let width = styles
            .metric("context.menu.width", f32::default())
            .max(
                atlas.measure(option_label)
                    + styles.metric("icon.slot", f32::default())
                    + padding * 2.,
            )
            .max(anchor.width())
            .min(bounds.width());
        let count = ((bounds.height() - padding * 2. - header) / row)
            .floor()
            .max(1.) as usize;
        let count = count.min(8).min(entries.len().max(1));
        let height = (header + row * count as f32 + padding * 2.).min(bounds.height());
        let popup = Rect::new(
            anchor.min[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
            anchor.max[1].clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
            width,
            height,
        );
        if input
            .pressed
            .is_some_and(|(button, point)| button != MouseButton::Left || !popup.contains(point))
        {
            return Decision::Close;
        }
        if popup.contains(input.mouse) {
            if input.wheel[1] > 0. {
                self.scroll = self.scroll.saturating_add(1);
            }
            if input.wheel[1] < 0. {
                self.scroll = self.scroll.saturating_sub(1);
            }
        }
        if key(Key::Up) || key(Key::Down) {
            if self.highlighted < self.scroll {
                self.scroll = self.highlighted;
            }
            if self.highlighted >= self.scroll + count {
                self.scroll = self.highlighted + 1 - count;
            }
        }
        self.scroll = self.scroll.min(entries.len().saturating_sub(count));
        let mut overlay = list.child();
        box_style(
            &mut overlay,
            popup,
            styles.resolve("popup", States::default()).unwrap(),
        );
        let field = Rect::new(
            popup.min[0] + padding,
            popup.min[1] + padding,
            (popup.width() - padding * 2.).max(0.),
            row,
        );
        let title = if zh {
            format!("父级 · {} 个图层", self.targets.len())
        } else {
            format!("Parent · {} layers", self.targets.len())
        };
        if title_visible {
            text_style(
                &mut overlay,
                atlas,
                field,
                &title,
                styles.resolve("label", States::default()).unwrap(),
                Align::Left,
            );
        }
        let search = Rect::new(
            field.min[0],
            field.min[1] + if title_visible { row } else { 0. },
            field.width(),
            row,
        );
        self.editor.draw(&mut overlay, atlas, search);
        if self.editor.text.is_empty() {
            text_style(
                &mut overlay,
                atlas,
                Rect::new(
                    search.min[0] + padding,
                    search.min[1],
                    (search.width() - padding * 2.).max(0.),
                    search.height(),
                ),
                if zh {
                    "搜索名称或图层 ID…"
                } else {
                    "Search name or layer ID…"
                },
                styles
                    .resolve("label", States::default().with(State::Disabled))
                    .unwrap(),
                Align::Left,
            );
        }
        let option = Rect::new(field.min[0], search.max[1], field.width(), row);
        #[cfg(test)]
        {
            self.option = Some(option);
        }
        if Checkbox::draw(
            &mut overlay,
            atlas,
            option,
            option_label,
            *mode == ParentMode::PreserveWorld,
            input,
            true,
            false,
            None,
        )
        .clicked
        {
            *mode = if *mode == ParentMode::PreserveWorld {
                ParentMode::PreserveLocal
            } else {
                ParentMode::PreserveWorld
            };
        }
        #[cfg(test)]
        self.rows.clear();
        let mut choice = None;
        for (index, (parent, name)) in entries.iter().enumerate().skip(self.scroll).take(count) {
            let rect = Rect::new(
                field.min[0],
                option.max[1] + (index - self.scroll) as f32 * row,
                field.width(),
                row,
            );
            #[cfg(test)]
            self.rows.push((*parent, rect));
            let hovered = rect.contains(input.mouse);
            let mut states = States::default();
            if hovered {
                states = states.with(State::Hovered);
            }
            if index == self.highlighted {
                states = states.with(State::Selected);
            }
            let style = styles.resolve("row", states).unwrap();
            box_style(&mut overlay, rect, style);
            text_style(
                &mut overlay,
                atlas,
                Rect::new(
                    rect.min[0] + padding,
                    rect.min[1],
                    (rect.width() - padding * 2.).max(0.),
                    rect.height(),
                ),
                name,
                style,
                Align::Left,
            );
            if clicked(input, rect) || key(Key::Enter) && index == self.highlighted {
                choice = Some(*parent);
            }
        }
        if entries.is_empty() {
            text_style(
                &mut overlay,
                atlas,
                Rect::new(field.min[0], option.max[1], field.width(), row),
                if zh {
                    "没有匹配的合法图层"
                } else {
                    "No matching legal layers"
                },
                styles
                    .resolve("label", States::default().with(State::Disabled))
                    .unwrap(),
                Align::Left,
            );
        }
        overlay.clip_to(popup);
        list.append(overlay);
        choice.map_or(Decision::Open, |parent| {
            Decision::Apply(self.commands(state, parent, *mode))
        })
    }
}

pub(in crate::panels) fn dispatch(commands: Vec<Command>, out: &mut Vec<Action>) {
    if commands.is_empty() {
        return;
    }
    if commands.len() == 1 {
        for command in commands {
            out.push(Action::Edit(command));
        }
    } else {
        out.push(Action::EditBatch(commands));
    }
}
