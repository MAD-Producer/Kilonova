use super::*;
use motion_effects::{ParamDefinition, ParamKind};
#[derive(Clone, Copy, PartialEq)]
enum TextField {
    Name,
    Units,
    Options,
}
#[derive(Default)]
pub struct Parameters {
    selected: usize,
    selector: Dropdown,
    kind: Dropdown,
    enum_default: Dropdown,
    numbers: [NumberEditor; 7],
    name: Option<(TextField, TextEditor)>,
}
impl Parameters {
    pub fn editing(&self) -> bool {
        self.enum_default.open
            || self.selector.open
            || self.kind.open
            || self.name.is_some()
            || self.numbers.iter().any(NumberEditor::interacting)
    }
    pub fn finish(&mut self, draft: &mut model::Draft) -> Result<(), String> {
        if let Some((field, editor)) = &self.name {
            if let PluginLogic::Wgsl { document } = &draft.project.logic {
                if let Some(p) = document.parameters.get(self.selected) {
                    let id = p.id.clone();
                    let value = editor.text.clone();
                    let field = *field;
                    draft.edit(|draft| {
                        set_text(parameter(draft, &id)?, field, value);
                        Ok(())
                    })?;
                    self.name = None;
                }
            }
        }
        draft.commit();
        Ok(())
    }
    pub fn cancel(&mut self, draft: &mut model::Draft) {
        if self.numbers.iter().any(NumberEditor::interacting) {
            draft.cancel();
        }
        self.enum_default.open = false;
        self.selector.open = false;
        self.kind.open = false;
        self.name = None;
        for n in &mut self.numbers {
            let _ = n.cancel();
        }
    }
    fn text_field(
        &mut self,
        draft: &mut model::Draft,
        id: &str,
        field: TextField,
        label: &str,
        value: &str,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        vx: f32,
        y: f32,
        input: &Input,
    ) -> Result<(), String> {
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let r = Rect::new(vx, y, area.max[0] - vx, row);
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, vx - area.min[0], row),
            label,
            sheet.color("text"),
            Align::Left,
        );
        if !self.editing() && press(input, r) {
            let mut e = TextEditor::default();
            e.begin(value);
            self.name = Some((field, e));
        }
        if let Some((owner, editor)) = &mut self.name {
            if *owner == field {
                let event = editor.input(input);
                editor.draw_styled(list, atlas, r, &sheet, "field", Default::default());
                match event {
                    Some(TextEvent::Commit(value)) => {
                        draft.edit(|p| {
                            set_text(parameter(p, id)?, field, value);
                            Ok(())
                        })?;
                        self.name = None;
                    }
                    Some(TextEvent::Cancel) => self.name = None,
                    _ => {}
                }
                return Ok(());
            }
        }
        ui::text(list, atlas, r, value, sheet.color("text"), Align::Left);
        Ok(())
    }
    pub fn draw(
        &mut self,
        draft: &mut model::Draft,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        zh: bool,
    ) -> Result<(), String> {
        let PluginLogic::Wgsl { document } = &draft.project.logic else {
            return Ok(());
        };
        let selector_owned = self.selector.open;
        let kind_owned = self.kind.open;
        let default_owned = self.enum_default.open;
        let popup_owned = selector_owned || kind_owned || default_owned;
        let parameters = document.parameters.clone();
        self.selected = self.selected.min(parameters.len().saturating_sub(1));
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let pad = sheet.metric("padding", 5.);
        let disabled = neutral();
        let command = if self.editing() { &disabled } else { input };
        let mut y = area.min[1];
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &tr(
                zh,
                "公共参数 · ID 保持稳定",
                "Public parameters · stable IDs",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        y += row;
        // The parameter budget is 32. Keep a bounded list, with scrolling left
        // to the source inspector rather than compressing parameter hit areas.
        // Persistent selection is rendered as a native popup below; kind owns
        // its own input and cannot fall through to another parameter.
        let choices: Vec<_> = parameters
            .iter()
            .map(|p| DropdownItem {
                label: p.name.as_str(),
                enabled: true,
            })
            .collect();
        let mut selector_paint = list.child();
        if !choices.is_empty() {
            if let Some(index) = self.selector.draw_inline(
                &mut selector_paint,
                atlas,
                Rect::new(area.min[0], y, area.width(), row),
                area,
                &choices,
                self.selected.min(parameters.len() - 1),
                if self.kind.open
                    || self.enum_default.open
                    || self.name.is_some()
                    || (popup_owned && !selector_owned)
                {
                    &disabled
                } else {
                    input
                },
                true,
                false,
            ) {
                self.selected = index;
            }
        }
        // The actual persistent selector lives separately from the type menu.
        y += row;
        let width = (area.width() - pad) * 0.5;
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0], y, width, row),
            &tr(zh, "添加参数", "Add parameter"),
            command,
            parameters.len() < motion_effects::MAX_PARAMS,
        )
        .clicked
        {
            draft.edit(|p|{let PluginLogic::Wgsl{document}=&mut p.logic else{return Err("WGSL parameter owner changed".into());};
                let mut i=1;while document.parameters.iter().any(|p|p.id==format!("parameter_{i}")){i+=1;}
                let id=format!("parameter_{i}");let param:ParamDefinition=serde_json::from_value(serde_json::json!({"id":id,"name":tr(zh,"新参数","New parameter"),"kind":"float","default":[0,0,0,0],"min":0,"max":100,"step":0.1,"units":"","animatable":true})).map_err(|e|e.to_string())?;
                document.parameters.push(param);Ok(())})?;
            self.selected = parameters.len();
        }
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + width + pad, y, width, row),
            &tr(zh, "移除参数", "Remove"),
            command,
            !parameters.is_empty(),
        )
        .clicked
        {
            if let Some(p) = parameters.get(self.selected) {
                draft.edit(|draft| model::remove_parameter(draft, &p.id))?;
                self.selected = self.selected.saturating_sub(1);
            }
        }
        y += row + pad;
        let Some(p) = parameters.get(self.selected.min(parameters.len().saturating_sub(1))) else {
            return Ok(());
        };
        let id = p.id.clone();
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &format!("ID: {id}"),
            sheet.color("muted"),
            Align::Left,
        );
        y += row;
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &format!(
                "{}()",
                motion_graph::plugin_project::parameter_function(&id)
            ),
            sheet.color("focus"),
            Align::Left,
        );
        y += row;
        let vx = area.min[0] + sheet.metric("label.column", 72.);
        self.text_field(
            draft,
            &id,
            TextField::Name,
            &tr(zh, "名称", "Name"),
            &p.name,
            list,
            atlas,
            area,
            vx,
            y,
            if popup_owned { &disabled } else { input },
        )?;
        y += row;
        self.text_field(
            draft,
            &id,
            TextField::Units,
            &tr(zh, "单位", "Units"),
            &p.units,
            list,
            atlas,
            area,
            vx,
            y,
            if popup_owned { &disabled } else { input },
        )?;
        y += row;
        if p.kind == ParamKind::Enum {
            self.text_field(
                draft,
                &id,
                TextField::Options,
                &tr(zh, "选项（| 分隔）", "Options (|)"),
                &p.options.join(" | "),
                list,
                atlas,
                area,
                vx,
                y,
                if popup_owned { &disabled } else { input },
            )?;
            y += row;
        }
        let kinds = [
            ParamKind::Float,
            ParamKind::Vec2,
            ParamKind::Vec3,
            ParamKind::Color,
            ParamKind::Bool,
            ParamKind::Enum,
        ];
        let labels = ["Float", "Vec2", "Vec3", "Color", "Bool", "Enum"];
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, vx - area.min[0], row),
            &tr(zh, "类型", "Type"),
            sheet.color("text"),
            Align::Left,
        );
        let items: Vec<_> = labels
            .iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let mut popup = list.child();
        if let Some(index) = self.kind.draw_inline(
            &mut popup,
            atlas,
            Rect::new(vx, y, area.max[0] - vx, row),
            area,
            &items,
            kinds.iter().position(|k| k == &p.kind).unwrap_or(0),
            if self.name.is_none()
                && !self.selector.open
                && !self.enum_default.open
                && (!popup_owned || kind_owned)
            {
                input
            } else {
                &disabled
            },
            true,
            false,
        ) {
            draft.edit(|draft| {
                let p = parameter(draft, &id)?;
                p.kind = kinds[index];
                p.default = [0.; 4];
                if p.kind == ParamKind::Color {
                    p.default[3] = 1.;
                }
                p.min = 0.;
                p.max = if matches!(p.kind, ParamKind::Color | ParamKind::Bool | ParamKind::Enum) {
                    1.
                } else {
                    100.
                };
                p.step = if matches!(p.kind, ParamKind::Bool | ParamKind::Enum) {
                    1.
                } else {
                    0.1
                };
                p.options = if p.kind == ParamKind::Enum {
                    vec!["A".into(), "B".into()]
                } else {
                    vec![]
                };
                draft
                    .interface
                    .validate_bindings(&draft.parameters().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                Ok(())
            })?;
        }
        y += row;
        let owned = self.numbers.iter().position(NumberEditor::interacting);
        for (i, (label, value, min, max)) in [
            (tr(zh, "最小值", "Minimum"), p.min, -1e9, 1e9),
            (tr(zh, "最大值", "Maximum"), p.max, -1e9, 1e9),
            (tr(zh, "步长", "Step"), p.step, 0.0001, 1e9),
        ]
        .into_iter()
        .enumerate()
        {
            let r = Rect::new(vx, y, area.max[0] - vx, row);
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, vx - area.min[0], row),
                &label,
                sheet.color("text"),
                Align::Left,
            );
            let event = if !popup_owned
                && !self.selector.open
                && !self.kind.open
                && self.name.is_none()
                && owned.is_none_or(|axis| axis == i)
            {
                input
            } else {
                &disabled
            };
            let options = NumberOptions {
                min,
                max,
                speed: 0.1,
                decimals: 2,
            };
            let number = &mut self.numbers[i];
            number.compact = true;
            number.align_left = true;
            for change in number.update(value as f64, options, event, r) {
                match change {
                    NumberEvent::Begin { .. } => draft.begin(),
                    NumberEvent::Change { value } | NumberEvent::Commit { value } => {
                        draft.update(|draft| {
                            let p = parameter(draft, &id)?;
                            match i {
                                0 => p.min = value as f32,
                                1 => p.max = value as f32,
                                2 => p.step = value as f32,
                                _ => p.default[i - 3] = value as f32,
                            }
                            Ok(())
                        })?;
                        if matches!(change, NumberEvent::Commit { .. }) {
                            draft.commit();
                        }
                    }
                    NumberEvent::Cancel { .. } => draft.cancel(),
                }
            }
            number.draw_styled(
                list,
                atlas,
                r,
                value as f64,
                options,
                &sheet,
                Default::default(),
            );
            y += row;
        }
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, vx - area.min[0], row),
            &tr(zh, "默认值", "Default"),
            sheet.color("text"),
            Align::Left,
        );
        let cell = sheet.metric("effect.number.cell_width", 44.);
        let gap = sheet.metric("effect.number.gap", 4.);
        let event = if !popup_owned && !self.selector.open && !self.kind.open && self.name.is_none()
        {
            input
        } else {
            &disabled
        };
        if p.kind == ParamKind::Bool {
            if motion_ui::widgets::Checkbox::draw(
                list,
                atlas,
                Rect::new(vx, y, cell, row),
                "",
                p.default[0] != 0.,
                event,
                true,
                false,
                None,
            )
            .clicked
            {
                draft.edit(|draft| {
                    parameter(draft, &id)?.default[0] = if p.default[0] == 0. { 1. } else { 0. };
                    Ok(())
                })?;
            }
        } else if p.kind == ParamKind::Enum {
            let choices: Vec<_> = p
                .options
                .iter()
                .map(|label| DropdownItem {
                    label,
                    enabled: true,
                })
                .collect();
            if let Some(index) = self.enum_default.draw_inline(
                &mut popup,
                atlas,
                Rect::new(vx, y, area.max[0] - vx, row),
                area,
                &choices,
                p.default[0] as usize,
                if default_owned { input } else { event },
                true,
                false,
            ) {
                draft.edit(|draft| {
                    parameter(draft, &id)?.default[0] = index as f32;
                    Ok(())
                })?;
            }
        } else if p.min.is_finite() && p.max.is_finite() && p.min <= p.max {
            for axis in 0..p.kind.dimensions() {
                let options = NumberOptions {
                    min: p.min as f64,
                    max: p.max as f64,
                    speed: if p.kind == ParamKind::Enum {
                        1.
                    } else {
                        p.step as f64
                    },
                    decimals: if p.kind == ParamKind::Enum { 0 } else { 2 },
                };
                let number = &mut self.numbers[axis + 3];
                number.compact = true;
                number.align_left = true;
                let r = Rect::new(vx + axis as f32 * (cell + gap), y, cell, row);
                let event = if owned.is_none_or(|i| i == axis + 3) {
                    event
                } else {
                    &disabled
                };
                for change in number.update(p.default[axis] as f64, options, event, r) {
                    match change {
                        NumberEvent::Begin { .. } => draft.begin(),
                        NumberEvent::Change { value } | NumberEvent::Commit { value } => {
                            draft.update(|draft| {
                                parameter(draft, &id)?.default[axis] = value as f32;
                                Ok(())
                            })?;
                            if matches!(change, NumberEvent::Commit { .. }) {
                                draft.commit();
                            }
                        }
                        NumberEvent::Cancel { .. } => draft.cancel(),
                    }
                }
                number.draw_styled(
                    list,
                    atlas,
                    r,
                    p.default[axis] as f64,
                    options,
                    &sheet,
                    Default::default(),
                );
            }
        }
        y += row;
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &tr(zh, "允许动画", "Animatable"),
            p.animatable,
            event,
            true,
            false,
            None,
        )
        .clicked
        {
            draft.edit(|draft| {
                parameter(draft, &id)?.animatable = !p.animatable;
                Ok(())
            })?;
        }
        list.append(popup);
        list.append(selector_paint);
        Ok(())
    }
}
fn parameter<'a>(
    draft: &'a mut PluginProject,
    id: &str,
) -> Result<&'a mut ParamDefinition, String> {
    let PluginLogic::Wgsl { document } = &mut draft.logic else {
        return Err("parameter owner changed".into());
    };
    document
        .parameters
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or_else(|| "parameter missing".into())
}

fn set_text(p: &mut ParamDefinition, field: TextField, value: String) {
    match field {
        TextField::Name => p.name = value,
        TextField::Units => p.units = value,
        TextField::Options => p.options = value.split('|').map(|s| s.trim().to_owned()).collect(),
    }
}
