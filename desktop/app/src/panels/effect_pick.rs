//! One click in any composition viewer, with source/revision/time guards.
use super::*;
use crate::layer_clipboard::KeyframeSource;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Position,
    Color,
}
#[derive(Clone, Debug)]
pub(crate) struct Ticket {
    pub source: KeyframeSource,
    pub frame: f64,
    pub object: u64,
    pub effect: u64,
    pub parameter: String,
    pub kind: Kind,
    pub view: Option<(u64, usize, crate::viewer_views::View)>,
}
impl Ticket {
    pub fn new(state: &Value, object: u64, effect: u64, parameter: &str, kind: Kind) -> Self {
        Self {
            source: KeyframeSource::of(state),
            frame: state["frame"].as_f64().unwrap_or(0.),
            object,
            effect,
            parameter: parameter.into(),
            kind,
            view: None,
        }
    }
    pub fn current(&self, state: &Value) -> bool {
        let source = KeyframeSource::of(state);
        self.source == source
            && state["frame"].as_f64() == Some(self.frame)
            && state["observing"] != true
            && state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|l| {
                    l["id"].as_u64() == Some(self.object)
                        && l["locked"] != true
                        && l["effects"].as_array().into_iter().flatten().any(|e| {
                            e["id"].as_u64() == Some(self.effect)
                                && e["params"][&self.parameter].is_object()
                        })
                })
    }
}
/// Existing package metadata identifies source-pixel positions. A relative size
/// default alone is insufficient (for example a vignette's XY radii).
pub(crate) fn is_position(parameter: &Value) -> bool {
    parameter["kind"] == "vec2"
        && matches!(parameter["units"].as_str(), Some("px" | "pixels" | "像素"))
        && (parameter["center_default"] == true || parameter["relative_default"].is_array())
}
#[derive(Default)]
pub(crate) struct State {
    pub ticket: Option<Ticket>,
}
impl State {
    pub fn prepare(&mut self, state: &Value, input: &Input) -> bool {
        let Some(ticket) = &self.ticket else {
            return false;
        };
        if !ticket.current(state)
            || !input.composing
                && !input.ime_consumed
                && (input.keys.iter().any(|(key, _)| *key == Key::Escape)
                    || input
                        .pressed
                        .is_some_and(|(button, _)| button != MouseButton::Left))
        {
            self.ticket = None;
        }
        // Armed picks have no held-pointer state. Native focus transfer between
        // the source inspector and another composition window cannot commit one.
        true
    }
}

pub(crate) fn finish(
    panels: &mut Panels,
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    frames: &[(Panel, Rect)],
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(mut ticket) = panels.effect_pick.ticket.clone() else {
        return;
    };
    let styles = list.styles();
    let tiles = frames
        .iter()
        .find(|(p, _)| *p == Panel::Composition)
        .map(|(_, body)| styled_viewer(*body, &styles))
        .map(|outer| {
            observe!(
                "effect_pick.tiles",
                panels
                    .viewer_views
                    .settings
                    .layout()
                    .tiles(outer, styles.metric("viewer.multiview.gap", 2.)),
                "window={} phase=layout",
                panels.viewer_views.window
            )
        })
        .transpose();
    let tiles = match tiles {
        Ok(Some(tiles)) => tiles,
        Ok(None) => Vec::new(),
        Err(_) => return,
    };
    // An armed pick owns the click, so the composition wrapper does not activate
    // another cell first. Resolve the clicked cell here and keep its navigation.
    let index = tiles
        .iter()
        .position(|tile| tile.contains(input.mouse))
        .unwrap_or(panels.viewer_views.settings.active());
    let viewer = tiles.get(index).copied();
    let pressed = input
        .pressed
        .filter(|(button, _)| *button == MouseButton::Left)
        .map(|(_, p)| p);
    if let Some(viewer) = viewer {
        if viewer.contains(input.mouse) {
            panels.cursor = Some(motion_ui::cursor::Cursor::Crosshair);
        }
        let canvas = [
            state["project"]["width"].as_f64().unwrap_or(0.),
            state["project"]["height"].as_f64().unwrap_or(0.),
        ];
        let Some(view) = panels.viewer_views.settings.view(index).cloned() else {
            return;
        };
        let navigation = if index == panels.viewer_views.settings.active() {
            panels.composition.navigation
        } else {
            view.navigation
        };
        let mapping = navigation.mapping(viewer, canvas);
        if let Some(point) = pressed {
            panels.effect_pick.ticket = None;
            let clip = panels.viewer_guides.canvas(viewer, &styles);
            if let Some(mapping) =
                mapping.filter(|m| clip.contains(point) && m.image.contains(point))
            {
                if ticket.kind == Kind::Position
                    && matches!(
                        view.mode,
                        crate::viewer_views::Mode::Free
                            | crate::viewer_views::Mode::Top
                            | crate::viewer_views::Mode::Side
                    )
                {
                    panels.message = Some(
                        if panels.locale == Locale::Zh {
                            "请在合成或摄影机视图中选择图层位置"
                        } else {
                            "Pick a layer position in a composition or camera view"
                        }
                        .into(),
                    );
                    return;
                }
                ticket.view = Some((panels.viewer_views.window, index, view.clone()));
                let key = panels.viewer_views.key(state);
                let projected = panels
                    .viewer_views
                    .cache
                    .as_ref()
                    .filter(|(source, _)| *source == key)
                    .and_then(|(_, views)| views.get(index));
                if ticket.kind == Kind::Position
                    && projected.is_none()
                    && (view.mode != crate::viewer_views::Mode::Composition
                        || state["observing"] == true)
                {
                    panels.message = Some(
                        if panels.locale == Locale::Zh {
                            "视图尚未就绪，请稍后重试"
                        } else {
                            "View is not ready; pick again after it finishes"
                        }
                        .into(),
                    );
                    return;
                }
                let state = projected.unwrap_or(state);
                let picked = match ticket.kind {
                    Kind::Position => {
                        let position = (|| {
                            let layer = state["project"]["layers"]
                                .as_array()?
                                .iter()
                                .find(|l| l["id"].as_u64() == Some(ticket.object))?;
                            let size = [layer["size"][0].as_f64()?, layer["size"][1].as_f64()?];
                            let local = super::path_controls::layer_projection(
                                state,
                                ticket.object,
                                mapping.image,
                            )?
                            .local(point)?;
                            let position = [
                                local[0] as f64 + size[0] * 0.5,
                                local[1] as f64 + size[1] * 0.5,
                            ];
                            position.iter().all(|v| v.is_finite()).then_some(position)
                        })();
                        let Some(position) = position else {
                            log::error!("effect position pick failed object={} effect={} parameter={} frame={} phase=unproject reason=source_plane_unavailable", ticket.object, ticket.effect, ticket.parameter, ticket.frame);
                            panels.message = Some(
                                if panels.locale == Locale::Zh {
                                    "无法将所选位置映射到图层平面"
                                } else {
                                    "Cannot map this point onto the layer plane"
                                }
                                .into(),
                            );
                            return;
                        };
                        position
                    }
                    Kind::Color => {
                        let point = mapping.local(point);
                        [point[0] / canvas[0], point[1] / canvas[1]]
                    }
                };
                out.push(Action::EffectPick {
                    ticket,
                    point: picked,
                });
            }
            return;
        }
        let pad = styles.metric("padding.panel", 8.);
        let row = styles.metric("row.height", 22.);
        let notice = Rect::new(
            viewer.min[0] + pad,
            viewer.min[1] + pad,
            (viewer.width() - pad * 2.)
                .max(0.)
                .min(styles.metric("menu.popup.max_width", 480.)),
            row,
        );
        paint::rect(list, notice, styles.colors().CHROME);
        let caption = match (ticket.kind, panels.locale) {
            (Kind::Position, Locale::Zh) => "点选效果位置 · Esc / 右键取消",
            (Kind::Color, Locale::Zh) => "点选合成颜色 · Esc / 右键取消",
            (Kind::Position, _) => "Pick effect position · Esc / right-click cancels",
            (Kind::Color, _) => "Pick composition color · Esc / right-click cancels",
        };
        ui::text(
            list,
            atlas,
            notice,
            caption,
            styles.colors().TEXT,
            Align::Left,
        );
    } else if pressed.is_some() {
        panels.effect_pick.ticket = None;
    }
}
