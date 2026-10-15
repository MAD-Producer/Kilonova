//! Editable vector contents, using the same numeric transaction widget as effects.
use super::*;
use motion_core::vector::groups::{Composite, GroupItem, ParameterValue, Repeater, VectorGroup};
use motion_ui::{
    dock::Panel,
    icons::{self, Icon},
    widgets::NumberOptions,
};
use std::collections::HashSet;

struct Field {
    host: Panel,
    item: u64,
    parameter: &'static str,
    axis: usize,
    number: crate::panels::numeric::Field,
}

#[derive(Default)]
pub(in crate::panels) struct Controls {
    context: Option<(String, String, u64, u32)>,
    field: Option<Field>,
    open: HashSet<u64>,
    transforms: HashSet<u64>,
    initialized: bool,
    pub(in crate::panels) cursor: Option<(Rect, bool)>,
}
struct Parameter {
    id: &'static str,
    zh: &'static str,
    en: &'static str,
    values: Vec<f32>,
    animated: bool,
    min: f64,
    max: f64,
}
fn scalar(
    id: &'static str,
    zh: &'static str,
    en: &'static str,
    track: &Track<f32>,
    local: f64,
    min: f64,
    max: f64,
) -> Parameter {
    Parameter {
        id,
        zh,
        en,
        values: vec![track.sample(local)],
        animated: track.is_animated(),
        min,
        max,
    }
}
fn vector(
    id: &'static str,
    zh: &'static str,
    en: &'static str,
    track: &Track<[f32; 2]>,
    local: f64,
    min: f64,
    max: f64,
) -> Parameter {
    Parameter {
        id,
        zh,
        en,
        values: track.sample(local).to_vec(),
        animated: track.is_animated(),
        min,
        max,
    }
}
fn value(values: &[f32]) -> ParameterValue {
    if values.len() == 2 {
        ParameterValue::Vector([values[0], values[1]])
    } else {
        ParameterValue::Scalar(values[0])
    }
}
fn replace(object: u64, vector: VectorContent) -> Action {
    Action::Edit(Command::Vector {
        object,
        action: VectorAction::Replace { vector },
    })
}
fn find(group: &mut VectorGroup, id: u64) -> Option<&mut VectorGroup> {
    if group.id == id {
        return Some(group);
    }
    group.items.iter_mut().find_map(|item| {
        if let GroupItem::Group { group } = item {
            find(group, id)
        } else {
            None
        }
    })
}
fn editable(group: &VectorGroup, id: u64) -> bool {
    group.id == id
        || group.items.iter().any(|item| match item {
            GroupItem::Group { group } => editable(group, id),
            GroupItem::Repeater { id: item_id, .. } => *item_id == id,
            _ => false,
        })
}
fn maximum(group: &VectorGroup) -> u64 {
    group
        .items
        .iter()
        .map(|item| match item {
            GroupItem::Group { group } => maximum(group),
            _ => item.id(),
        })
        .fold(group.id, u64::max)
}
pub(super) fn add_repeater(
    vector: &VectorContent,
    size: [f32; 2],
    parent: Option<u64>,
    locale: Locale,
) -> Option<(VectorContent, u64)> {
    let mut next = if matches!(vector.source, VectorSource::Group { .. }) {
        vector.clone()
    } else {
        VectorGroup::wrap(vector.clone(), size)
    };
    let VectorSource::Group { group } = &mut next.source else {
        return None;
    };
    let id = maximum(group).checked_add(1)?;
    let root = group.id;
    let target = find(group, parent.unwrap_or(root))?;
    target.items.push(GroupItem::Repeater {
        id,
        name: if locale == Locale::Zh {
            format!("中继器 {id}")
        } else {
            format!("Repeater {id}")
        },
        repeater: Repeater::default(),
    });
    next.validate().ok()?;
    Some((next, id))
}
fn remove(group: &mut VectorGroup, id: u64) -> bool {
    if let Some(index) = group
        .items
        .iter()
        .position(|item| matches!(item, GroupItem::Repeater { .. }) && item.id() == id)
    {
        group.items.remove(index);
        return true;
    }
    group.items.iter_mut().any(|item| match item {
        GroupItem::Group { group } => remove(group, id),
        _ => false,
    })
}
fn composite(group: &mut VectorGroup, id: u64) -> bool {
    for item in &mut group.items {
        match item {
            GroupItem::Repeater {
                id: item_id,
                repeater,
                ..
            } if *item_id == id => {
                repeater.composite = if repeater.composite == Composite::Below {
                    Composite::Above
                } else {
                    Composite::Below
                };
                return true;
            }
            GroupItem::Group { group } => {
                if composite(group, id) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}
fn icon(
    list: &mut PaintList,
    input: &Input,
    rect: Rect,
    kind: Icon,
    active: bool,
    enabled: bool,
) -> bool {
    let colors = list.styles().colors();
    if enabled && rect.contains(input.mouse) {
        paint::rect(list, rect, colors.SUNKEN);
    }
    icons::draw(
        list,
        rect,
        kind,
        13.,
        if !enabled {
            colors.TEXT_MUTED.with_alpha(0.4)
        } else if active {
            colors.ACCENT
        } else {
            colors.TEXT_MUTED
        },
    );
    enabled && click(input, rect)
}
impl Controls {
    pub(in crate::panels) fn editing(&self) -> bool {
        self.field.is_some()
    }
    pub(in crate::panels) fn cancel(&mut self) -> bool {
        self.field
            .take()
            .is_some_and(|mut field| field.number.cancel())
    }
    fn finish(&mut self, out: &mut Vec<Action>) {
        if self.cancel() {
            out.push(Action::History(3));
        }
    }
    pub(in crate::panels) fn height(&self, layer: &Layer) -> f32 {
        fn group_height(
            group: &VectorGroup,
            open: &HashSet<u64>,
            transforms: &HashSet<u64>,
        ) -> f32 {
            if !open.contains(&group.id) {
                return 26.;
            }
            52. + if transforms.contains(&group.id) {
                7. * 26.
            } else {
                0.
            } + group
                .items
                .iter()
                .map(|item| match item {
                    GroupItem::Group { group } => group_height(group, open, transforms),
                    GroupItem::Repeater { id, .. } => {
                        26. + if open.contains(id) { 9. * 26. } else { 0. }
                    }
                    _ => 26.,
                })
                .sum::<f32>()
        }
        match &layer.content {
            Content::Vector { vector } => {
                30. + match &vector.source {
                    VectorSource::Group { group } => {
                        group_height(group, &self.open, &self.transforms)
                    }
                    _ => 0.,
                }
            }
            _ => 0.,
        }
    }
    pub(in crate::panels) fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        let Content::Vector { vector } = &layer.content else {
            if self.cancel() {
                out.push(Action::History(4));
            }
            self.cursor = None;
            return;
        };
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let context = (
            state["root"].as_str().unwrap_or("").into(),
            state["composition"].as_str().unwrap_or("").into(),
            layer.id,
            frame,
        );
        if self.context.as_ref() != Some(&context) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            if self.context.as_ref().map(|c| (&c.0, &c.1, c.2))
                != Some((&context.0, &context.1, context.2))
            {
                self.open.clear();
                self.transforms.clear();
                self.initialized = false;
            }
            self.context = Some(context);
        }
        if layer.locked || input.focus_lost {
            if self.cancel() {
                out.push(Action::History(4));
            }
        }
        if self.field.as_ref().is_some_and(|field| !matches!(&vector.source,VectorSource::Group {group} if editable(group,field.item))) {
            if self.cancel(){out.push(Action::History(4));}
        }
        self.cursor = None;
        let colors = list.styles().colors();
        label(
            list,
            atlas,
            Rect::new(area.min[0] + 12., *y, (area.width() - 140.).max(1.), 26.),
            if locale == Locale::Zh {
                "矢量内容"
            } else {
                "Vector contents"
            },
            colors.TEXT,
        );
        let styles = list.styles();
        if ui::button_styled(
            list,
            atlas,
            Rect::new(area.max[0] - 126., *y, 114., 24.),
            if locale == Locale::Zh {
                "添加中继器"
            } else {
                "Add repeater"
            },
            input,
            !layer.locked,
            &styles,
            "button.icon",
        )
        .clicked
        {
            self.finish(out);
            if let Some((next, id)) = add_repeater(vector, layer.size, None, locale) {
                if let VectorSource::Group { group } = &next.source {
                    self.open.insert(group.id);
                }
                self.open.insert(id);
                self.initialized = true;
                out.push(replace(layer.id, next));
            }
        }
        *y += 30.;
        if let VectorSource::Group { group } = &vector.source {
            if !self.initialized {
                self.open.insert(group.id);
                self.initialized = true;
            }
            let local = f64::from(frame)
                - f64::from(
                    layer
                        .clip(state["project"]["frames"].as_u64().unwrap_or(1) as u32)
                        .offset_frame,
                );
            self.group(
                list, atlas, area, y, layer, vector, group, local, frame, input, locale, host, 0,
                out,
            );
        }
    }
    fn group(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        layer: &Layer,
        vector_data: &VectorContent,
        group: &VectorGroup,
        local: f64,
        frame: u32,
        input: &Input,
        locale: Locale,
        host: Panel,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let heading = Rect::new(
            area.min[0] + 16. + depth as f32 * 12.,
            *y,
            (area.width() - 54. - depth as f32 * 12.).max(1.),
            24.,
        );
        self.heading(list, atlas, heading, &group.name, group.id, input, out);
        if icon(
            list,
            input,
            Rect::new(area.max[0] - 34., *y, 22., 24.),
            Icon::Plus,
            false,
            !layer.locked,
        ) {
            self.finish(out);
            if let Some((next, id)) = add_repeater(vector_data, layer.size, Some(group.id), locale)
            {
                self.open.insert(group.id);
                self.open.insert(id);
                out.push(replace(layer.id, next));
            }
        }
        *y += 26.;
        if !self.open.contains(&group.id) {
            return;
        }
        let transform_rect = Rect::new(
            area.min[0] + 28. + depth as f32 * 12.,
            *y,
            (area.width() - 54. - depth as f32 * 12.).max(1.),
            24.,
        );
        icons::draw(
            list,
            Rect::new(transform_rect.min[0], *y, 18., 24.),
            if self.transforms.contains(&group.id) {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            12.,
            list.styles().colors().TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(
                transform_rect.min[0] + 20.,
                *y,
                (transform_rect.width() - 20.).max(1.),
                24.,
            ),
            if locale == Locale::Zh {
                "变换"
            } else {
                "Transform"
            },
            list.styles().colors().TEXT_MUTED,
        );
        if click(input, transform_rect) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            if !self.transforms.remove(&group.id) {
                self.transforms.insert(group.id);
            }
        }
        *y += 26.;
        if self.transforms.contains(&group.id) {
            let t = &group.transform;
            for parameter in [
                vector(
                    "position",
                    "位置",
                    "Position",
                    &t.position,
                    local,
                    -32768.,
                    32768.,
                ),
                vector(
                    "anchor", "锚点", "Anchor", &t.anchor, local, -32768., 32768.,
                ),
                vector(
                    "scale", "缩放 %", "Scale %", &t.scale, local, -10000., 10000.,
                ),
                scalar(
                    "rotation",
                    "旋转 °",
                    "Rotation °",
                    &t.rotation,
                    local,
                    -360000.,
                    360000.,
                ),
                scalar("skew", "倾斜 °", "Skew °", &t.skew, local, -89., 89.),
                scalar(
                    "skew_axis",
                    "倾斜轴 °",
                    "Skew axis °",
                    &t.skew_axis,
                    local,
                    -360000.,
                    360000.,
                ),
                scalar(
                    "opacity",
                    "不透明度 %",
                    "Opacity %",
                    &t.opacity,
                    local,
                    0.,
                    100.,
                ),
            ] {
                self.parameter(
                    list,
                    atlas,
                    area,
                    y,
                    layer.id,
                    group.id,
                    parameter,
                    frame,
                    input,
                    locale,
                    host,
                    !layer.locked,
                    depth + 1,
                    out,
                );
            }
        }
        for item in &group.items {
            match item {
                GroupItem::Group { group } => self.group(
                    list,
                    atlas,
                    area,
                    y,
                    layer,
                    vector_data,
                    group,
                    local,
                    frame,
                    input,
                    locale,
                    host,
                    depth + 1,
                    out,
                ),
                GroupItem::Repeater {
                    id,
                    name,
                    repeater: r,
                } => {
                    let heading = Rect::new(
                        area.min[0] + 28. + depth as f32 * 12.,
                        *y,
                        (area.width() - 66. - depth as f32 * 12.).max(1.),
                        24.,
                    );
                    self.heading(list, atlas, heading, name, *id, input, out);
                    if icon(
                        list,
                        input,
                        Rect::new(area.max[0] - 34., *y, 22., 24.),
                        Icon::Close,
                        false,
                        !layer.locked,
                    ) {
                        self.finish(out);
                        let mut next = vector_data.clone();
                        if let VectorSource::Group { group } = &mut next.source {
                            if remove(group, *id) {
                                out.push(replace(layer.id, next));
                            }
                        }
                    }
                    *y += 26.;
                    if !self.open.contains(id) {
                        continue;
                    }
                    for parameter in [
                        scalar("copies", "副本数", "Copies", &r.copies, local, 0., 1024.),
                        scalar("offset", "偏移", "Offset", &r.offset, local, -1024., 1024.),
                        vector(
                            "position",
                            "位置",
                            "Position",
                            &r.position,
                            local,
                            -32768.,
                            32768.,
                        ),
                        vector(
                            "anchor", "锚点", "Anchor", &r.anchor, local, -32768., 32768.,
                        ),
                        vector("scale", "缩放 %", "Scale %", &r.scale, local, -1000., 1000.),
                        scalar(
                            "rotation",
                            "旋转 °",
                            "Rotation °",
                            &r.rotation,
                            local,
                            -360000.,
                            360000.,
                        ),
                        scalar(
                            "start_opacity",
                            "起始不透明度 %",
                            "Start opacity %",
                            &r.start_opacity,
                            local,
                            0.,
                            100.,
                        ),
                        scalar(
                            "end_opacity",
                            "结束不透明度 %",
                            "End opacity %",
                            &r.end_opacity,
                            local,
                            0.,
                            100.,
                        ),
                    ] {
                        self.parameter(
                            list,
                            atlas,
                            area,
                            y,
                            layer.id,
                            *id,
                            parameter,
                            frame,
                            input,
                            locale,
                            host,
                            !layer.locked,
                            depth + 2,
                            out,
                        );
                    }
                    let caption = match (locale == Locale::Zh, r.composite) {
                        (true, Composite::Above) => "副本叠放：上方",
                        (true, Composite::Below) => "副本叠放：下方",
                        (false, Composite::Above) => "Composite copies: above",
                        _ => "Composite copies: below",
                    };
                    let rect = Rect::new(area.min[0] + 38., *y, (area.width() - 54.).max(1.), 24.);
                    let styles = list.styles();
                    if ui::button_styled(
                        list,
                        atlas,
                        rect,
                        caption,
                        input,
                        !layer.locked,
                        &styles,
                        "button.icon",
                    )
                    .clicked
                    {
                        self.finish(out);
                        let mut next = vector_data.clone();
                        if let VectorSource::Group { group } = &mut next.source {
                            if composite(group, *id) {
                                out.push(replace(layer.id, next));
                            }
                        }
                    }
                    *y += 26.;
                }
                _ => {
                    let kind = match item {
                        GroupItem::Geometry { .. } => {
                            if locale == Locale::Zh {
                                "路径"
                            } else {
                                "Geometry"
                            }
                        }
                        GroupItem::Fill { .. } => {
                            if locale == Locale::Zh {
                                "填充"
                            } else {
                                "Fill"
                            }
                        }
                        GroupItem::Stroke { .. } => {
                            if locale == Locale::Zh {
                                "描边"
                            } else {
                                "Stroke"
                            }
                        }
                        _ => {
                            if locale == Locale::Zh {
                                "修剪"
                            } else {
                                "Trim"
                            }
                        }
                    };
                    label(
                        list,
                        atlas,
                        Rect::new(
                            area.min[0] + 38. + depth as f32 * 12.,
                            *y,
                            (area.width() - 54. - depth as f32 * 12.).max(1.),
                            24.,
                        ),
                        &format!("{} · {kind}", item.name()),
                        list.styles().colors().TEXT_MUTED,
                    );
                    *y += 26.;
                }
            }
        }
    }
    fn heading(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        rect: Rect,
        name: &str,
        id: u64,
        input: &Input,
        out: &mut Vec<Action>,
    ) {
        icons::draw(
            list,
            Rect::new(rect.min[0], rect.min[1], 18., 24.),
            if self.open.contains(&id) {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            12.,
            list.styles().colors().TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(
                rect.min[0] + 20.,
                rect.min[1],
                (rect.width() - 20.).max(1.),
                24.,
            ),
            name,
            list.styles().colors().TEXT,
        );
        if click(input, rect) {
            if self.cancel() {
                out.push(Action::History(4));
            }
            if !self.open.remove(&id) {
                self.open.insert(id);
            }
        }
    }
    fn parameter(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: &mut f32,
        object: u64,
        item: u64,
        p: Parameter,
        frame: u32,
        input: &Input,
        locale: Locale,
        host: Panel,
        enabled: bool,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let indent = (26. + depth as f32 * 12.).min(area.width() * 0.22);
        let options = NumberOptions {
            min: p.min,
            max: p.max,
            ..Default::default()
        };
        if icon(
            list,
            input,
            Rect::new(area.min[0] + indent, *y, 18., 24.),
            Icon::Stopwatch,
            p.animated,
            enabled,
        ) {
            self.finish(out);
            out.push(Action::Edit(Command::Vector {
                object,
                action: VectorAction::SetGroupParameter {
                    item,
                    parameter: p.id.into(),
                    frame,
                    value: value(&p.values),
                    animated: Some(!p.animated),
                },
            }));
        }
        label(
            list,
            atlas,
            Rect::new(
                area.min[0] + indent + 20.,
                *y,
                (area.width() * 0.55 - indent - 24.).max(1.),
                24.,
            ),
            if locale == Locale::Zh { p.zh } else { p.en },
            list.styles().colors().TEXT_MUTED,
        );
        let width = (area.width() * 0.43 - 12.).max(2.) / p.values.len() as f32;
        for (axis, current) in p.values.iter().copied().enumerate() {
            let rect = Rect::new(
                area.min[0] + area.width() * 0.55 + axis as f32 * width,
                *y,
                (width - 2.).max(1.),
                24.,
            );
            let active = self.field.as_ref().is_some_and(|f| {
                f.host == host && f.item == item && f.parameter == p.id && f.axis == axis
            });
            if enabled && click(input, rect) && !active {
                self.finish(out);
                self.field = Some(Field {
                    host,
                    item,
                    parameter: p.id,
                    axis,
                    number: Default::default(),
                });
                out.push(Action::Seek(f64::from(frame)));
            }
            if self.field.as_ref().is_some_and(|f| {
                f.host == host && f.item == item && f.parameter == p.id && f.axis == axis
            }) {
                let field = self.field.as_mut().unwrap();
                let events = field
                    .number
                    .draw(list, atlas, rect, current as f64, options, input);
                if field.number.editor.dragging() || rect.contains(input.mouse) {
                    self.cursor = Some((rect, field.number.editor.dragging()));
                }
                let finished = crate::panels::numeric::apply(events, out, |new| {
                    let mut values = p.values.clone();
                    values[axis] = new;
                    Command::Vector {
                        object,
                        action: VectorAction::SetGroupParameter {
                            item,
                            parameter: p.id.into(),
                            frame,
                            value: value(&values),
                            animated: None,
                        },
                    }
                });
                if finished {
                    self.field = None;
                }
            } else {
                crate::panels::numeric::passive_with_precision(
                    list,
                    atlas,
                    rect,
                    current as f64,
                    input,
                    enabled,
                    2,
                );
                if enabled && rect.contains(input.mouse) {
                    self.cursor = Some((rect, false));
                }
            }
        }
        *y += 26.;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> motion_core::Engine {
        let mut project = motion_core::Project::new(640, 360, 30, 120).unwrap();
        let mut layer = Layer::solid(7, "Vector", [100.; 2], [320., 180., 0.], [1.; 4]);
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 12,
            out_frame: 120,
            offset_frame: 12,
        });
        let original = super::super::new_vector(VectorPath {
            id: 1,
            closed: true,
            nodes: [[0., 0.], [100., 0.], [0., 100.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        });
        layer.content = Content::Vector { vector: original };
        project.layers.push(layer);
        motion_core::Engine::new(project).unwrap()
    }
    fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Edit(command) => engine.apply(command).unwrap(),
                Action::History(2) => engine.begin_gesture().unwrap(),
                Action::History(3) => engine.end_gesture(true).unwrap(),
                Action::History(4) => engine.end_gesture(false).unwrap(),
                Action::Seek(_) => {}
                _ => panic!("unexpected vector action"),
            }
        }
    }
    fn draw(
        controls: &mut Controls,
        engine: &motion_core::Engine,
        input: &Input,
        host: Panel,
        atlas: &mut TextAtlas,
    ) -> Vec<Action> {
        let mut actions = vec![];
        let mut y = 0.;
        let state = serde_json::json!({"root":"fixture","composition":motion_core::composition::MAIN_COMPOSITION,"frame":33,"project":engine.project()});
        controls.render(
            &mut PaintList::default(),
            atlas,
            Rect::new(0., 0., 400., 1000.),
            &mut y,
            &engine.project().layers[0],
            &state,
            input,
            Locale::Zh,
            host,
            &mut actions,
        );
        actions
    }
    fn source(engine: &motion_core::Engine) -> &VectorContent {
        let Content::Vector { vector } = &engine.project().layers[0].content else {
            panic!()
        };
        vector
    }
    fn repeater(engine: &motion_core::Engine) -> &Repeater {
        let VectorSource::Group { group } = &source(engine).source else {
            panic!()
        };
        let GroupItem::Repeater { repeater, .. } = &group.items[1] else {
            panic!()
        };
        repeater
    }
    fn press(x: f32, y: f32) -> Input {
        Input {
            mouse: [x, y],
            pressed: Some((MouseButton::Left, [x, y])),
            held: Some(MouseButton::Left),
            ..Default::default()
        }
    }
    #[test]
    fn add_repeater_from_ui_preserves_original_geometry_and_style_and_roundtrips() {
        let mut engine = engine();
        let before = engine.project().clone();
        let mut controls = Controls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let actions = draw(
            &mut controls,
            &engine,
            &press(378., 12.),
            Panel::Properties,
            &mut atlas,
        );
        assert!(matches!(
            &actions[..],
            [Action::Edit(Command::Vector {
                action: VectorAction::Replace { .. },
                ..
            })]
        ));
        apply(&mut engine, actions);
        let VectorSource::Group { group } = &source(&engine).source else {
            panic!()
        };
        let GroupItem::Geometry { vector, size, .. } = &group.items[0] else {
            panic!()
        };
        let Content::Vector { vector: original } = &before.layers[0].content else {
            panic!()
        };
        assert_eq!(vector.as_ref(), original);
        assert_eq!(size.value, [100.; 2]);
        assert_eq!(group.items[1].id(), 3);
        let directory = tempfile::tempdir().unwrap();
        motion_core::storage::save(directory.path(), engine.project()).unwrap();
        assert_eq!(
            &motion_core::storage::load(directory.path()).unwrap(),
            engine.project()
        );
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
    #[test]
    fn repeater_eight_parameter_rows_animate_scrub_and_preserve_vector_components_with_one_undo() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for host in [Panel::Properties, Panel::Vector] {
            for index in 0..8 {
                let mut engine = engine();
                let mut controls = Controls::default();
                let actions = draw(&mut controls, &engine, &press(378., 12.), host, &mut atlas);
                apply(&mut engine, actions);
                let row_y = 134. + index as f32 * 26.;
                let actions = draw(
                    &mut controls,
                    &engine,
                    &press(59., row_y + 12.),
                    host,
                    &mut atlas,
                );
                apply(&mut engine, actions);
                let before = engine.project().clone();
                let sign = if index >= 6 { -1. } else { 1. };
                for input in [
                    press(230., row_y + 12.),
                    Input {
                        mouse: [230. + sign * 12., row_y + 12.],
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    Input {
                        mouse: [230. + sign * 20., row_y + 12.],
                        released: Some((MouseButton::Left, [230. + sign * 20., row_y + 12.])),
                        ..Default::default()
                    },
                ] {
                    let actions = draw(&mut controls, &engine, &input, host, &mut atlas);
                    apply(&mut engine, actions);
                }
                let r = repeater(&engine);
                let local = 21;
                match index {
                    0 => assert_eq!(
                        (r.copies.keys[0].frame, r.copies.keys[0].value),
                        (local, 23.)
                    ),
                    1 => assert_eq!(
                        (r.offset.keys[0].frame, r.offset.keys[0].value),
                        (local, 20.)
                    ),
                    2 => assert_eq!(
                        (r.position.keys[0].frame, r.position.keys[0].value),
                        (local, [120., 0.])
                    ),
                    3 => assert_eq!(
                        (r.anchor.keys[0].frame, r.anchor.keys[0].value),
                        (local, [20., 0.])
                    ),
                    4 => assert_eq!(
                        (r.scale.keys[0].frame, r.scale.keys[0].value),
                        (local, [120., 100.])
                    ),
                    5 => assert_eq!(
                        (r.rotation.keys[0].frame, r.rotation.keys[0].value),
                        (local, 20.)
                    ),
                    6 => assert_eq!(
                        (r.start_opacity.keys[0].frame, r.start_opacity.keys[0].value),
                        (local, 80.)
                    ),
                    _ => assert_eq!(
                        (r.end_opacity.keys[0].frame, r.end_opacity.keys[0].value),
                        (local, 80.)
                    ),
                }
                assert_ne!(engine.project(), &before);
                engine.undo().unwrap();
                assert_eq!(engine.project(), &before);
            }
        }
    }
    #[test]
    fn repeater_drag_cancels_on_escape_and_collapsing_group_and_other_host_cannot_consume_it() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for collapse in [false, true] {
            let mut engine = engine();
            let mut controls = Controls::default();
            let actions = draw(
                &mut controls,
                &engine,
                &press(378., 12.),
                Panel::Vector,
                &mut atlas,
            );
            apply(&mut engine, actions);
            let before = engine.project().clone();
            let actions = draw(
                &mut controls,
                &engine,
                &press(230., 146.),
                Panel::Vector,
                &mut atlas,
            );
            apply(&mut engine, actions);
            let actions = draw(
                &mut controls,
                &engine,
                &Input {
                    mouse: [242., 146.],
                    held: Some(MouseButton::Left),
                    ..Default::default()
                },
                Panel::Vector,
                &mut atlas,
            );
            apply(&mut engine, actions);
            assert_ne!(engine.project(), &before);
            assert!(draw(
                &mut controls,
                &engine,
                &Input {
                    mouse: [270., 146.],
                    released: Some((MouseButton::Left, [270., 146.])),
                    ..Default::default()
                },
                Panel::Properties,
                &mut atlas
            )
            .is_empty());
            let input = if collapse {
                press(50., 42.)
            } else {
                Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                }
            };
            let actions = draw(&mut controls, &engine, &input, Panel::Vector, &mut atlas);
            apply(&mut engine, actions);
            assert_eq!(engine.project(), &before);
            assert!(!controls.editing());
        }
    }
    #[test]
    fn removing_repeater_finishes_its_value_gesture_and_undo_restores_each_step() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let mut engine = engine();
        let mut controls = Controls::default();
        let actions = draw(
            &mut controls,
            &engine,
            &press(378., 12.),
            Panel::Properties,
            &mut atlas,
        );
        apply(&mut engine, actions);
        let before = engine.project().clone();
        for input in [
            press(230., 146.),
            Input {
                mouse: [242., 146.],
                held: Some(MouseButton::Left),
                ..Default::default()
            },
        ] {
            let actions = draw(
                &mut controls,
                &engine,
                &input,
                Panel::Properties,
                &mut atlas,
            );
            apply(&mut engine, actions);
        }
        assert_eq!(repeater(&engine).copies.value, 15.);
        let edited = engine.project().clone();
        let actions = draw(
            &mut controls,
            &engine,
            &press(378., 120.),
            Panel::Properties,
            &mut atlas,
        );
        apply(&mut engine, actions);
        let VectorSource::Group { group } = &source(&engine).source else {
            panic!()
        };
        assert_eq!(group.items.len(), 1);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &edited);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
    }
}
