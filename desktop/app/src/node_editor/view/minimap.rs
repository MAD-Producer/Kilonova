//! A view-only overview. Freeze its transform during navigation to avoid feedback drift.
use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Overview {
    pub area: Rect,
    inner: Rect,
    world: Rect,
    scale: f32,
    pub viewport: Rect,
}

pub(in crate::node_editor) struct NavigationDrag {
    mapping: Overview,
    pub before: [f32; 2],
}

impl Overview {
    pub fn project(self, p: [f32; 2]) -> [f32; 2] {
        [
            self.inner.min[0] + (p[0] - self.world.min[0]) * self.scale,
            self.inner.min[1] + (p[1] - self.world.min[1]) * self.scale,
        ]
    }
    fn unproject(self, p: [f32; 2]) -> [f32; 2] {
        [
            self.world.min[0] + (p[0] - self.inner.min[0]) / self.scale,
            self.world.min[1] + (p[1] - self.inner.min[1]) / self.scale,
        ]
    }
    pub fn project_rect(self, rect: Rect) -> Rect {
        Rect {
            min: self.project(rect.min),
            max: self.project(rect.max),
        }
    }
}

impl NodeEditor {
    pub(super) fn minimap(
        &self,
        canvas: Rect,
        styles: &motion_ui::style::StyleSheet,
    ) -> Option<Overview> {
        if !self.view_preferences.minimap
            || !self.view.zoom.is_finite()
            || self.view.zoom <= 0.
            || !self.view.pan.iter().all(|v| v.is_finite())
        {
            return None;
        }
        let width = styles.metric("node.minimap.width", 180.);
        let height = styles.metric("node.minimap.height", 112.);
        let margin = styles.metric("node.minimap.margin", 8.);
        let padding = styles.metric("node.minimap.padding", 6.);
        if canvas.width() < width + margin * 2. || canvas.height() < height + margin * 2. {
            return None;
        }
        let area = Rect::new(
            canvas.max[0] - width - margin,
            canvas.max[1] - height - margin,
            width,
            height,
        );
        let viewport = Rect {
            min: self.view.screen_to_world(canvas.min, canvas),
            max: self.view.screen_to_world(canvas.max, canvas),
        };
        let mut world = viewport;
        let pad = styles.metric("node.minimap.world_padding", 32.);
        for node in &self.document.nodes {
            for axis in 0..2 {
                let end = node.position[axis]
                    + if axis == 0 {
                        NODE_WIDTH
                    } else {
                        node_height(node, &self.document)
                    };
                world.min[axis] = world.min[axis].min(node.position[axis] - pad);
                world.max[axis] = world.max[axis].max(end + pad);
            }
        }
        if !world
            .min
            .iter()
            .chain(world.max.iter())
            .all(|v| v.is_finite())
            || world.width() <= 0.
            || world.height() <= 0.
        {
            return None;
        }
        let inner = area.shrink(padding);
        let scale = (inner.width() / world.width()).min(inner.height() / world.height());
        if !scale.is_finite() || scale <= 0. {
            return None;
        }
        let inner = Rect::new(
            inner.min[0] + (inner.width() - world.width() * scale) * 0.5,
            inner.min[1] + (inner.height() - world.height() * scale) * 0.5,
            world.width() * scale,
            world.height() * scale,
        );
        Some(Overview {
            area,
            inner,
            world,
            scale,
            viewport,
        })
    }

    pub(super) fn minimap_captures(
        &self,
        canvas: Rect,
        input: &Input,
        styles: &motion_ui::style::StyleSheet,
    ) -> bool {
        self.minimap_drag.is_some()
            || self.minimap(canvas, styles).is_some_and(|m| {
                m.area.contains(input.mouse)
                    || input.pressed.is_some_and(|(_, p)| m.area.contains(p))
            })
    }

    pub(super) fn minimap_input(
        &mut self,
        canvas: Rect,
        input: &Input,
        styles: &motion_ui::style::StyleSheet,
    ) -> bool {
        if input.focus_lost {
            if let Some(drag) = self.minimap_drag.take() {
                self.view.pan = drag.before;
                return true;
            }
        }
        if let Some(drag) = self.minimap_drag.take() {
            if input.held == Some(MouseButton::Left)
                || input.released.is_some_and(|(b, _)| b == MouseButton::Left)
            {
                let point = input
                    .released
                    .filter(|(b, _)| *b == MouseButton::Left)
                    .map_or(input.mouse, |(_, p)| p);
                self.center_from_minimap(drag.mapping, point, canvas);
                if input.held == Some(MouseButton::Left)
                    && !input.released.is_some_and(|(b, _)| b == MouseButton::Left)
                {
                    self.minimap_drag = Some(drag);
                }
            } else {
                self.view.pan = drag.before;
            }
            return true;
        }
        if self.drag.is_some() {
            return false;
        }
        let Some(mapping) = self.minimap(canvas, styles) else {
            return false;
        };
        if let Some((button, point)) = input.pressed.filter(|(_, p)| mapping.area.contains(*p)) {
            if button == MouseButton::Left {
                self.minimap_drag = Some(NavigationDrag {
                    mapping,
                    before: self.view.pan,
                });
                self.center_from_minimap(mapping, point, canvas);
            }
            return true;
        }
        mapping.area.contains(input.mouse)
    }

    fn center_from_minimap(&mut self, mapping: Overview, point: [f32; 2], canvas: Rect) {
        let point = [
            point[0].clamp(mapping.inner.min[0], mapping.inner.max[0]),
            point[1].clamp(mapping.inner.min[1], mapping.inner.max[1]),
        ];
        let center = mapping.unproject(point);
        self.view.pan = [
            canvas.width() * 0.5 - center[0] * self.view.zoom,
            canvas.height() * 0.5 - center[1] * self.view.zoom,
        ];
    }

    pub(super) fn paint_minimap(&self, list: &mut PaintList, canvas: Rect) {
        let styles = list.styles();
        let Some(mut mapping) = self.minimap(canvas, &styles) else {
            return;
        };
        if let Some(drag) = &self.minimap_drag {
            mapping = drag.mapping;
            mapping.viewport = Rect {
                min: self.view.screen_to_world(canvas.min, canvas),
                max: self.view.screen_to_world(canvas.max, canvas),
            };
        }
        let colors = styles.colors();
        list.retain_glyphs(|g| {
            !intersects(
                mapping.area,
                Rect::new(g.position[0], g.position[1], g.size[0], g.size[1]),
            )
        });
        let mut paint = list.child();
        paint::rect(&mut paint, mapping.area, colors.CHROME);
        paint::stroke(
            &mut paint,
            mapping.area,
            colors.BORDER,
            styles.metric("border", 1.),
        );
        for node in &self.document.nodes {
            let rect = mapping.project_rect(Rect::new(
                node.position[0],
                node.position[1],
                NODE_WIDTH,
                node_height(node, &self.document),
            ));
            paint::rect(
                &mut paint,
                rect,
                if self.selected.contains(&node.id) {
                    colors.ACCENT
                } else {
                    colors.TEXT_MUTED
                },
            );
        }
        let viewport = mapping.project_rect(mapping.viewport);
        paint::rect(
            &mut paint,
            viewport,
            colors
                .ACCENT
                .with_alpha(styles.metric("node.minimap.viewport_opacity", 0.10)),
        );
        paint::stroke(
            &mut paint,
            viewport,
            colors.ACCENT,
            styles.metric("border", 1.),
        );
        paint.clip_to(mapping.area);
        list.append(paint);
    }
}
