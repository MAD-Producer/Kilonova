//! Native panel layout data shared by authoring and the eventual host renderer.
//! It owns no values, clocks, native code, colors or executable commands.
use crate::{ensure, valid_id, Error, ParamDefinition, ParamKind, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const LAYOUT_VERSION: u32 = 1;
pub const MAX_LAYOUT_NODES: usize = 256;
pub const MAX_LAYOUT_DEPTH: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLayout {
    pub format_version: u32,
    pub root: LayoutNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutNode {
    pub id: String,
    pub width: LayoutExtent,
    pub height: LayoutExtent,
    pub content: LayoutContent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayoutExtent {
    #[default]
    Auto,
    Points {
        value: f32,
    },
    Weight {
        value: f32,
    },
}

/// These are semantic choices, resolved through the host's existing stylesheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutSpacing {
    None,
    Compact,
    Normal,
    Relaxed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAlign {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutTab {
    pub id: String,
    pub title: String,
    pub child: LayoutNode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LayoutContent {
    Row {
        gap: LayoutSpacing,
        align: LayoutAlign,
        children: Vec<LayoutNode>,
    },
    Column {
        gap: LayoutSpacing,
        align: LayoutAlign,
        children: Vec<LayoutNode>,
    },
    Group {
        title: String,
        collapsible: bool,
        children: Vec<LayoutNode>,
    },
    Tabs {
        tabs: Vec<LayoutTab>,
    },
    Split {
        axis: LayoutAxis,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
    Parameter {
        parameter: String,
        control: ParameterControl,
    },
    Preview,
    Timeline,
    Note {
        text: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterControl {
    Auto,
    Number,
    Vector,
    Color,
    Checkbox,
    Dropdown,
}
impl ParameterControl {
    pub fn for_kind(kind: ParamKind) -> Option<Self> {
        Some(match kind {
            ParamKind::Float => Self::Number,
            ParamKind::Vec2 | ParamKind::Vec3 => Self::Vector,
            ParamKind::Color => Self::Color,
            ParamKind::Bool => Self::Checkbox,
            ParamKind::Enum => Self::Dropdown,
            ParamKind::Curve => return None,
        })
    }
}

fn invalid(node: &str, field: &str, cause: &str) -> Error {
    Error::Invalid(format!(
        "native layout node={} field={field}: {cause}",
        node.chars().take(128).collect::<String>()
    ))
}
fn check(node: &str, field: &str, ok: bool, cause: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(invalid(node, field, cause))
    }
}
fn title(node: &str, field: &str, value: &str, limit: usize) -> Result<()> {
    check(
        node,
        field,
        !value.trim().is_empty()
            && value.len() <= limit
            && !value
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "text must be visible and within the field byte limit",
    )
}
fn extent(node: &str, field: &str, extent: LayoutExtent) -> Result<()> {
    match extent {
        LayoutExtent::Auto => Ok(()),
        LayoutExtent::Points { value } | LayoutExtent::Weight { value } => check(
            node,
            field,
            value.is_finite() && value > 0. && value <= 8192.,
            "extent must be finite and in (0,8192]",
        ),
    }
}

impl NativeLayout {
    /// Drafts may contain unresolved parameter bindings. Saving must not require
    /// compiling the plugin or replacing an incomplete binding with a fallback.
    pub fn validate_structure(&self) -> Result<()> {
        ensure(
            self.format_version == LAYOUT_VERSION,
            "unsupported native layout version",
        )?;
        let mut budget = Budget::default();
        budget.visit(&self.root, 1)
    }

    /// Preview/publication checks against the authoritative parameter contract.
    /// Repeated bindings are valid; they are views of one value, not extra state.
    pub fn validate_bindings(&self, params: &[ParamDefinition]) -> Result<()> {
        self.validate_structure()?;
        let mut ids = BTreeSet::new();
        ensure(
            params.len() <= crate::MAX_PARAMS
                && params.iter().all(|p| valid_id(&p.id) && ids.insert(&p.id)),
            "invalid native layout parameter table",
        )?;
        fn visit(node: &LayoutNode, params: &[ParamDefinition]) -> Result<()> {
            if let LayoutContent::Parameter { parameter, control } = &node.content {
                let param = params
                    .iter()
                    .find(|p| p.id == *parameter)
                    .ok_or_else(|| invalid(&node.id, "parameter", "public parameter missing"))?;
                let expected = ParameterControl::for_kind(param.kind).ok_or_else(|| {
                    invalid(&node.id, "control", "parameter type has no layout control")
                })?;
                check(
                    &node.id,
                    "control",
                    *control == ParameterControl::Auto || *control == expected,
                    "control type does not match the public parameter",
                )?;
                check(
                    &node.id,
                    "parameter",
                    param.implemented,
                    "parameter is unavailable",
                )?;
            }
            for child in node.children() {
                visit(child, params)?;
            }
            Ok(())
        }
        visit(&self.root, params)
    }
}

impl LayoutNode {
    pub fn children(&self) -> LayoutChildren<'_> {
        match &self.content {
            LayoutContent::Row { children, .. }
            | LayoutContent::Column { children, .. }
            | LayoutContent::Group { children, .. } => LayoutChildren::Nodes(children.iter()),
            LayoutContent::Tabs { tabs } => LayoutChildren::Tabs(tabs.iter()),
            LayoutContent::Split { first, second, .. } => {
                LayoutChildren::Split([first.as_ref(), second.as_ref()].into_iter())
            }
            _ => LayoutChildren::Empty,
        }
    }
}

pub enum LayoutChildren<'a> {
    Nodes(std::slice::Iter<'a, LayoutNode>),
    Tabs(std::slice::Iter<'a, LayoutTab>),
    Split(std::array::IntoIter<&'a LayoutNode, 2>),
    Empty,
}
impl<'a> Iterator for LayoutChildren<'a> {
    type Item = &'a LayoutNode;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Nodes(nodes) => nodes.next(),
            Self::Tabs(tabs) => tabs.next().map(|tab| &tab.child),
            Self::Split(children) => children.next(),
            Self::Empty => None,
        }
    }
}

#[derive(Default)]
struct Budget {
    ids: BTreeSet<String>,
    nodes: usize,
    preview: bool,
    timeline: bool,
}
impl Budget {
    fn identity(&mut self, id: &str) -> Result<()> {
        self.nodes += 1;
        check(
            id,
            "id",
            self.nodes <= MAX_LAYOUT_NODES,
            "layout exceeds 256 components",
        )?;
        check(
            id,
            "id",
            valid_id(id) && self.ids.insert(id.into()),
            "invalid or duplicate component ID",
        )
    }
    fn visit(&mut self, node: &LayoutNode, depth: usize) -> Result<()> {
        check(
            &node.id,
            "content",
            depth <= MAX_LAYOUT_DEPTH,
            "layout exceeds depth 16",
        )?;
        self.identity(&node.id)?;
        extent(&node.id, "width", node.width)?;
        extent(&node.id, "height", node.height)?;
        match &node.content {
            LayoutContent::Row { children, .. }
            | LayoutContent::Column { children, .. }
            | LayoutContent::Group { children, .. } => {
                check(
                    &node.id,
                    "children",
                    children.len() <= MAX_LAYOUT_NODES,
                    "too many children",
                )?;
                if let LayoutContent::Group { title: text, .. } = &node.content {
                    title(&node.id, "title", text, 256)?;
                }
            }
            LayoutContent::Tabs { tabs } => {
                check(&node.id, "tabs", tabs.len() <= 32, "too many tabs")?;
                for tab in tabs {
                    self.identity(&tab.id)?;
                    title(&tab.id, "title", &tab.title, 256)?;
                }
            }
            LayoutContent::Split { ratio, .. } => {
                check(
                    &node.id,
                    "ratio",
                    ratio.is_finite() && (0.05..=0.95).contains(ratio),
                    "split ratio must be in [0.05,0.95]",
                )?;
            }
            LayoutContent::Parameter { parameter, .. } => {
                check(
                    &node.id,
                    "parameter",
                    valid_id(parameter),
                    "invalid public parameter ID",
                )?;
            }
            LayoutContent::Preview => {
                check(
                    &node.id,
                    "content",
                    !self.preview,
                    "preview slot must be unique",
                )?;
                self.preview = true;
            }
            LayoutContent::Timeline => {
                check(
                    &node.id,
                    "content",
                    !self.timeline,
                    "timeline slot must be unique",
                )?;
                self.timeline = true;
            }
            LayoutContent::Note { text } => title(&node.id, "text", text, 2048)?,
        }
        for child in node.children() {
            self.visit(child, depth + 1)?;
        }
        Ok(())
    }
}
