#[path = "catalog.rs"]
mod catalog;
#[path = "inspector.rs"]
mod inspector;
use super::*;
use model::{find, find_mut, Component};
use motion_effects::native_layout::*;

#[derive(Clone)]
enum Payload {
    Move(String),
    Add(Component),
}
enum Gesture {
    Move {
        payload: Payload,
        origin: [f32; 2],
    },
    Size {
        id: String,
        origin: [f32; 2],
        size: [f32; 2],
    },
    Split {
        id: String,
        axis: usize,
        rect: Rect,
    },
}
#[derive(Clone)]
struct Target {
    parent: String,
    index: usize,
    mark: Rect,
}
#[derive(Default)]
pub struct Designer {
    pub selected: Option<String>,
    #[cfg(test)]
    pub rects: BTreeMap<String, Rect>,
    gesture: Option<Gesture>,
    pub narrow_tab: usize,
    pub canvas_size: usize,
    library_tab: usize,
    scroll: f32,
    fields: BTreeMap<String, Field>,
    pub text: Option<(String, TextEditor)>,
    overlays: Vec<PaintList>,
    catalog: Option<catalog::Catalog>,
    #[cfg(test)]
    pub component_rects: Vec<Rect>,
}
#[derive(Default)]
struct Field {
    menu: Dropdown,
    number: NumberEditor,
}
fn outline(list: &mut PaintList, r: Rect, color: motion_ui::Color) {
    let b = list.styles().metric("border", 1.);
    for edge in [
        Rect::new(r.min[0], r.min[1], r.width(), b),
        Rect::new(r.min[0], r.max[1] - b, r.width(), b),
        Rect::new(r.min[0], r.min[1], b, r.height()),
        Rect::new(r.max[0] - b, r.min[1], b, r.height()),
    ] {
        paint::rect(list, edge, color);
    }
}
impl Designer {
    fn menu(
        &mut self,
        key: &str,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        r: Rect,
        bounds: Rect,
        labels: &[String],
        selected: usize,
        input: &Input,
        owner: &Option<String>,
    ) -> Option<usize> {
        let disabled = neutral();
        let input = if owner.as_ref().is_none_or(|k| k == key) {
            input
        } else {
            &disabled
        };
        let items: Vec<_> = labels
            .iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect();
        let mut popup = list.child();
        let result = self.fields.entry(key.into()).or_default().menu.draw_inline(
            &mut popup, atlas, r, bounds, &items, selected, input, true, false,
        );
        self.overlays.push(popup);
        result
    }
    fn number(
        &mut self,
        key: &str,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        r: Rect,
        value: f32,
        min: f64,
        max: f64,
        input: &Input,
        owner: &Option<String>,
    ) -> Vec<NumberEvent> {
        let disabled = neutral();
        let input = if owner.as_ref().is_none_or(|k| k == key) {
            input
        } else {
            &disabled
        };
        let f = &mut self.fields.entry(key.into()).or_default().number;
        f.compact = true;
        f.align_left = true;
        let options = NumberOptions {
            min,
            max,
            speed: 0.1,
            decimals: 2,
        };
        let changes = f.update(value as f64, options, input, r);
        f.draw_styled(
            list,
            atlas,
            r,
            value as f64,
            options,
            &list.styles(),
            Default::default(),
        );
        changes
    }
    pub fn busy(&self) -> bool {
        self.gesture.is_some()
            || self.text.is_some()
            || self
                .fields
                .values()
                .any(|f| f.menu.open || f.number.interacting())
    }
    pub fn finish(&mut self, draft: &mut model::Draft) -> Result<(), String> {
        if let Some((id, editor)) = &self.text {
            let id = id.clone();
            let value = editor.text.clone();
            draft.edit(|p| {
                match &mut find_mut(&mut p.interface.root, &id)
                    .ok_or("component missing")?
                    .content
                {
                    LayoutContent::Group { title, .. } => *title = value,
                    LayoutContent::Note { text } => *text = value,
                    _ => {}
                };
                Ok(())
            })?;
            self.text = None;
        }
        draft.commit();
        Ok(())
    }
    pub fn cancel(&mut self, draft: &mut model::Draft) {
        if self.gesture.is_some() || self.fields.values().any(|f| f.number.interacting()) {
            draft.cancel();
        }
        self.gesture = None;
        self.text = None;
        for f in self.fields.values_mut() {
            f.menu.open = false;
            let _ = f.number.cancel();
        }
    }
    pub fn draw(
        &mut self,
        draft: &mut model::Draft,
        preview: &mut Preview,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        zh: bool,
    ) -> Result<Option<Rect>, String> {
        self.overlays.clear();
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let pad = sheet.metric("padding.panel", 8.);
        let left = sheet.metric("plugin.author.sidebar.width", 224.);
        let right = sheet.metric("plugin.author.inspector.width", 256.);
        let wide = area.width()
            >= left + right + sheet.metric("plugin.preview.row.min_width", 288.) + pad * 4.;
        let sidebar = Rect::new(
            area.min[0],
            area.min[1],
            if wide { left } else { right },
            area.height(),
        );
        let canvas = Rect::new(
            sidebar.max[0] + pad,
            area.min[1],
            (area.width() - sidebar.width() - pad - if wide { right + pad } else { 0. }).max(0.),
            area.height(),
        );
        let inspector = if wide {
            Rect::new(area.max[0] - right, area.min[1], right, area.height())
        } else {
            Rect::new(
                sidebar.min[0],
                sidebar.min[1] + row,
                sidebar.width(),
                (sidebar.height() - row).max(0.),
            )
        };
        // Separate the three working regions without introducing another theme.
        paint::rect(list, sidebar, sheet.color("panel"));
        if wide {
            paint::rect(list, inspector, sheet.color("panel"));
            ui::text(
                list,
                atlas,
                Rect::new(inspector.min[0], inspector.min[1], inspector.width(), row),
                &tr(zh, "布局检查器", "Layout inspector"),
                sheet.color("muted"),
                Align::Left,
            );
        }
        let inspector = if wide {
            Rect::new(
                inspector.min[0],
                inspector.min[1] + row,
                inspector.width(),
                (inspector.height() - row).max(0.),
            )
        } else {
            inspector
        };
        if !wide {
            if let Some(tab) = ui::tab_strip(
                list,
                atlas,
                Rect::new(sidebar.min[0], sidebar.min[1], sidebar.width(), row),
                &[&tr(zh, "结构", "Structure"), &tr(zh, "检查器", "Inspector")],
                self.narrow_tab,
                input,
            ) {
                self.narrow_tab = tab;
            }
        }
        let disabled = neutral();
        let popup_owner = self
            .fields
            .iter()
            .find(|(_, f)| f.menu.open || f.number.interacting())
            .map(|(k, _)| k.clone());
        let owned = self.text.is_some() || popup_owner.is_some();
        let content_input = if owned { &disabled } else { input };
        let mut target = None;
        if wide || self.narrow_tab == 0 {
            let mut r = sidebar;
            if !wide {
                r.min[1] += row;
            }
            target = self.library(draft, list, atlas, r, canvas, content_input, zh)?;
        }
        let mode = Rect::new(canvas.min[0], canvas.min[1], canvas.width(), row);
        if let Some(size) = ui::tab_strip(
            list,
            atlas,
            mode,
            &[
                &tr(zh, "自适应", "Fit"),
                &tr(zh, "窄面板", "Narrow"),
                &tr(zh, "宽面板", "Wide"),
            ],
            self.canvas_size,
            content_input,
        ) {
            self.canvas_size = size;
        }
        let width = match self.canvas_size {
            1 => sheet.metric("plugin.author.canvas.narrow", 320.),
            2 => sheet.metric("plugin.author.canvas.wide", 480.),
            _ => canvas.width(),
        };
        let panel = Rect::new(
            canvas.min[0],
            canvas.min[1] + row + pad,
            width.min(canvas.width()),
            (canvas.height() - row - pad).max(0.),
        );
        preview.layout = draft.project.interface.clone();
        let mut scrolling = neutral();
        scrolling.mouse = content_input.mouse;
        scrolling.wheel = content_input.wheel;
        scrolling.modifiers = content_input.modifiers;
        let slot = preview.draw(list, atlas, panel, panel, &scrolling)?;
        let m = layout::Metrics::new(&sheet);
        let bar = sheet.metric("plugin.preview.scrollbar.width", 8.);
        let viewport = Rect::new(
            panel.min[0],
            panel.min[1],
            (panel.width() - bar).max(0.),
            (panel.height() - bar).max(0.),
        );
        let size = m.measure(&preview.layout.root, &preview.view);
        let root = Rect::new(
            panel.min[0] - preview.scroll.offset[0],
            panel.min[1] - preview.scroll.offset[1],
            size[0].max(viewport.width()),
            size[1].max(viewport.height()),
        );
        let mut placed = Vec::new();
        m.place(&preview.layout.root, root, &preview.view, &mut placed);
        #[cfg(test)]
        {
            self.rects = placed.iter().map(|p| (p.node.id.clone(), p.rect)).collect();
        }
        let tolerance = sheet.metric("plugin.author.resize.hit", 6.);
        let point = content_input.mouse;
        let hit = placed
            .iter()
            .rev()
            .find(|p| {
                if !p.rect.contains(point) || !viewport.contains(point) {
                    return false;
                }
                if let LayoutContent::Split { axis, ratio, .. } = &p.node.content {
                    let a = usize::from(*axis == LayoutAxis::Vertical);
                    return (point[a] - (p.rect.min[a] + (p.rect.max[a] - p.rect.min[a]) * ratio))
                        .abs()
                        <= tolerance;
                }
                false
            })
            .or_else(|| {
                placed.iter().rev().find(|p| {
                    self.selected.as_ref() == Some(&p.node.id)
                        && viewport.contains(point)
                        && (point[0] - p.rect.max[0]).abs() <= tolerance
                        && (point[1] - p.rect.max[1]).abs() <= tolerance
                })
            })
            .or_else(|| {
                placed
                    .iter()
                    .rev()
                    .find(|p| p.rect.contains(point) && viewport.contains(point))
            });
        if let Some(item) = hit {
            if let Some((MouseButton::Left, point)) = content_input.pressed {
                let id = item.node.id.clone();
                self.selected = Some(id.clone());
                let tolerance = sheet.metric("plugin.author.resize.hit", 6.);
                if matches!(item.node.content, LayoutContent::Split { .. }) {
                    if let LayoutContent::Split { axis, ratio, .. } = &item.node.content {
                        let axis = usize::from(*axis == LayoutAxis::Vertical);
                        let divider = item.rect.min[axis]
                            + (item.rect.max[axis] - item.rect.min[axis]) * ratio;
                        if (point[axis] - divider).abs() <= tolerance {
                            draft.begin();
                            self.gesture = Some(Gesture::Split {
                                id: id.clone(),
                                axis,
                                rect: item.rect,
                            });
                        }
                    }
                }
                if self.gesture.is_none() {
                    if (point[0] - item.rect.max[0]).abs() <= tolerance
                        && (point[1] - item.rect.max[1]).abs() <= tolerance
                    {
                        draft.begin();
                        self.gesture = Some(Gesture::Size {
                            id,
                            origin: point,
                            size: [item.rect.width(), item.rect.height()],
                        });
                    } else {
                        self.gesture = Some(Gesture::Move {
                            payload: Payload::Move(id),
                            origin: point,
                        });
                    }
                }
            }
            if self.gesture.is_some() {
                target = Some(self.target_for(
                    draft,
                    item.node,
                    &item.rect,
                    content_input.mouse,
                    &sheet,
                ));
            }
        }
        target = target.filter(|target| {
            let Some(parent) = model::find(&draft.project.interface.root, &target.parent) else {
                return false;
            };
            if !matches!(
                parent.content,
                LayoutContent::Row { .. }
                    | LayoutContent::Column { .. }
                    | LayoutContent::Group { .. }
            ) {
                return false;
            }
            if let Some(Gesture::Move {
                payload: Payload::Move(id),
                ..
            }) = &self.gesture
            {
                if let Some(moving) = model::find(&draft.project.interface.root, id) {
                    if model::find(moving, &target.parent).is_some() {
                        return false;
                    }
                }
            }
            true
        });
        let mut overlay = list.child();
        outline(&mut overlay, viewport, sheet.color("border"));
        for p in &placed {
            if p.node.children().next().is_none()
                && matches!(
                    p.node.content,
                    LayoutContent::Row { .. }
                        | LayoutContent::Column { .. }
                        | LayoutContent::Group { .. }
                )
            {
                outline(&mut overlay, p.rect, sheet.color("border"));
                let mut hint = p.rect.shrink(pad);
                if matches!(p.node.content, LayoutContent::Group { .. }) {
                    hint.min[1] += row;
                }
                ui::text(
                    &mut overlay,
                    atlas,
                    hint,
                    &tr(zh, "拖入组件或参数", "Drop a component or parameter"),
                    sheet.color("muted"),
                    Align::Center,
                );
            }
            if let LayoutContent::Split { axis, ratio, .. } = &p.node.content {
                let b = sheet.metric("border", 1.);
                let r = if *axis == LayoutAxis::Horizontal {
                    Rect::new(
                        p.rect.min[0] + p.rect.width() * ratio,
                        p.rect.min[1],
                        b,
                        p.rect.height(),
                    )
                } else {
                    Rect::new(
                        p.rect.min[0],
                        p.rect.min[1] + p.rect.height() * ratio,
                        p.rect.width(),
                        b,
                    )
                };
                paint::rect(&mut overlay, r, sheet.color("border"));
            }
        }
        if let Some(id) = &self.selected {
            if let Some(p) = placed.iter().find(|p| p.node.id == *id) {
                outline(&mut overlay, p.rect, sheet.color("focus"));
                let size = sheet.metric("plugin.author.resize.hit", 6.);
                paint::rect(
                    &mut overlay,
                    Rect::new(p.rect.max[0] - size, p.rect.max[1] - size, size, size),
                    sheet.color("focus"),
                );
            }
        }
        if let Some(t) = &target {
            if self.gesture.is_some() {
                outline(&mut overlay, t.mark, sheet.color("focus"));
            }
        }
        overlay.clip_to(viewport);
        list.append(overlay);
        if !owned {
            match &self.gesture {
                Some(Gesture::Size { id, origin, size })
                    if content_input.held == Some(MouseButton::Left) =>
                {
                    let id = id.clone();
                    let w = (size[0] + content_input.mouse[0] - origin[0]).clamp(1., 8192.);
                    let h = (size[1] + content_input.mouse[1] - origin[1]).clamp(1., 8192.);
                    draft.update(|p| {
                        let n = find_mut(&mut p.interface.root, &id)
                            .ok_or("layout selection missing")?;
                        n.width = LayoutExtent::Points { value: w };
                        n.height = LayoutExtent::Points { value: h };
                        Ok(())
                    })?;
                }
                Some(Gesture::Split { id, axis, rect })
                    if content_input.held == Some(MouseButton::Left) =>
                {
                    let id = id.clone();
                    let ratio = ((content_input.mouse[*axis] - rect.min[*axis])
                        / (rect.max[*axis] - rect.min[*axis]).max(f32::EPSILON))
                    .clamp(0.05, 0.95);
                    draft.update(|p| {
                        if let LayoutContent::Split { ratio: r, .. } =
                            &mut find_mut(&mut p.interface.root, &id)
                                .ok_or("split missing")?
                                .content
                        {
                            *r = ratio;
                        }
                        Ok(())
                    })?;
                }
                _ => {}
            }
        }
        if content_input
            .released
            .is_some_and(|(b, _)| b == MouseButton::Left)
        {
            if let Some(gesture) = self.gesture.take() {
                match gesture {
                    Gesture::Move { payload, origin } => {
                        let d = (input.mouse[0] - origin[0])
                            .abs()
                            .max((input.mouse[1] - origin[1]).abs());
                        if d >= sheet.metric("motion.drag_threshold", 3.) {
                            if let Some(t) = target {
                                let mut inserted = None;
                                draft.edit(|p| match payload {
                                    Payload::Move(id) => {
                                        model::move_node(p, &id, &t.parent, t.index)
                                    }
                                    Payload::Add(c) => {
                                        let empty = matches!(
                                            &c,
                                            Component::Row | Component::Column | Component::Split
                                        );
                                        let id = model::insert(p, &t.parent, t.index, c)?;
                                        if empty {
                                            model::find_mut(&mut p.interface.root, &id)
                                                .ok_or("inserted node missing")?
                                                .height = LayoutExtent::Points {
                                                value: sheet
                                                    .metric("plugin.author.empty.height", 66.),
                                            };
                                        }
                                        inserted = Some(id);
                                        Ok(())
                                    }
                                })?;
                                if inserted.is_some() {
                                    self.selected = inserted;
                                }
                            }
                        }
                    }
                    _ => draft.commit(),
                }
            }
        } else if self.gesture.is_some()
            && content_input.held.is_none()
            && content_input.pressed.is_none()
        {
            self.gesture = None;
            draft.cancel();
        }
        if wide || self.narrow_tab == 1 {
            self.inspector(
                draft,
                preview,
                list,
                atlas,
                inspector,
                input,
                zh,
                popup_owner,
            )?;
        }
        for overlay in self.overlays.drain(..) {
            list.append(overlay);
        }
        Ok(slot)
    }
    fn target_for(
        &self,
        draft: &model::Draft,
        node: &LayoutNode,
        r: &Rect,
        point: [f32; 2],
        sheet: &motion_ui::style::StyleSheet,
    ) -> Target {
        if matches!(
            node.content,
            LayoutContent::Row { .. } | LayoutContent::Column { .. } | LayoutContent::Group { .. }
        ) {
            let count = node.children().count();
            return Target {
                parent: node.id.clone(),
                index: count,
                mark: *r,
            };
        }
        fn parent<'a>(n: &'a LayoutNode, id: &str) -> Option<(&'a LayoutNode, usize)> {
            for (i, c) in n.children().enumerate() {
                if c.id == id {
                    return Some((n, i));
                }
                if let Some(p) = parent(c, id) {
                    return Some(p);
                }
            }
            None
        }
        if let Some((p, i)) = parent(&draft.project.interface.root, &node.id) {
            let horizontal = matches!(p.content, LayoutContent::Row { .. });
            let a = usize::from(!horizontal);
            let after = point[a] > (r.min[a] + r.max[a]) * 0.5;
            let mut mark = *r;
            let b = sheet.metric("border", 1.);
            mark.min[a] = if after { r.max[a] - b } else { r.min[a] };
            mark.max[a] = mark.min[a] + b;
            return Target {
                parent: p.id.clone(),
                index: i + usize::from(after),
                mark,
            };
        }
        Target {
            parent: node.id.clone(),
            index: 0,
            mark: *r,
        }
    }
    fn library(
        &mut self,
        draft: &mut model::Draft,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        canvas: Rect,
        input: &Input,
        zh: bool,
    ) -> Result<Option<Target>, String> {
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let indent = sheet.metric("tree.indent", 14.);
        if let Some(t) = ui::tab_strip(
            list,
            atlas,
            Rect::new(area.min[0], area.min[1], area.width(), row),
            &[
                &tr(zh, "结构", "Structure"),
                &tr(zh, "组件", "Components"),
                &tr(zh, "参数", "Parameters"),
            ],
            self.library_tab,
            input,
        ) {
            self.library_tab = t;
            self.scroll = 0.;
        }
        let body = Rect::new(
            area.min[0],
            area.min[1] + row,
            area.width(),
            (area.height() - row).max(0.),
        );
        if self.library_tab == 1 {
            if self.catalog.is_none() {
                self.catalog = Some(catalog::Catalog::new(zh)?);
            }
            let catalog = self.catalog.as_mut().expect("catalog initialized");
            let (payload, rects) = catalog.draw(list, atlas, body, input, &mut self.scroll, zh)?;
            #[cfg(test)]
            {
                self.component_rects = rects;
            }
            #[cfg(not(test))]
            let _ = rects;
            if let Some(component) = payload {
                self.gesture = Some(Gesture::Move {
                    payload: Payload::Add(component),
                    origin: input.mouse,
                });
            }
            if self.gesture.is_none() {
                if let Some(popover) = catalog.popover(list, atlas, canvas, zh)? {
                    self.overlays.push(popover);
                }
            }
            return Ok(None);
        }
        if body.contains(input.mouse) {
            self.scroll += input.wheel[1];
        }
        let mut rows: Vec<(usize, String, Option<Payload>)> = vec![];
        if self.library_tab == 0 {
            fn visit(
                n: &LayoutNode,
                depth: usize,
                rows: &mut Vec<(usize, String, Option<Payload>)>,
            ) {
                rows.push((depth, n.id.clone(), Some(Payload::Move(n.id.clone()))));
                for c in n.children() {
                    visit(c, depth + 1, rows);
                }
            }
            visit(&draft.project.interface.root, 0, &mut rows);
        } else {
            for p in draft.project.parameters().map_err(|e| e.to_string())? {
                rows.push((
                    0,
                    format!("{} · {}", p.name, p.id),
                    Some(Payload::Add(Component::Parameter(p.id))),
                ));
            }
        }
        self.scroll = self
            .scroll
            .clamp(0., (rows.len() as f32 * row - body.height()).max(0.));
        let mut paint = list.child();
        let mut target = None;
        for (i, (depth, label, payload)) in rows.into_iter().enumerate() {
            let r = Rect::new(
                body.min[0],
                body.min[1] + i as f32 * row - self.scroll,
                body.width(),
                row,
            );
            if r.max[1] <= body.min[1] || r.min[1] >= body.max[1] {
                continue;
            }
            if self.selected.as_ref() == Some(&label) {
                paint::rect(&mut paint, r, sheet.color("selected"));
            }
            ui::text(
                &mut paint,
                atlas,
                Rect::new(
                    r.min[0] + depth as f32 * indent,
                    r.min[1],
                    (r.width() - depth as f32 * indent).max(0.),
                    row,
                ),
                &label,
                sheet.color("text"),
                Align::Left,
            );
            if press(input, r.intersection(body)) {
                if let Some(Payload::Move(id)) = &payload {
                    self.selected = Some(id.clone());
                }
                if let Some(payload) = payload {
                    self.gesture = Some(Gesture::Move {
                        payload,
                        origin: input.mouse,
                    });
                }
            }
            if self.library_tab == 0 && r.contains(input.mouse) && body.contains(input.mouse) {
                if let Some(n) = find(&draft.project.interface.root, &label) {
                    target = Some(self.target_for(draft, n, &r, input.mouse, &sheet));
                }
            }
        }
        paint.clip_to(body);
        list.append(paint);
        Ok(target)
    }
}
