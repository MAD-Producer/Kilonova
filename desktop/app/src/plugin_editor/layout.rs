//! Theme-driven minimums preserve one-row controls. Overflow is clipped/scrollable,
//! never resolved by shrinking an existing interaction target.
use motion_effects::native_layout::*;
use motion_ui::{style::StyleSheet, Rect};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct ViewState {
    pub folded: BTreeSet<String>,
    pub tabs: BTreeMap<String, usize>,
}
pub struct Placed<'a> {
    pub node: &'a LayoutNode,
    pub rect: Rect,
}
pub struct Metrics {
    pub row: f32,
    pub header: f32,
    pub gap: f32,
    pub pad: f32,
    pub min_width: f32,
    pub canvas: [f32; 2],
}
impl Metrics {
    pub fn new(s: &StyleSheet) -> Self {
        Self {
            row: s.metric("row.height", 22.),
            header: s.metric("panel.header.height", 24.),
            gap: s.metric("padding", 5.),
            pad: s.metric("padding.panel", 8.),
            min_width: s.metric("plugin.preview.row.min_width", 288.),
            canvas: [
                s.metric("plugin.preview.canvas.min_width", 240.),
                s.metric("plugin.preview.canvas.min_height", 200.),
            ],
        }
    }
    fn gap(&self, spacing: LayoutSpacing) -> f32 {
        match spacing {
            LayoutSpacing::None => 0.,
            LayoutSpacing::Compact => self.gap,
            LayoutSpacing::Normal => self.pad,
            LayoutSpacing::Relaxed => self.pad * 2.,
        }
    }
    pub fn measure(&self, n: &LayoutNode, state: &ViewState) -> [f32; 2] {
        let mut size = match &n.content {
            LayoutContent::Row { gap, children, .. } => {
                self.children_size(children, 0, self.gap(*gap), state)
            }
            LayoutContent::Column { gap, children, .. } => {
                self.children_size(children, 1, self.gap(*gap), state)
            }
            LayoutContent::Group { children, .. } => {
                let mut size = if state.folded.contains(&n.id) {
                    [self.min_width, 0.]
                } else {
                    self.children_size(children, 1, 0., state)
                };
                size[0] += self.pad * 2.;
                size[1] += self.header;
                size
            }
            LayoutContent::Tabs { tabs } => {
                let mut size = tabs
                    .get(*state.tabs.get(&n.id).unwrap_or(&0))
                    .map(|t| self.measure(&t.child, state))
                    .unwrap_or([self.min_width, 0.]);
                size[1] += self.header;
                size
            }
            LayoutContent::Split {
                axis,
                first,
                second,
                ..
            } => {
                let axis = usize::from(*axis == LayoutAxis::Vertical);
                let a = self.measure(first, state);
                let b = self.measure(second, state);
                let mut size = [0.; 2];
                size[axis] = a[axis] + b[axis] + self.gap;
                size[1 - axis] = a[1 - axis].max(b[1 - axis]);
                size
            }
            LayoutContent::Preview => self.canvas,
            LayoutContent::Parameter { .. }
            | LayoutContent::Note { .. }
            | LayoutContent::Timeline => [self.min_width, self.row],
        };
        for (axis, e) in [n.width, n.height].into_iter().enumerate() {
            if let LayoutExtent::Points { value } = e {
                size[axis] = size[axis].max(value);
            }
        }
        size
    }
    fn children_size(
        &self,
        children: &[LayoutNode],
        axis: usize,
        gap: f32,
        state: &ViewState,
    ) -> [f32; 2] {
        let mut size = [0f32; 2];
        for child in children {
            let s = self.measure(child, state);
            size[axis] += s[axis];
            size[1 - axis] = size[1 - axis].max(s[1 - axis]);
        }
        size[axis] += gap * children.len().saturating_sub(1) as f32;
        size
    }
    pub fn place<'a>(
        &self,
        n: &'a LayoutNode,
        r: Rect,
        state: &ViewState,
        out: &mut Vec<Placed<'a>>,
    ) {
        out.push(Placed { node: n, rect: r });
        match &n.content {
            LayoutContent::Row {
                gap,
                align,
                children,
            } => self.linear(children, r, 0, self.gap(*gap), *align, state, out),
            LayoutContent::Column {
                gap,
                align,
                children,
            } => self.linear(children, r, 1, self.gap(*gap), *align, state, out),
            LayoutContent::Group { children, .. } if !state.folded.contains(&n.id) => self.linear(
                children,
                Rect::new(
                    r.min[0] + self.pad,
                    r.min[1] + self.header,
                    (r.width() - self.pad * 2.).max(0.),
                    (r.height() - self.header).max(0.),
                ),
                1,
                0.,
                LayoutAlign::Stretch,
                state,
                out,
            ),
            LayoutContent::Tabs { tabs } => {
                if let Some(tab) = tabs.get(*state.tabs.get(&n.id).unwrap_or(&0)) {
                    self.place(
                        &tab.child,
                        Rect::new(
                            r.min[0],
                            r.min[1] + self.header,
                            r.width(),
                            (r.height() - self.header).max(0.),
                        ),
                        state,
                        out,
                    );
                }
            }
            LayoutContent::Split {
                axis,
                ratio,
                first,
                second,
            } => {
                let axis = usize::from(*axis == LayoutAxis::Vertical);
                let a = self.measure(first, state);
                let b = self.measure(second, state);
                let length = if axis == 0 { r.width() } else { r.height() };
                let first_len = ((length - self.gap) * ratio)
                    .max(a[axis])
                    .min((length - self.gap - b[axis]).max(a[axis]));
                let mut ar = r;
                ar.max[axis] = ar.min[axis] + first_len;
                let mut br = r;
                br.min[axis] = ar.max[axis] + self.gap;
                self.place(first, ar, state, out);
                self.place(second, br, state, out);
            }
            _ => {}
        }
    }
    fn linear<'a>(
        &self,
        children: &'a [LayoutNode],
        r: Rect,
        axis: usize,
        gap: f32,
        align: LayoutAlign,
        state: &ViewState,
        out: &mut Vec<Placed<'a>>,
    ) {
        let measured: Vec<_> = children.iter().map(|c| self.measure(c, state)).collect();
        let available = if axis == 0 { r.width() } else { r.height() };
        let used = measured.iter().map(|s| s[axis]).sum::<f32>()
            + gap * children.len().saturating_sub(1) as f32;
        let weight = |n: &LayoutNode| match if axis == 0 { n.width } else { n.height } {
            LayoutExtent::Weight { value } => value,
            _ => 0.,
        };
        let total = children.iter().map(weight).sum::<f32>();
        let extra = (available - used).max(0.);
        let mut pos = r.min[axis];
        for (n, s) in children.iter().zip(measured) {
            let length = s[axis]
                + if total > 0. {
                    extra * weight(n) / total
                } else {
                    0.
                };
            let cross = 1 - axis;
            let cross_available = r.max[cross] - r.min[cross];
            let cross_len = if align == LayoutAlign::Stretch {
                cross_available.max(s[cross])
            } else {
                s[cross]
            };
            let offset = match align {
                LayoutAlign::Center => (cross_available - cross_len) * 0.5,
                LayoutAlign::End => cross_available - cross_len,
                _ => 0.,
            }
            .max(0.);
            let mut child = r;
            child.min[axis] = pos;
            child.max[axis] = pos + length;
            child.min[cross] += offset;
            child.max[cross] = child.min[cross] + cross_len;
            self.place(n, child, state, out);
            pos += length + gap;
        }
    }
}
