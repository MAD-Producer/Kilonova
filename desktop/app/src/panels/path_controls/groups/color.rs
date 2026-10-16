//! A color remains one property row. Precise channels appear only while editing.
use super::*;
use crate::panels::color_well;
use motion_ui::style::{box_style, text_style, States};

#[derive(Clone)]
pub(super) struct Popup {
    pub host: Panel,
    pub object: u64,
    item: u64,
    parameter: String,
    anchor: Rect,
    opened: bool,
}
impl Controls {
    pub(super) fn color_row(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        y: f32,
        object: u64,
        item: u64,
        p: &parameters::Parameter,
        input: &Input,
        host: Panel,
        enabled: bool,
        depth: usize,
        out: &mut Vec<Action>,
    ) {
        let styles = list.styles();
        let f = layout(styles.as_ref(), area, y, depth, 0, 1, host);
        let edge = styles.metric("field.height", 18.).min(f.number.height());
        let preview = Rect::new(
            f.number.min[0],
            y + (styles.metric("row.height", 22.) - edge) * 0.5,
            (edge + styles.metric("icon.slot", 20.)).min(f.number.width()),
            edge,
        );
        color_well::swatch(list, preview, &p.values, input, enabled);
        let text_area = Rect::new(
            preview.max[0] + styles.metric("padding", 5.),
            y,
            (area.max[0]
                - styles.metric("padding.panel", 8.)
                - preview.max[0]
                - styles.metric("padding", 5.))
            .max(0.),
            styles.metric("row.height", 22.),
        );
        let text = color_well::caption(&p.values);
        let style = styles
            .resolve(
                if enabled {
                    "label.small.muted"
                } else {
                    "label.muted"
                },
                States::default(),
            )
            .unwrap();
        text_style(list, atlas, text_area, &text, style, Align::Left);
        #[cfg(test)]
        self.color_rects
            .insert((host as u32, object, item, p.id.clone()), preview);
        #[cfg(test)]
        for axis in 0..4 {
            self.rects.insert(
                (host as u32, object, item, p.id.clone(), axis),
                (f.watch, preview),
            );
        }
        if enabled && (click(input, preview) || click(input, text_area)) {
            self.finish(out);
            self.color = Some(Popup {
                host,
                object,
                item,
                parameter: p.id.clone(),
                anchor: preview,
                opened: true,
            });
        }
    }
    pub(super) fn color_overlay(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        host: Panel,
        out: &mut Vec<Action>,
    ) {
        let Some(popup) = self.color.clone() else {
            return;
        };
        if popup.host != host {
            return;
        }
        let frame = state["frame"].as_f64().unwrap_or(0.).floor().max(0.) as u32;
        let parameter = keys::cached_layer(state, popup.object, &self.keys.cache)
            .filter(|l| !l.locked)
            .and_then(|l| match &l.content {
                Content::Vector { vector } => parameters::lookup_source(
                    vector,
                    l.size,
                    popup.item,
                    &popup.parameter,
                    l.local_frame(f64::from(frame)),
                    locale,
                ),
                _ => None,
            })
            .filter(|p| p.color && p.values.len() == 4);
        let Some(p) = parameter else {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        };
        let styles = list.styles();
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let slot = styles.metric("icon.slot", 20.);
        let width =
            (styles.metric("label.column", 72.) + styles.metric("value.min_width", 64.) + pad * 2.)
                .max(styles.metric("dropdown.menu_min_width", 160.))
                .min(area.width());
        let height = (row * 5. + pad * 2.).min(area.height());
        let rect = Rect::new(
            popup.anchor.min[0]
                .min(area.max[0] - width)
                .max(area.min[0]),
            popup.anchor.max[1]
                .min(area.max[1] - height)
                .max(area.min[1]),
            width,
            height,
        );
        if !popup.opened
            && input
                .pressed
                .is_some_and(|(_, point)| !rect.contains(point))
        {
            if self.cancel() {
                out.push(Action::History(4));
            }
            return;
        }
        if let Some(popup) = &mut self.color {
            popup.opened = false;
        }
        let neutral = Input::default();
        let input = if popup.opened { &neutral } else { input };
        let mut paint = list.child();
        box_style(
            &mut paint,
            rect,
            styles.resolve("popup", States::default()).unwrap(),
        );
        let title = Rect::new(
            rect.min[0] + pad,
            rect.min[1] + pad,
            (width - pad * 2. - slot).max(0.),
            row,
        );
        label(&mut paint, atlas, title, "RGBA", styles.colors().TEXT);
        if icon(
            &mut paint,
            Rect::new(rect.max[0] - pad - slot, title.min[1], slot, row),
            Icon::Close,
            input,
            true,
            false,
        ) {
            if self.cancel() {
                out.push(Action::History(4));
            }
        } else {
            for (axis, name) in (if locale == Locale::Zh {
                ["R 红", "G 绿", "B 蓝", "A 不透明度"]
            } else {
                ["R Red", "G Green", "B Blue", "A Opacity"]
            })
            .iter()
            .enumerate()
            {
                let y = title.max[1] + axis as f32 * row;
                let start = rect.min[0] + pad + styles.metric("label.column", 72.);
                label(
                    &mut paint,
                    atlas,
                    Rect::new(
                        rect.min[0] + pad,
                        y,
                        (start - rect.min[0] - pad).max(0.),
                        row,
                    ),
                    name,
                    styles.colors().TEXT_MUTED,
                );
                let number = Rect::new(start, y, (rect.max[0] - pad - start).max(0.), row);
                #[cfg(test)]
                self.rects.insert(
                    (host as u32, popup.object, popup.item, p.id.clone(), axis),
                    (Rect::new(0., 0., 0., 0.), number),
                );
                self.component(
                    &mut paint,
                    atlas,
                    number,
                    popup.object,
                    popup.item,
                    &p,
                    axis,
                    state,
                    input,
                    host,
                    true,
                    out,
                );
            }
        }
        paint.clip_to(rect);
        list.append(paint);
    }
}
