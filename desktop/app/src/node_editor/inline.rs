//! Card controls share the document and history with the external inspector.
use super::*;
use motion_ui::widgets::{NumberEditor, NumberOptions};

#[derive(Default)]
pub(super) struct InlineState {
    pub number: Option<NumericEdit>,
    pub popup: Option<Popup>,
}
impl InlineState {
    pub fn busy(&self) -> bool {
        self.number.is_some() || self.popup.is_some()
    }
}
pub(super) struct NumericEdit {
    pub owner: String,
    pub binding: Binding,
    pub editor: NumberEditor,
    pub before: Option<History>,
    pub slider: bool,
    pub in_popup: bool,
}
pub(super) enum Popup {
    Choice {
        position: [f32; 2],
        node: String,
        items: Vec<Choice>,
        selected: usize,
        scroll: f32,
    },
    Color {
        position: [f32; 2],
        node: String,
        bindings: [Binding; 4],
        before: History,
        hue: f32,
        scroll: f32,
        drag: Option<bool>, // false: saturation/value, true: hue
    },
}
#[derive(Clone)]
pub(super) struct Choice {
    pub label: String,
    pub operation: NodeOp,
}
#[derive(Clone)]
pub(super) enum Control {
    Number {
        label: String,
        binding: Binding,
        value: f32,
        options: NumberOptions,
        slider: bool,
    },
    Toggle {
        label: String,
        binding: Option<Binding>,
        checked: bool,
    },
    Color {
        bindings: [Binding; 4],
        value: [f32; 4],
    },
    Choice {
        label: String,
        value: String,
        items: Vec<Choice>,
    },
}
pub(super) const CONTROL_ROW: f32 = 28.;

pub(super) fn control_count(node: &Node, document: &GraphDocument) -> usize {
    match &node.operation {
        NodeOp::Float { .. }
        | NodeOp::Bool { .. }
        | NodeOp::ColorMath { .. }
        | NodeOp::Splat { .. }
        | NodeOp::Select { .. }
        | NodeOp::Image { .. }
        | NodeOp::SampleImage { .. } => 1,
        NodeOp::Vec2 { .. } | NodeOp::Math { .. } => 2,
        NodeOp::Vec3 { .. } => 3,
        NodeOp::Color { .. } => 5,
        NodeOp::Parameter { parameter } => document
            .public_parameters
            .iter()
            .find(|p| p.id == *parameter)
            .map_or(0, |p| {
                dimensions(p.value_type) + usize::from(p.value_type == ValueType::Color)
            }),
        _ => 0,
    }
}

fn number(
    label: &str,
    binding: Binding,
    value: f32,
    min: f32,
    max: f32,
    speed: f32,
    slider: bool,
) -> Control {
    Control::Number {
        label: label.into(),
        binding,
        value,
        slider,
        options: NumberOptions {
            min: min as f64,
            max: max as f64,
            speed: speed as f64,
            decimals: 3,
        },
    }
}
fn choices(
    label: &str,
    current: &NodeOp,
    operations: Vec<NodeOp>,
    language: Language,
    types: bool,
) -> Control {
    let items: Vec<_> = operations
        .into_iter()
        .map(|operation| {
            let text = if types {
                match &operation {
                    NodeOp::Math { value_type, .. }
                    | NodeOp::Splat { value_type }
                    | NodeOp::Select { value_type } => catalog::type_label(*value_type).to_string(),
                    _ => unreachable!(),
                }
            } else {
                match &operation {
                    NodeOp::Image { source } => language
                        .text(
                            if *source == ImageSource::Input {
                                "图层输入"
                            } else {
                                "原始素材"
                            },
                            if *source == ImageSource::Input {
                                "Layer Input"
                            } else {
                                "Original Source"
                            },
                        )
                        .into(),
                    NodeOp::SampleImage { coordinates } => {
                        if *coordinates == SampleCoordinates::Pixels {
                            language.text("像素", "Pixels")
                        } else {
                            "UV"
                        }
                        .into()
                    }
                    _ => catalog::operation_title(&operation, language),
                }
            };
            Choice {
                label: text,
                operation,
            }
        })
        .collect();
    let value = items
        .iter()
        .find(|item| item.operation == *current)
        .map(|item| item.label.clone())
        .unwrap_or_default();
    Control::Choice {
        label: label.into(),
        value,
        items,
    }
}

pub(super) fn controls(node: &Node, document: &GraphDocument, language: Language) -> Vec<Control> {
    let constant = |i| Binding::Constant(node.id.clone(), i);
    let mut out = vec![];
    match &node.operation {
        NodeOp::Float { value } => out.push(number(
            language.text("值", "Value"),
            constant(0),
            *value,
            -1e6,
            1e6,
            0.1,
            false,
        )),
        NodeOp::Vec2 { value } => {
            for (i, value) in value.iter().enumerate() {
                out.push(number(
                    ["X", "Y"][i],
                    constant(i),
                    *value,
                    -1e6,
                    1e6,
                    0.1,
                    false,
                ));
            }
        }
        NodeOp::Vec3 { value } => {
            for (i, value) in value.iter().enumerate() {
                out.push(number(
                    ["X", "Y", "Z"][i],
                    constant(i),
                    *value,
                    -1e6,
                    1e6,
                    0.1,
                    false,
                ));
            }
        }
        NodeOp::Color { value } => {
            out.push(Control::Color {
                bindings: std::array::from_fn(constant),
                value: *value,
            });
            for (i, value) in value.iter().enumerate() {
                out.push(number(
                    ["R", "G", "B", "A"][i],
                    constant(i),
                    *value,
                    0.,
                    1.,
                    0.005,
                    true,
                ));
            }
        }
        NodeOp::Bool { value } => out.push(Control::Toggle {
            label: language.text("启用", "Enabled").into(),
            binding: None,
            checked: *value,
        }),
        NodeOp::Parameter { parameter } => {
            if let Some(p) = document
                .public_parameters
                .iter()
                .find(|p| p.id == *parameter)
            {
                let binding = |i| Binding::ParamDefault(p.id.clone(), i);
                if p.value_type == ValueType::Bool {
                    out.push(Control::Toggle {
                        label: language.text("默认启用", "Default enabled").into(),
                        binding: Some(binding(0)),
                        checked: p.default[0] != 0.,
                    });
                } else {
                    if p.value_type == ValueType::Color {
                        out.push(Control::Color {
                            bindings: std::array::from_fn(binding),
                            value: p.default,
                        });
                    }
                    for i in 0..dimensions(p.value_type) {
                        let label = if dimensions(p.value_type) == 1 {
                            language.text("默认值", "Default")
                        } else if p.value_type == ValueType::Color {
                            ["R", "G", "B", "A"][i]
                        } else {
                            ["X", "Y", "Z", "W"][i]
                        };
                        out.push(number(
                            label,
                            binding(i),
                            p.default[i],
                            p.min,
                            p.max,
                            p.step.max(0.001),
                            true,
                        ));
                    }
                }
            }
        }
        NodeOp::Math {
            operation,
            value_type,
        } => {
            let math = [
                MathOp::Add,
                MathOp::Subtract,
                MathOp::Multiply,
                MathOp::Divide,
                MathOp::Min,
                MathOp::Max,
                MathOp::Abs,
                MathOp::Sin,
                MathOp::Cos,
                MathOp::Sqrt,
                MathOp::Floor,
                MathOp::Fract,
                MathOp::Clamp,
                MathOp::Lerp,
                MathOp::Smoothstep,
                MathOp::Dot,
                MathOp::Length,
                MathOp::Normalize,
            ];
            out.push(choices(
                language.text("运算", "Op"),
                &node.operation,
                math.into_iter()
                    .filter(|op| {
                        *value_type != ValueType::Float
                            || !matches!(op, MathOp::Dot | MathOp::Length | MathOp::Normalize)
                    })
                    .map(|operation| NodeOp::Math {
                        operation,
                        value_type: *value_type,
                    })
                    .collect(),
                language,
                false,
            ));
            let types = if matches!(operation, MathOp::Dot | MathOp::Length | MathOp::Normalize) {
                vec![ValueType::Vec2, ValueType::Vec3]
            } else {
                vec![ValueType::Float, ValueType::Vec2, ValueType::Vec3]
            };
            out.push(choices(
                language.text("类型", "Type"),
                &node.operation,
                types
                    .into_iter()
                    .map(|value_type| NodeOp::Math {
                        operation: *operation,
                        value_type,
                    })
                    .collect(),
                language,
                true,
            ));
        }
        NodeOp::ColorMath { .. } => out.push(choices(
            language.text("运算", "Op"),
            &node.operation,
            [
                ColorOp::Invert,
                ColorOp::Mix,
                ColorOp::Multiply,
                ColorOp::Screen,
                ColorOp::SrgbEncode,
                ColorOp::SrgbDecode,
            ]
            .into_iter()
            .map(|operation| NodeOp::ColorMath { operation })
            .collect(),
            language,
            false,
        )),
        NodeOp::Splat { .. } | NodeOp::Select { .. } => {
            let types = if matches!(node.operation, NodeOp::Splat { .. }) {
                vec![ValueType::Vec2, ValueType::Vec3]
            } else {
                vec![
                    ValueType::Float,
                    ValueType::Vec2,
                    ValueType::Vec3,
                    ValueType::Color,
                    ValueType::Bool,
                    ValueType::DistancePixels,
                ]
            };
            out.push(choices(
                language.text("类型", "Type"),
                &node.operation,
                types
                    .into_iter()
                    .map(|value_type| {
                        if matches!(node.operation, NodeOp::Splat { .. }) {
                            NodeOp::Splat { value_type }
                        } else {
                            NodeOp::Select { value_type }
                        }
                    })
                    .collect(),
                language,
                true,
            ));
        }
        NodeOp::Image { .. } => out.push(choices(
            language.text("来源", "Source"),
            &node.operation,
            [ImageSource::Input, ImageSource::Source]
                .into_iter()
                .map(|source| NodeOp::Image { source })
                .collect(),
            language,
            false,
        )),
        NodeOp::SampleImage { .. } => out.push(choices(
            language.text("坐标", "Coords"),
            &node.operation,
            [SampleCoordinates::Pixels, SampleCoordinates::Uv]
                .into_iter()
                .map(|coordinates| NodeOp::SampleImage { coordinates })
                .collect(),
            language,
            false,
        )),
        _ => {}
    }
    out
}

impl NodeEditor {
    pub(super) fn binding_value(&self, binding: &Binding) -> Option<f32> {
        match binding {
            Binding::Constant(id, i) => {
                self.document
                    .nodes
                    .iter()
                    .find(|n| n.id == *id)
                    .and_then(|n| match &n.operation {
                        NodeOp::Float { value } => [*value].get(*i).copied(),
                        NodeOp::Vec2 { value } => value.get(*i).copied(),
                        NodeOp::Vec3 { value } => value.get(*i).copied(),
                        NodeOp::Color { value } => value.get(*i).copied(),
                        _ => None,
                    })
            }
            Binding::ParamDefault(id, i) => self
                .document
                .public_parameters
                .iter()
                .find(|p| p.id == *id)
                .and_then(|p| p.default.get(*i))
                .copied(),
            _ => None,
        }
    }
    /// Validate each live value, advance preview freshness, and defer history to release.
    pub(super) fn preview_binding(
        &mut self,
        binding: &Binding,
        value: f32,
    ) -> Result<(), Diagnostic> {
        if self.binding_value(binding) == Some(value) {
            return Ok(());
        }
        if !value.is_finite() {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Enter a finite value within ±1e6",
            ));
        }
        match binding {
            Binding::Constant(id, component) => {
                let mut candidate = self
                    .document
                    .nodes
                    .iter()
                    .find(|n| n.id == *id)
                    .cloned()
                    .ok_or_else(|| {
                        diag(ErrorCode::UnknownNode, Some(id), None, "Node is missing")
                    })?;
                match &mut candidate.operation {
                    NodeOp::Float { value: v } if *component == 0 => *v = value,
                    NodeOp::Vec2 { value: v } if *component < 2 => v[*component] = value,
                    NodeOp::Vec3 { value: v } if *component < 3 => v[*component] = value,
                    NodeOp::Color { value: v } if *component < 4 => v[*component] = value,
                    _ => {
                        return Err(diag(
                            ErrorCode::InvalidValue,
                            Some(id),
                            None,
                            "Invalid constant component",
                        ))
                    }
                }
                self.check_node(&candidate)?;
                *self
                    .document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *id)
                    .unwrap() = candidate;
            }
            Binding::ParamDefault(id, component) => {
                let p = self
                    .document
                    .public_parameters
                    .iter_mut()
                    .find(|p| p.id == *id)
                    .ok_or_else(|| {
                        diag(
                            ErrorCode::UnknownParameter,
                            None,
                            Some(id),
                            "Parameter is missing",
                        )
                    })?;
                if *component >= dimensions(p.value_type)
                    || value < p.min
                    || value > p.max
                    || p.value_type == ValueType::Bool && value != 0. && value != 1.
                {
                    return Err(diag(
                        ErrorCode::InvalidValue,
                        None,
                        Some(id),
                        "Value is outside the parameter range",
                    ));
                }
                p.default[*component] = value;
            }
            _ => {
                return Err(diag(
                    ErrorCode::InvalidValue,
                    None,
                    None,
                    "Not an inline value",
                ))
            }
        }
        self.bump(true);
        Ok(())
    }
    pub(super) fn finish_inline_history(&mut self, before: History, commit: bool) {
        if self.document == before.document {
            if self.options.plugin_id != before.options.plugin_id
                || self.options.version != before.options.version
                || self.options.author != before.options.author
                || self.options.license != before.options.license
            {
                self.observe_failure(
                    "inline.history_options",
                    format_args!("commit={commit} reason=document_unchanged_options_differ"),
                );
            }
            return;
        }
        if commit {
            self.finish_edit(before);
        } else {
            self.document = before.document;
            self.options = before.options;
            self.bump(true);
        }
    }
    pub(super) fn cancel_inline(&mut self) {
        if let Some(edit) = self.inline.number.take() {
            if let Some(before) = edit.before {
                self.finish_inline_history(before, false);
            }
        }
        if let Some(Popup::Color { before, .. }) = self.inline.popup.take() {
            self.finish_inline_history(before, false);
        }
    }
}
