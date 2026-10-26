//! One native window owns a reference gesture. Only rendered property rows in
//! that window are candidates; release inserts source, never an engine edit.
use super::*;

#[derive(Clone)]
pub(super) struct Hit {
    target: ExpressionTarget,
    rect: Rect,
    owner: Panel,
}
pub(super) struct Pick {
    window: Option<Panel>,
    target: ExpressionTarget,
    origin: [f32; 2],
    started: bool,
    opened: bool,
}
fn controls(panels: &mut Panels) -> [&mut Controls; 3] {
    [
        &mut panels.expressions,
        &mut panels.effect_controls.expressions,
        &mut panels.properties.effects.expressions,
    ]
}
pub(in crate::panels) fn active(panels: &Panels, window: Option<Panel>) -> bool {
    [
        &panels.expressions,
        &panels.effect_controls.expressions,
        &panels.properties.effects.expressions,
    ]
    .iter()
    .any(|controls| {
        controls
            .pick
            .as_ref()
            .is_some_and(|pick| pick.window == window)
    })
}
pub(in crate::panels) fn prepare(panels: &mut Panels, window: Option<Panel>) {
    let collecting = active(panels, window);
    for (index, controls) in controls(panels).into_iter().enumerate() {
        controls.reference_hits.clear();
        controls.collect_references = collecting;
        controls.reference_window = window;
        controls.reference_host = match index {
            1 => Some(Panel::EffectControls),
            2 => Some(Panel::Properties),
            _ => None,
        };
    }
}
pub(in crate::panels) fn finish(
    panels: &mut Panels,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    window: Option<Panel>,
    clips: &[(Panel, Rect)],
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if !active(panels, window) {
        return;
    }
    let mut hits = Vec::new();
    for controls in controls(panels) {
        for hit in &controls.reference_hits {
            let Some((_, clip)) = clips.iter().find(|(panel, _)| *panel == hit.owner) else {
                continue;
            };
            let rect = hit.rect.intersection(*clip);
            if rect.width() > 0. && rect.height() > 0. {
                hits.push(Hit {
                    rect,
                    ..hit.clone()
                });
            }
        }
    }
    let locale = panels.locale;
    for controls in controls(panels) {
        if controls
            .pick
            .as_ref()
            .is_some_and(|pick| pick.window == window)
        {
            controls.finish_reference_pick(list, atlas, bounds, state, input, locale, &hits);
            if controls.pick.is_none() {
                controls.finish_menu(list, atlas, bounds, state, &Input::default(), locale, out);
            }
        }
    }
}
impl Controls {
    pub(in crate::panels) fn reference_hit(
        &mut self,
        target: ExpressionTarget,
        rect: Rect,
        owner: Panel,
    ) {
        if self.collect_references && rect.width() > 0. && rect.height() > 0. {
            self.reference_hits.push(Hit {
                target,
                rect,
                owner: self.reference_host.unwrap_or(owner),
            });
        }
    }
    pub(super) fn begin_reference_pick(&mut self, state: &Value, rect: Rect, click: bool) {
        let Some(editor) = self
            .expanded
            .as_ref()
            .filter(|editor| editor.reference_ready(state))
        else {
            return;
        };
        self.pick = Some(Pick {
            window: self.reference_window,
            target: editor.reference_target().clone(),
            origin: [
                rect.min[0] + rect.width() * 0.5,
                rect.min[1] + rect.height() * 0.5,
            ],
            started: !click,
            opened: click,
        });
    }
    fn finish_reference_pick(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        state: &Value,
        input: &Input,
        locale: Locale,
        hits: &[Hit],
    ) {
        let Some(pick) = &mut self.pick else {
            return;
        };
        let Some(editor) = &mut self.expanded else {
            self.pick = None;
            return;
        };
        let neutral = Input {
            mouse: input.mouse,
            ..Default::default()
        };
        let input = if std::mem::take(&mut pick.opened) {
            &neutral
        } else {
            input
        };
        let escape = !input.composing
            && !input.ime_consumed
            && input
                .key_presses()
                .any(|(key, _, repeat)| key == Key::Escape && !repeat);
        let cancel = !editor.reference_ready(state)
            || input.focus_lost
            || escape
            || input
                .pressed
                .is_some_and(|(button, _)| button != MouseButton::Left)
            || input
                .released
                .is_some_and(|(button, _)| button != MouseButton::Left)
            || pick.started
                && input.held != Some(MouseButton::Left)
                && input.pressed.is_none()
                && input.released.is_none();
        if cancel {
            editor.reference_notice(locale, true);
            self.pick = None;
            return;
        }
        if input
            .pressed
            .is_some_and(|(button, _)| button == MouseButton::Left)
        {
            pick.started = true;
        }
        let release = input
            .released
            .filter(|(button, _)| *button == MouseButton::Left && pick.started);
        let point = release.map_or(input.mouse, |(_, point)| point);
        let hit = hits
            .iter()
            .rev()
            .find(|hit| hit.target != pick.target && hit.rect.contains(point));
        if release.is_some() {
            if let Some(hit) = hit {
                match motion_core::expression_reference_source(&hit.target) {
                    Ok(source) => editor.insert_reference(source, locale),
                    Err(cause) => {
                        log::error!("expression reference failed operation=insert object={} phase=source cause={}", pick.target.object(), crate::fallback_observation::error_cause(&cause.to_string()));
                        editor.reference_error(cause.to_string());
                    }
                }
            } else {
                editor.reference_notice(locale, true);
            }
            self.pick = None;
            return;
        }
        let styles = list.styles();
        let origin = hits
            .iter()
            .find(|hit| hit.target == pick.target)
            .map_or(pick.origin, |hit| {
                [
                    hit.rect.min[0] + hit.rect.width() * 0.5,
                    hit.rect.min[1] + hit.rect.height() * 0.5,
                ]
            });
        if let Some(hit) = hit {
            paint::stroke(
                list,
                hit.rect,
                styles.colors().ACCENT,
                styles.metric("border", f32::default()),
            );
        }
        paint::line_with_width(
            list,
            vec![origin, point],
            styles.colors().ACCENT,
            styles.metric("border", f32::default()),
        );
        let pad = styles.metric("padding.panel", f32::default());
        let row = styles.metric("row.height", f32::default());
        let hint = Rect::new(
            bounds.min[0] + pad,
            (bounds.max[1] - row - pad).max(bounds.min[1]),
            (bounds.width() - pad - pad).max(0.),
            row,
        );
        paint::rect(list, hint, styles.color("panel"));
        ui::text(
            list,
            atlas,
            hint,
            auxiliary::localized(
                locale,
                "释放到属性行以插入引用；Esc 取消。",
                "Release on a property row to insert a reference; Esc cancels.",
            ),
            styles.color("muted"),
            Align::Left,
        );
    }
}
