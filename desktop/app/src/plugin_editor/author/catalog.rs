//! Component examples are rendered by the runtime, then reduced for the library.
//! Example children demonstrate composition; an inserted container remains empty.
use super::*;
use motion_ui::paint::{Pattern, Shape};

pub struct Catalog {
    examples: Vec<Preview>,
    hovered: Option<(usize, f32)>,
}
pub fn entries(zh: bool) -> Vec<(Component, String, String)> {
    [
        (
            Component::Row,
            "行",
            "Row",
            "横向排列控件",
            "Arrange horizontally",
        ),
        (
            Component::Column,
            "列",
            "Column",
            "纵向排列控件",
            "Arrange vertically",
        ),
        (
            Component::Group,
            "分组",
            "Group",
            "带标题的折叠区域",
            "Collapsible section",
        ),
        (
            Component::Tabs,
            "标签页",
            "Tabs",
            "切换不同参数页",
            "Switch parameter pages",
        ),
        (
            Component::Split,
            "分栏",
            "Split",
            "可调比例的两个区域",
            "Two resizable regions",
        ),
        (
            Component::Preview,
            "效果预览",
            "Effect preview",
            "放置 GPU 画面区域",
            "Place a GPU viewport",
        ),
        (
            Component::Note,
            "说明",
            "Note",
            "显示一段操作提示",
            "Show an instruction",
        ),
    ]
    .into_iter()
    .map(|(c, z, e, dz, de)| (c, tr(zh, z, e), tr(zh, dz, de)))
    .collect()
}
impl Catalog {
    pub fn new(zh: bool) -> Result<Self, String> {
        let project = PluginProject::from_json(include_str!(
            "../../../../../crates/motion-graph/examples/parameter-workbench.msplugin"
        ))
        .map_err(|e| e.to_string())?;
        let definitions = project.parameters().map_err(|e| e.to_string())?;
        let field = |id: &str, parameter: &str| LayoutNode {
            id: id.into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Parameter {
                parameter: parameter.into(),
                control: ParameterControl::Auto,
            },
        };
        let fields = || vec![field("a", "enabled"), field("b", "amount")];
        let column = |id: &str| LayoutNode {
            id: id.into(),
            width: LayoutExtent::Auto,
            height: LayoutExtent::Auto,
            content: LayoutContent::Column {
                gap: LayoutSpacing::Compact,
                align: LayoutAlign::Stretch,
                children: fields(),
            },
        };
        let mut examples = vec![];
        for (component, _, _) in entries(zh) {
            let content = match component {
                Component::Row => LayoutContent::Row {
                    gap: LayoutSpacing::Normal,
                    align: LayoutAlign::Stretch,
                    children: fields(),
                },
                Component::Column => column("root").content,
                Component::Group => LayoutContent::Group {
                    title: tr(zh, "外观", "Appearance"),
                    collapsible: true,
                    children: fields(),
                },
                Component::Tabs => LayoutContent::Tabs {
                    tabs: vec![
                        LayoutTab {
                            id: "tab-a".into(),
                            title: tr(zh, "外观", "Appearance"),
                            child: column("page-a"),
                        },
                        LayoutTab {
                            id: "tab-b".into(),
                            title: tr(zh, "运动", "Motion"),
                            child: column("page-b"),
                        },
                    ],
                },
                Component::Split => LayoutContent::Split {
                    axis: LayoutAxis::Horizontal,
                    ratio: 0.5,
                    first: Box::new(field("a", "enabled")),
                    second: Box::new(field("b", "amount")),
                },
                Component::Preview => LayoutContent::Preview,
                Component::Note => LayoutContent::Note {
                    text: tr(zh, "在此添加使用说明", "Add an instruction here"),
                },
                Component::Parameter(_) => unreachable!("catalog contains layout components only"),
            };
            // Give each tab's independent subtree unique stable identities.
            let mut layout = NativeLayout {
                format_version: LAYOUT_VERSION,
                root: LayoutNode {
                    id: "root".into(),
                    width: LayoutExtent::Auto,
                    height: LayoutExtent::Auto,
                    content,
                },
            };
            fn unique(n: &mut LayoutNode, next: &mut usize) {
                *next += 1;
                n.id = format!("sample-{next}");
                match &mut n.content {
                    LayoutContent::Row { children, .. }
                    | LayoutContent::Column { children, .. }
                    | LayoutContent::Group { children, .. } => {
                        for c in children {
                            unique(c, next)
                        }
                    }
                    LayoutContent::Tabs { tabs } => {
                        for t in tabs {
                            unique(&mut t.child, next)
                        }
                    }
                    LayoutContent::Split { first, second, .. } => {
                        unique(first, next);
                        unique(second, next)
                    }
                    _ => {}
                }
            }
            unique(&mut layout.root, &mut 0);
            examples.push(Preview::new(layout, definitions.clone(), zh)?);
        }
        Ok(Self {
            examples,
            hovered: None,
        })
    }
    pub fn sample(
        &mut self,
        index: usize,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        target: Rect,
    ) -> Result<(), String> {
        let sheet = list.styles();
        let sample = &mut self.examples[index];
        let metrics = Metrics::new(&sheet);
        let size = metrics.measure(&sample.layout.root, &sample.view);
        let width = size[0].max(sheet.metric("plugin.author.library.sample_width", 320.));
        let height = (size[1] + sheet.metric("plugin.preview.scrollbar.width", 8.))
            .max(sheet.metric("row.height", 22.));
        let source = Rect::new(0., 0., width, height);
        let mut drawing = list.child();
        if let Some(slot) = sample.draw(&mut drawing, atlas, source, source, &neutral())? {
            paint::checkerboard(
                &mut drawing,
                slot,
                sheet.color("panel"),
                sheet.color("border"),
                sheet.metric("padding.panel", 8.),
            );
            ui::text(
                &mut drawing,
                atlas,
                slot,
                "GPU",
                sheet.color("muted"),
                Align::Center,
            );
        }
        let scale = (target.width() / width).min(target.height() / height);
        let origin = [
            target.min[0],
            target.min[1] + (target.height() - height * scale) * 0.5,
        ];
        reduce(&mut drawing, scale, origin);
        if target.width() <= sheet.metric("plugin.author.library.thumbnail_width", 80.) {
            // Tiny labels become unreadable dots. Keep the actual measured
            // geometry, and show its cells as a schematic; hover stays native.
            drawing.glyphs.clear();
            let mut placed = vec![];
            metrics.place(&sample.layout.root, source, &sample.view, &mut placed);
            for placed in placed {
                let r = Rect::new(
                    origin[0] + placed.rect.min[0] * scale,
                    origin[1] + placed.rect.min[1] * scale,
                    placed.rect.width() * scale,
                    placed.rect.height() * scale,
                );
                outline(&mut drawing, r, sheet.color("muted"));
            }
        }
        drawing.clip_to(target);
        list.append(drawing);
        Ok(())
    }
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        scroll: &mut f32,
        zh: bool,
    ) -> Result<(Option<Component>, Vec<Rect>), String> {
        let sheet = list.styles();
        self.hovered = None;
        let pad = sheet.metric("padding.panel", 8.);
        let row = sheet.metric("row.height", 22.);
        let height = sheet.metric("plugin.author.library.item_height", 88.);
        let header = Rect::new(
            area.min[0] + pad,
            area.min[1],
            (area.width() - pad * 2.).max(0.),
            row,
        );
        ui::text(
            list,
            atlas,
            header,
            &tr(
                zh,
                "拖入画布 · 下图为布局示例",
                "Drag to canvas · layout examples",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        let body = Rect::new(
            area.min[0],
            header.max[1],
            area.width(),
            (area.height() - row * 2.).max(0.),
        );
        if body.contains(input.mouse) {
            *scroll += input.wheel[1];
        }
        *scroll = scroll.clamp(
            0.,
            (height * self.examples.len() as f32 - body.height()).max(0.),
        );
        let mut drawing = list.child();
        let mut payload = None;
        let mut rects = vec![];
        for (i, (component, title, description)) in entries(zh).into_iter().enumerate() {
            let r = Rect::new(
                body.min[0],
                body.min[1] + i as f32 * height - *scroll,
                body.width(),
                height,
            );
            rects.push(r.intersection(body));
            if r.intersection(body).height() <= 0. {
                continue;
            }
            if r.contains(input.mouse) && body.contains(input.mouse) {
                self.hovered = Some((i, r.min[1]));
                paint::rect(&mut drawing, r, sheet.color("hover"));
            }
            let thumb = Rect::new(
                r.min[0] + pad,
                r.min[1] + pad,
                sheet.metric("plugin.author.library.thumbnail_width", 80.),
                sheet.metric("plugin.author.library.thumbnail_height", 56.),
            );
            paint::rect(&mut drawing, thumb, sheet.color("well"));
            self.sample(
                i,
                &mut drawing,
                atlas,
                thumb.shrink(sheet.metric("border", 1.)),
            )?;
            outline(&mut drawing, thumb, sheet.color("border"));
            let x = thumb.max[0] + pad;
            let w = (r.max[0] - pad - x).max(0.);
            ui::text(
                &mut drawing,
                atlas,
                Rect::new(x, r.min[1] + pad, w, row),
                &title,
                sheet.color("text"),
                Align::Left,
            );
            super::super::guide::wrapped(
                &mut drawing,
                atlas,
                Rect::new(x, r.min[1] + pad + row, w, height - row - pad * 2.),
                &description,
                sheet.color("muted"),
            );
            paint::rect(
                &mut drawing,
                Rect::new(
                    r.min[0] + pad,
                    r.max[1] - sheet.metric("border", 1.),
                    (r.width() - pad * 2.).max(0.),
                    sheet.metric("border", 1.),
                ),
                sheet.color("border"),
            );
            if press(input, r.intersection(body)) {
                payload = Some(component);
            }
        }
        drawing.clip_to(body);
        list.append(drawing);
        ui::text(
            list,
            atlas,
            Rect::new(
                area.min[0] + pad,
                area.max[1] - row,
                (area.width() - pad * 2.).max(0.),
                row,
            ),
            &tr(
                zh,
                "容器拖入后为空；参数另行绑定",
                "Containers start empty; bind parameters",
            ),
            sheet.color("muted"),
            Align::Left,
        );
        Ok((payload, rects))
    }
    pub fn popover(
        &mut self,
        list: &PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        zh: bool,
    ) -> Result<Option<PaintList>, String> {
        let Some((index, y)) = self.hovered else {
            return Ok(None);
        };
        let sheet = list.styles();
        let pad = sheet.metric("padding.panel", 8.);
        let row = sheet.metric("row.height", 22.);
        let width = bounds
            .width()
            .min(sheet.metric("plugin.author.canvas.wide", 480.));
        if width < sheet.metric("plugin.preview.canvas.min_width", 240.) {
            return Ok(None);
        }
        let sample_height = Metrics::new(&sheet).measure(
            &self.examples[index].layout.root,
            &self.examples[index].view,
        )[1]
        .max(row);
        let height = (sample_height + pad * 4. + row * 3.).min(bounds.height());
        let area = Rect::new(
            bounds.min[0],
            y.clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
            width,
            height,
        );
        let mut drawing = list.child();
        paint::rect(&mut drawing, area, sheet.color("panel"));
        outline(&mut drawing, area, sheet.color("border"));
        let (_, title, description) = entries(zh).remove(index);
        ui::text(
            &mut drawing,
            atlas,
            Rect::new(
                area.min[0] + pad,
                area.min[1] + pad,
                (width - pad * 2.).max(0.),
                row,
            ),
            &format!("{title} · {description}"),
            sheet.color("text"),
            Align::Left,
        );
        let sample = Rect::new(
            area.min[0] + pad,
            area.min[1] + row + pad * 2.,
            (width - pad * 2.).max(0.),
            (height - row * 3. - pad * 3.).max(0.),
        );
        paint::rect(&mut drawing, sample, sheet.color("well"));
        self.sample(index, &mut drawing, atlas, sample)?;
        guide::wrapped(
            &mut drawing,
            atlas,
            Rect::new(
                area.min[0] + pad,
                sample.max[1] + pad,
                (width - pad * 2.).max(0.),
                row * 2.,
            ),
            &tr(
                zh,
                "布局示例 · 示例子项不会随组件插入",
                "Layout example · sample children are not inserted",
            ),
            sheet.color("muted"),
        );
        drawing.clip_to(bounds);
        Ok(Some(drawing))
    }
}
/// Only the noninteractive thumbnail is scaled; runtime hit targets are untouched.
fn reduce(list: &mut PaintList, scale: f32, origin: [f32; 2]) {
    let point = |p: &mut [f32; 2]| {
        for a in 0..2 {
            p[a] = origin[a] + p[a] * scale;
        }
    };
    let rect = |r: &mut Rect| {
        point(&mut r.min);
        point(&mut r.max);
    };
    for shape in &mut list.shapes {
        match shape {
            Shape::Quad { points } => {
                for p in points {
                    point(p)
                }
            }
            Shape::Line { points, clip } | Shape::Stroke { points, clip, .. } => {
                for p in points {
                    point(p)
                }
                if let Some(c) = clip {
                    rect(c)
                }
            }
            Shape::Triangles { points } => {
                for p in points {
                    point(p)
                }
            }
            Shape::Procedural { area, clip, kind } => {
                rect(area);
                rect(clip);
                match kind {
                    Pattern::Box { width, radius, .. } => {
                        *width *= scale;
                        *radius *= scale
                    }
                    Pattern::Checker { size, .. } => *size *= scale,
                    Pattern::Shadow {
                        radius,
                        blur,
                        bounds,
                    } => {
                        *radius *= scale;
                        *blur *= scale;
                        rect(bounds)
                    }
                    Pattern::Grid {
                        origin,
                        minor,
                        major_step,
                        ..
                    } => {
                        point(origin);
                        *minor *= scale;
                        *major_step *= scale
                    }
                    Pattern::Diamond => {}
                }
            }
        }
        if let Shape::Stroke { width, .. } = shape {
            *width *= scale;
        }
    }
    for glyph in &mut list.glyphs {
        point(&mut glyph.position);
        for n in &mut glyph.size {
            *n *= scale;
        }
    }
    for image in &mut list.images {
        rect(&mut image.area);
    }
}
