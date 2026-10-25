//! A timeline pick owns one native window and submits only a legal release.
use super::*;
use motion_ui::{dock::Panel, input::Key, paint};
use std::collections::HashSet;

pub(super) struct Pick {
    pub(super) window: Option<Panel>,
    pub(super) rows: Vec<(u64, Rect)>,
    source: u64,
    chooser: Chooser,
    legal: HashSet<u64>,
    origin: [f32; 2],
    mode: ParentMode,
}
impl Pick {
    pub(super) fn new(
        state: &Value,
        object: u64,
        selected: &[u64],
        window: Option<Panel>,
        rect: Rect,
        mode: ParentMode,
    ) -> Option<Self> {
        let scope = targets(state, object, selected)?;
        Some(Self {
            window,
            rows: Vec::new(),
            source: object,
            chooser: Chooser::new(state, object, selected)?,
            legal: candidates_for(state, &scope, Locale::En)
                .into_iter()
                .map(|(id, _)| id)
                .collect(),
            origin: [
                rect.min[0] + rect.width() * 0.5,
                rect.min[1] + rect.height() * 0.5,
            ],
            mode,
        })
    }
    pub(super) fn row(&mut self, object: u64, rect: Rect, link: Option<Rect>) {
        if object == self.source {
            if let Some(link) = link {
                self.origin = [
                    link.min[0] + link.width() * 0.5,
                    link.min[1] + link.height() * 0.5,
                ];
            }
        }
        self.rows.push((object, rect));
    }
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        bounds: Rect,
        state: &Value,
        selected: &[u64],
        input: &Input,
    ) -> Decision {
        if !self.chooser.current(state, selected)
            || input.focus_lost
            || !input.composing
                && !input.ime_consumed
                && input.keys.iter().any(|(key, _)| *key == Key::Escape)
            || input
                .pressed
                .is_some_and(|(button, _)| button != MouseButton::Left)
        {
            return Decision::Close;
        }
        let release = input
            .released
            .filter(|(button, _)| *button == MouseButton::Left)
            .map(|(_, point)| point);
        let point = release.unwrap_or(input.mouse);
        let target = self
            .rows
            .iter()
            .find(|(id, rect)| self.legal.contains(id) && rect.contains(point));
        if release.is_some() {
            return target.map_or(Decision::Close, |(id, _)| {
                Decision::Apply(self.chooser.commands(state, Some(*id), self.mode))
            });
        }
        // A missing release must not leave a gesture attached to later mouse motion.
        if input.held != Some(MouseButton::Left) && input.pressed.is_none() {
            return Decision::Close;
        }
        let styles = list.styles();
        let mut overlay = list.child();
        if let Some((_, rect)) = target {
            paint::stroke(
                &mut overlay,
                *rect,
                styles.colors().ACCENT,
                styles.metric("border", f32::default()),
            );
        }
        paint::line_with_width(
            &mut overlay,
            vec![self.origin, point],
            styles.colors().ACCENT,
            styles.metric("border", f32::default()),
        );
        overlay.clip_to(bounds);
        list.append(overlay);
        Decision::Open
    }
}
