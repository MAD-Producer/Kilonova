//! One description of shape contents for inspectors and timeline rows.
use super::*;
use motion_core::vector::groups::OptionsEdit;
use motion_core::{
    vector::{shape_parameters, StrokeDashes, TrimMode},
    Easing,
};

#[derive(Clone)]
pub(in crate::panels) struct Parameter {
    pub id: String,
    pub name: String,
    pub values: Vec<f32>,
    authored_values: [f32; 4],
    pub keys: Vec<(i32, Easing)>,
    pub range: [f64; 2],
    pub discrete: bool,
    pub color: bool,
    pub static_value: bool,
}
impl Parameter {
    pub fn animated(&self) -> bool {
        !self.keys.is_empty()
    }
    pub fn value(&self) -> ParameterValue {
        value(&self.values)
    }
    pub fn set(&self, object: u64, item: u64, frame: u32, animated: Option<bool>) -> Command {
        Command::Vector {
            object,
            action: if self.static_value {
                VectorAction::EditGroupStructure {
                    edit: StructureEdit::Options {
                        item,
                        edit: OptionsEdit::Miter {
                            value: self.values[0],
                        },
                    },
                }
            } else {
                VectorAction::SetGroupParameter {
                    item,
                    parameter: self.id.clone(),
                    frame,
                    value: self.value(),
                    animated,
                }
            },
        }
    }
}
fn scalar(
    id: &str,
    zh: &str,
    en: &str,
    t: &Track<f32>,
    at: f64,
    range: [f64; 2],
    locale: Locale,
) -> motion_core::Result<Parameter> {
    Ok(Parameter {
        id: id.into(),
        name: if locale == Locale::Zh { zh } else { en }.into(),
        values: vec![t.sample(at)?],
        authored_values: [t.value, 0., 0., 0.],
        keys: t
            .keys
            .iter()
            .map(|k| {
                (
                    k.frame,
                    Easing {
                        ease: k.ease,
                        curve: k.curve,
                    },
                )
            })
            .collect(),
        range,
        discrete: false,
        color: false,
        static_value: false,
    })
}
fn vector(
    id: &str,
    zh: &str,
    en: &str,
    t: &Track<[f32; 2]>,
    at: f64,
    range: [f64; 2],
    locale: Locale,
) -> motion_core::Result<Parameter> {
    Ok(Parameter {
        id: id.into(),
        name: if locale == Locale::Zh { zh } else { en }.into(),
        values: t.sample(at)?.to_vec(),
        authored_values: [t.value[0], t.value[1], 0., 0.],
        keys: t
            .keys
            .iter()
            .map(|k| {
                (
                    k.frame,
                    Easing {
                        ease: k.ease,
                        curve: k.curve,
                    },
                )
            })
            .collect(),
        range,
        discrete: false,
        color: false,
        static_value: false,
    })
}
fn color(
    id: &str,
    zh: &str,
    en: &str,
    t: &Track<[f32; 4]>,
    at: f64,
    locale: Locale,
) -> motion_core::Result<Parameter> {
    Ok(Parameter {
        id: id.into(),
        name: if locale == Locale::Zh { zh } else { en }.into(),
        values: t.sample(at)?.to_vec(),
        authored_values: t.value,
        keys: t
            .keys
            .iter()
            .map(|k| {
                (
                    k.frame,
                    Easing {
                        ease: k.ease,
                        curve: k.curve,
                    },
                )
            })
            .collect(),
        range: [0., 1.],
        discrete: false,
        color: true,
        static_value: false,
    })
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::panels) enum OptionKind {
    Composite,
    FillRule,
    Cap,
    Join,
    TrimMode,
    Dashes,
    DashPairs,
    Fill,
    Stroke,
    Trim,
}
impl OptionKind {
    pub fn edit(self, index: usize) -> OptionsEdit {
        match self {
            Self::Composite => OptionsEdit::Composite {
                value: [Composite::Below, Composite::Above][index],
            },
            Self::FillRule => OptionsEdit::FillRule {
                value: [FillRule::NonZero, FillRule::EvenOdd][index],
            },
            Self::Cap => OptionsEdit::Cap {
                value: [LineCap::Butt, LineCap::Round, LineCap::Square][index],
            },
            Self::Join => OptionsEdit::Join {
                value: [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel][index],
            },
            Self::TrimMode => OptionsEdit::TrimMode {
                value: [TrimMode::Simultaneously, TrimMode::Individually][index],
            },
            Self::Dashes => OptionsEdit::Dashes {
                enabled: index == 1,
            },
            Self::DashPairs => OptionsEdit::DashPairs {
                pairs: index as u8 + 1,
            },
            Self::Fill => OptionsEdit::Fill {
                enabled: index == 1,
            },
            Self::Stroke => OptionsEdit::Stroke {
                enabled: index == 1,
            },
            Self::Trim => OptionsEdit::Trim {
                enabled: index == 1,
            },
        }
    }
}
#[derive(Clone)]
pub(in crate::panels) struct Setting {
    pub kind: OptionKind,
    pub title: &'static str,
    pub labels: Vec<&'static str>,
    pub selected: usize,
    pub boolean: bool,
}
fn setting(kind: OptionKind, selected: usize, locale: Locale) -> Setting {
    let zh = locale == Locale::Zh;
    let (title, labels, boolean) = match kind {
        OptionKind::Composite => (
            if zh { "叠放顺序" } else { "Composite" },
            if zh {
                vec!["下方", "上方"]
            } else {
                vec!["Below", "Above"]
            },
            false,
        ),
        OptionKind::FillRule => (
            if zh { "填充规则" } else { "Fill rule" },
            if zh {
                vec!["非零环绕", "奇偶"]
            } else {
                vec!["Nonzero", "Even-odd"]
            },
            false,
        ),
        OptionKind::Cap => (
            if zh { "线帽" } else { "Line cap" },
            if zh {
                vec!["平头", "圆头", "方头"]
            } else {
                vec!["Butt", "Round", "Square"]
            },
            false,
        ),
        OptionKind::Join => (
            if zh { "连接" } else { "Line join" },
            if zh {
                vec!["尖角", "圆角", "斜角"]
            } else {
                vec!["Miter", "Round", "Bevel"]
            },
            false,
        ),
        OptionKind::TrimMode => (
            if zh { "修剪方式" } else { "Trim mode" },
            if zh {
                vec!["同时", "逐个"]
            } else {
                vec!["Simultaneously", "Individually"]
            },
            false,
        ),
        OptionKind::Dashes => (if zh { "虚线" } else { "Dashes" }, vec![], true),
        OptionKind::DashPairs => (
            if zh { "虚线组数" } else { "Dash pairs" },
            vec!["1", "2", "3"],
            false,
        ),
        OptionKind::Fill => (if zh { "填充" } else { "Fill" }, vec![], true),
        OptionKind::Stroke => (if zh { "描边" } else { "Stroke" }, vec![], true),
        OptionKind::Trim => (if zh { "修剪路径" } else { "Trim paths" }, vec![], true),
    };
    Setting {
        kind,
        title,
        labels,
        selected,
        boolean,
    }
}
#[derive(Clone)]
pub(in crate::panels) struct Node {
    pub id: u64,
    pub tag: u8,
    pub name: String,
    pub children: Vec<Node>,
    pub parameters: Vec<Parameter>,
    pub settings: Vec<Setting>,
}
impl Node {
    pub fn animated(&self) -> bool {
        self.parameters.iter().any(Parameter::animated) || self.children.iter().any(Node::animated)
    }
}

/// Use authored values, never playhead samples, for both native property trees.
/// Operators and paths are authored additions; retain their headers even when
/// every editable track is at its constructor default.
pub(in crate::panels) fn filtered_tree(
    vector: &VectorContent,
    size: [f32; 2],
    at: f64,
    locale: Locale,
    filter: crate::panels::types::PropertyFilter,
) -> Option<Node> {
    use crate::panels::types::PropertyFilter;
    fn group_defaults(group: &VectorGroup, locale: Locale) -> VectorGroup {
        VectorGroup {
            id: group.id,
            name: group.name.clone(),
            transform: Default::default(),
            items: group
                .items
                .iter()
                .map(|item| match item {
                    GroupItem::Group { group } => GroupItem::Group {
                        group: Box::new(group_defaults(group, locale)),
                    },
                    GroupItem::Geometry {
                        id, name, vector, ..
                    } => GroupItem::Geometry {
                        id: *id,
                        name: name.clone(),
                        vector: Box::new(defaults(vector, true, locale)),
                        size: Track::constant([100.; 2]),
                        position: Track::constant([0.; 2]),
                    },
                    GroupItem::Fill { .. } => super::new_item(item.id(), 1, locale),
                    GroupItem::Stroke { .. } => super::new_item(item.id(), 2, locale),
                    GroupItem::Trim { .. } => super::new_item(item.id(), 3, locale),
                    GroupItem::Repeater { .. } => super::new_item(item.id(), 4, locale),
                })
                .collect(),
        }
    }
    fn defaults(v: &VectorContent, embedded: bool, locale: Locale) -> VectorContent {
        let mut value = match &v.source {
            VectorSource::Group { group } => {
                let mut value = VectorContent::shape(motion_core::vector::ShapeKind::Rectangle);
                value.source = VectorSource::Group {
                    group: Box::new(group_defaults(group, locale)),
                };
                value
            }
            VectorSource::Shape { shape, .. } => VectorContent::shape(*shape),
            VectorSource::Paths { .. } => {
                VectorContent::shape(motion_core::vector::ShapeKind::Rectangle)
            }
        };
        if embedded {
            value.fill = None;
            value.stroke = None;
        }
        value
    }
    fn retain(n: &mut Node, default: Option<&Node>, filter: PropertyFilter) -> bool {
        n.parameters.retain(|p| {
            p.animated()
                || filter == PropertyFilter::Modified
                    && default
                        .and_then(|n| n.parameters.iter().find(|d| d.id == p.id))
                        .is_none_or(|d| p.authored_values != d.authored_values)
        });
        n.settings.retain(|s| {
            filter == PropertyFilter::Modified
                && default
                    .and_then(|n| n.settings.iter().find(|d| d.kind == s.kind))
                    .is_none_or(|d| s.selected != d.selected)
        });
        n.children.retain_mut(|c| {
            retain(
                c,
                default.and_then(|d| d.children.iter().find(|d| d.id == c.id && d.tag == c.tag)),
                filter,
            )
        });
        !n.parameters.is_empty()
            || !n.settings.is_empty()
            || !n.children.is_empty()
            || filter == PropertyFilter::Modified && n.tag == 2
    }
    if matches!(filter, PropertyFilter::Properties(_)) {
        return None;
    }
    let mut node = observe!(
        "path.parameter_tree",
        tree(vector, size, at, locale),
        "frame={at}"
    )
    .ok()?;
    if filter == PropertyFilter::All {
        return Some(node);
    }
    let baseline = (filter == PropertyFilter::Modified)
        .then(|| tree(&defaults(vector, false, locale), [100.; 2], 0., locale))
        .transpose();
    let baseline = observe!("path.parameter_defaults", baseline, "frame={at}").ok()?;
    retain(&mut node, baseline.as_ref(), filter).then_some(node)
}
fn dashes(
    out: &mut Vec<Parameter>,
    d: &StrokeDashes,
    at: f64,
    locale: Locale,
) -> motion_core::Result<()> {
    out.push(scalar(
        "dash_offset",
        "虚线偏移",
        "Dash offset",
        &d.offset,
        at,
        [-32768., 32768.],
        locale,
    )?);
    for (i, t) in d.pattern.iter().enumerate() {
        let mut p = scalar(
            &format!("dash_{i}"),
            "",
            "",
            t,
            at,
            [if i % 2 == 0 { 0.1 } else { 0. }, 32768.],
            locale,
        )?;
        p.name = format!(
            "{} {}",
            if locale == Locale::Zh {
                if i % 2 == 0 {
                    "线段"
                } else {
                    "间隔"
                }
            } else if i % 2 == 0 {
                "Dash"
            } else {
                "Gap"
            },
            i / 2 + 1
        );
        out.push(p);
    }
    Ok(())
}
fn stroke(
    out: &mut Vec<Parameter>,
    settings: &mut Vec<Setting>,
    s: &Stroke,
    at: f64,
    locale: Locale,
    embedded: bool,
) -> motion_core::Result<()> {
    out.push(color(
        if embedded { "stroke_color" } else { "color" },
        if embedded { "描边颜色" } else { "颜色" },
        if embedded { "Stroke color" } else { "Color" },
        &s.color,
        at,
        locale,
    )?);
    out.push(scalar(
        if embedded { "stroke_width" } else { "width" },
        "宽度 px",
        "Width px",
        &s.width,
        at,
        [0., 32768.],
        locale,
    )?);
    let mut m = scalar(
        "miter_limit",
        "尖角限制",
        "Miter limit",
        &Track::constant(s.miter_limit),
        at,
        [1., 100.],
        locale,
    )?;
    m.static_value = true;
    out.push(m);
    settings.extend([
        setting(OptionKind::Cap, s.cap as usize, locale),
        setting(OptionKind::Join, s.join as usize, locale),
        setting(OptionKind::Dashes, usize::from(s.dashes.is_some()), locale),
    ]);
    if let Some(d) = &s.dashes {
        settings.push(setting(
            OptionKind::DashPairs,
            d.pattern.len() / 2 - 1,
            locale,
        ));
        dashes(out, d, at, locale)?;
    }
    Ok(())
}
fn trim(
    out: &mut Vec<Parameter>,
    settings: &mut Vec<Setting>,
    t: &motion_core::vector::TrimPaths,
    at: f64,
    locale: Locale,
    embedded: bool,
) -> motion_core::Result<()> {
    for (id, zh, en, track, range) in [
        ("start", "起点 %", "Start %", &t.start, [0., 100.]),
        ("end", "终点 %", "End %", &t.end, [0., 100.]),
        (
            "offset",
            "偏移 °",
            "Offset °",
            &t.offset,
            [-360000., 360000.],
        ),
    ] {
        out.push(scalar(
            &if embedded {
                format!("trim_{id}")
            } else {
                id.into()
            },
            zh,
            en,
            track,
            at,
            range,
            locale,
        )?);
    }
    settings.push(setting(OptionKind::TrimMode, t.mode as usize, locale));
    Ok(())
}
pub(in crate::panels) fn tree(
    vector: &VectorContent,
    size: [f32; 2],
    at: f64,
    locale: Locale,
) -> motion_core::Result<Node> {
    Ok(if let VectorSource::Group { group } = &vector.source {
        group_node(group, at, locale)?
    } else {
        let g = VectorGroup {
            id: 1,
            name: "Group 1".into(),
            transform: Default::default(),
            items: vec![],
        };
        let mut root = group_node(&g, at, locale)?;
        let (parameters, settings) = geometry(
            vector,
            &Track::constant(size),
            &Track::constant([0.; 2]),
            at,
            locale,
        )?;
        root.children.push(Node {
            id: 2,
            tag: 2,
            name: if locale == Locale::Zh {
                "路径"
            } else {
                "Geometry"
            }
            .into(),
            children: vec![],
            parameters,
            settings,
        });
        root
    })
}
fn transform_parameters(
    t: &motion_core::vector::groups::GroupTransform,
    at: f64,
    locale: Locale,
) -> motion_core::Result<Vec<Parameter>> {
    Ok(vec![
        vector(
            "position",
            "位置",
            "Position",
            &t.position,
            at,
            [-32768., 32768.],
            locale,
        )?,
        vector(
            "anchor",
            "锚点",
            "Anchor",
            &t.anchor,
            at,
            [-32768., 32768.],
            locale,
        )?,
        vector(
            "scale",
            "缩放 %",
            "Scale %",
            &t.scale,
            at,
            [-10000., 10000.],
            locale,
        )?,
        scalar(
            "rotation",
            "旋转 °",
            "Rotation °",
            &t.rotation,
            at,
            [-360000., 360000.],
            locale,
        )?,
        scalar("skew", "倾斜 °", "Skew °", &t.skew, at, [-89., 89.], locale)?,
        scalar(
            "skew_axis",
            "倾斜轴 °",
            "Skew axis °",
            &t.skew_axis,
            at,
            [-360000., 360000.],
            locale,
        )?,
        scalar(
            "opacity",
            "不透明度 %",
            "Opacity %",
            &t.opacity,
            at,
            [0., 100.],
            locale,
        )?,
    ])
}
fn group_node(g: &VectorGroup, at: f64, locale: Locale) -> motion_core::Result<Node> {
    let parameters = transform_parameters(&g.transform, at, locale)?;
    let mut children = vec![Node {
        id: g.id,
        tag: 1,
        name: if locale == Locale::Zh {
            "组变换"
        } else {
            "Group transform"
        }
        .into(),
        children: vec![],
        parameters,
        settings: vec![],
    }];
    children.extend(
        g.items
            .iter()
            .map(|item| item_node(item, at, locale))
            .collect::<motion_core::Result<Vec<_>>>()?,
    );
    Ok(Node {
        id: g.id,
        tag: 0,
        name: if g.name == "Group 1" && locale == Locale::Zh {
            "组 1".into()
        } else {
            g.name.clone()
        },
        children,
        parameters: vec![],
        settings: vec![],
    })
}
fn geometry(
    v: &VectorContent,
    size: &Track<[f32; 2]>,
    position: &Track<[f32; 2]>,
    at: f64,
    locale: Locale,
) -> motion_core::Result<(Vec<Parameter>, Vec<Setting>)> {
    let mut p = vec![];
    let mut s = vec![];

    p.extend([
        vector("size", "大小 px", "Size px", size, at, [0., 32768.], locale)?,
        vector(
            "position",
            "位置",
            "Position",
            position,
            at,
            [-32768., 32768.],
            locale,
        )?,
    ]);
    if let VectorSource::Shape { shape, parameters } = &v.source {
        for spec in shape_parameters(*shape) {
            if let Some(t) = parameters.get(spec.id) {
                let zh = match spec.id {
                    "radius" => "半径",
                    "roundness" => "圆角",
                    "sides" => "边数",
                    "points" => "顶点数",
                    "inner_radius" => "内半径",
                    "outer_radius" => "外半径",
                    "start_angle" => "起始角度",
                    "end_angle" => "结束角度",
                    id => id,
                };
                let mut param = scalar(
                    &format!("shape:{}", spec.id),
                    zh,
                    spec.id,
                    t,
                    at,
                    [spec.min.into(), spec.max.into()],
                    locale,
                )?;
                param.discrete = spec.discrete;
                p.push(param);
            }
        }
    }
    s.extend([
        setting(OptionKind::Fill, usize::from(v.fill.is_some()), locale),
        setting(OptionKind::Stroke, usize::from(v.stroke.is_some()), locale),
        setting(OptionKind::Trim, usize::from(v.trim.is_some()), locale),
    ]);
    if let Some(c) = &v.fill {
        p.push(color("fill", "填充颜色", "Fill color", c, at, locale)?);
        s.push(setting(OptionKind::FillRule, v.fill_rule as usize, locale));
    }
    if let Some(st) = &v.stroke {
        stroke(&mut p, &mut s, st, at, locale, true)?;
    }
    if let Some(t) = &v.trim {
        trim(&mut p, &mut s, t, at, locale, true)?;
    }

    Ok((p, s))
}
fn item_node(item: &GroupItem, at: f64, locale: Locale) -> motion_core::Result<Node> {
    if let GroupItem::Group { group } = item {
        return group_node(group, at, locale);
    }
    let mut p = vec![];
    let mut s = vec![];
    match item {
        GroupItem::Repeater { repeater: r, .. } => {
            p = vec![
                scalar(
                    "copies",
                    "副本数",
                    "Copies",
                    &r.copies,
                    at,
                    [0., 1024.],
                    locale,
                )?,
                scalar(
                    "offset",
                    "偏移",
                    "Offset",
                    &r.offset,
                    at,
                    [-1024., 1024.],
                    locale,
                )?,
                vector(
                    "position",
                    "位置",
                    "Position",
                    &r.position,
                    at,
                    [-32768., 32768.],
                    locale,
                )?,
                vector(
                    "anchor",
                    "锚点",
                    "Anchor",
                    &r.anchor,
                    at,
                    [-32768., 32768.],
                    locale,
                )?,
                vector(
                    "scale",
                    "缩放 %",
                    "Scale %",
                    &r.scale,
                    at,
                    [-1000., 1000.],
                    locale,
                )?,
                scalar(
                    "rotation",
                    "旋转 °",
                    "Rotation °",
                    &r.rotation,
                    at,
                    [-360000., 360000.],
                    locale,
                )?,
                scalar(
                    "start_opacity",
                    "起始不透明度 %",
                    "Start opacity %",
                    &r.start_opacity,
                    at,
                    [0., 100.],
                    locale,
                )?,
                scalar(
                    "end_opacity",
                    "结束不透明度 %",
                    "End opacity %",
                    &r.end_opacity,
                    at,
                    [0., 100.],
                    locale,
                )?,
            ];
            s.push(setting(OptionKind::Composite, r.composite as usize, locale));
        }
        GroupItem::Fill {
            color: c,
            fill_rule,
            composite,
            ..
        } => {
            p.push(color("color", "颜色", "Color", c, at, locale)?);
            s.extend([
                setting(OptionKind::FillRule, *fill_rule as usize, locale),
                setting(OptionKind::Composite, *composite as usize, locale),
            ]);
        }
        GroupItem::Stroke {
            stroke: st,
            composite,
            ..
        } => {
            stroke(&mut p, &mut s, st, at, locale, false)?;
            s.push(setting(OptionKind::Composite, *composite as usize, locale));
        }
        GroupItem::Trim { trim: t, .. } => trim(&mut p, &mut s, t, at, locale, false)?,
        GroupItem::Geometry {
            vector: v,
            size,
            position,
            ..
        } => {
            (p, s) = geometry(v, size, position, at, locale)?;
        }
        _ => {}
    }
    let default_name = match item {
        GroupItem::Geometry { .. } => "Geometry",
        GroupItem::Fill { .. } => "Fill",
        GroupItem::Stroke { .. } => "Stroke",
        GroupItem::Trim { .. } => "Trim",
        _ => "",
    };
    let name = if item.name() == default_name && locale == Locale::Zh {
        match default_name {
            "Geometry" => "路径",
            "Fill" => "填充",
            "Stroke" => "描边",
            "Trim" => "修剪路径",
            _ => default_name,
        }
        .into()
    } else {
        item.name().into()
    };
    Ok(Node {
        id: item.id(),
        tag: 2,
        name,
        children: vec![],
        parameters: p,
        settings: s,
    })
}
#[cfg(test)]
pub(in crate::panels) fn lookup(node: &Node, item: u64, id: &str) -> Option<Parameter> {
    if node.id == item {
        if let Some(p) = node.parameters.iter().find(|p| p.id == id) {
            return Some(p.clone());
        }
    }
    node.children.iter().find_map(|n| lookup(n, item, id))
}

/// Lookup one edited track without building every sibling property's values.
pub(in crate::panels) fn lookup_source(
    vector: &VectorContent,
    size: [f32; 2],
    item: u64,
    id: &str,
    at: f64,
    locale: Locale,
) -> Option<Parameter> {
    fn search(g: &VectorGroup, item: u64, id: &str, at: f64, locale: Locale) -> Option<Parameter> {
        if g.id == item {
            return observe!(
                "path.lookup_transform",
                transform_parameters(&g.transform, at, locale),
                "item={item} param={id} frame={at}"
            )
            .ok()?
            .into_iter()
            .find(|p| p.id == id);
        }
        for i in &g.items {
            if let GroupItem::Group { group } = i {
                if let Some(p) = search(group, item, id, at, locale) {
                    return Some(p);
                }
            } else if i.id() == item {
                return observe!(
                    "path.lookup_item",
                    item_node(i, at, locale),
                    "item={item} param={id} frame={at}"
                )
                .ok()?
                .parameters
                .into_iter()
                .find(|p| p.id == id);
            }
        }
        None
    }
    if let VectorSource::Group { group } = &vector.source {
        return search(group, item, id, at, locale);
    }
    match item {
        1 => observe!(
            "path.lookup_root",
            transform_parameters(&Default::default(), at, locale),
            "item={item} param={id} frame={at}"
        )
        .ok()?
        .into_iter()
        .find(|p| p.id == id),
        2 => observe!(
            "path.lookup_geometry",
            geometry(
                vector,
                &Track::constant(size),
                &Track::constant([0.; 2]),
                at,
                locale,
            ),
            "item={item} param={id} frame={at}"
        )
        .ok()?
        .0
        .into_iter()
        .find(|p| p.id == id),
        _ => None,
    }
}

#[cfg(test)]
mod modified_tests {
    use super::*;
    use crate::panels::types::PropertyFilter as F;

    #[test]
    fn shared_tree_keeps_authored_operators_and_only_changed_or_keyed_values() {
        let mut content = VectorContent::shape(motion_core::vector::ShapeKind::Rectangle);
        let mut repeater = Repeater::default();
        repeater.copies.value = 5.;
        repeater
            .rotation
            .upsert(0, 0., motion_core::Ease::Linear)
            .unwrap();
        let mut group = VectorGroup {
            id: 1,
            name: "Root".into(),
            transform: Default::default(),
            items: vec![
                super::super::new_item(2, 1, Locale::En),
                GroupItem::Repeater {
                    id: 3,
                    name: "Repeat".into(),
                    repeater,
                },
            ],
        };
        group.transform.position.value = [1., 0.];
        content.source = VectorSource::Group {
            group: Box::new(group),
        };
        let before = content.clone();
        for at in [0., 40., 119.] {
            let modified = filtered_tree(&content, [100.; 2], at, Locale::En, F::Modified).unwrap();
            let transform = &modified.children[0];
            assert_eq!(
                transform
                    .parameters
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>(),
                ["position"]
            );
            let fill = &modified.children[1];
            assert_eq!(fill.id, 2);
            assert!(fill.parameters.is_empty() && fill.settings.is_empty());
            let repeat = &modified.children[2];
            assert_eq!(
                repeat
                    .parameters
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>(),
                ["copies", "rotation"]
            );
            let animated = filtered_tree(&content, [100.; 2], at, Locale::En, F::Animated).unwrap();
            assert_eq!(animated.children.len(), 1);
            assert_eq!(animated.children[0].id, 3);
            assert_eq!(
                animated.children[0]
                    .parameters
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>(),
                ["rotation"]
            );
        }
        assert_eq!(content, before);
        let VectorSource::Group { group } = &mut content.source else {
            panic!()
        };
        group.transform.position.value = [0.; 2];
        let GroupItem::Repeater { repeater, .. } = &mut group.items[1] else {
            panic!()
        };
        repeater.copies.value = 3.;
        repeater.rotation.keys.clear();
        let reset = filtered_tree(&content, [100.; 2], 0., Locale::En, F::Modified).unwrap();
        assert_eq!(reset.children.len(), 2);
        assert!(reset.children.iter().all(|n| n.parameters.is_empty()));
        assert!(filtered_tree(&content, [100.; 2], 0., Locale::En, F::Animated).is_none());
    }
}
