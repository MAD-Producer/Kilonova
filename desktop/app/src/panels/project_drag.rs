//! Asset drag gestures own pointer input; only release over a time track edits.
use super::*;
use crate::layer_clipboard::KeyframeSource;
use motion_core::media_assets::AssetRef;

#[derive(Default)]
pub(crate) struct Controls {
    session: Option<Session>,
    window: Option<Panel>,
    target: Option<(Rect, f32, f32, u32)>,
}
#[derive(Clone, Copy)]
enum Asset {
    Media(AssetRef),
    Model(u64),
}
struct Session {
    source: KeyframeSource,
    asset: Asset,
    origin: [f32; 2],
    source_window: Option<Panel>,
    owner: Option<Panel>,
    point: [f32; 2],
    active: bool,
    released: bool,
}
impl Controls {
    pub(crate) fn pending(&self) -> bool {
        self.session.is_some()
    }
    #[cfg(test)]
    pub(crate) fn track_geometry(&self) -> Option<(Rect, f32, f32, u32)> {
        self.target
    }
    pub(crate) fn cancel(&mut self) {
        self.session = None;
        self.target = None;
    }
    pub(crate) fn begin(&mut self, asset: AssetRef, point: [f32; 2], state: &Value) {
        self.begin_asset(Asset::Media(asset), point, state);
    }
    pub(crate) fn begin_model(&mut self, asset: u64, point: [f32; 2], state: &Value) {
        self.begin_asset(Asset::Model(asset), point, state);
    }
    fn begin_asset(&mut self, asset: Asset, point: [f32; 2], state: &Value) {
        self.session = Some(Session {
            source: KeyframeSource::of(state),
            asset,
            origin: point,
            source_window: self.window,
            owner: self.window,
            point,
            active: false,
            released: false,
        });
    }
    pub(crate) fn pointer(
        &mut self,
        owner: Option<Panel>,
        point: [f32; 2],
        released: bool,
        threshold: f32,
    ) {
        if let Some(session) = &mut self.session {
            session.active |= owner != session.source_window
                || (point[0] - session.origin[0]).hypot(point[1] - session.origin[1]) >= threshold;
            session.owner = owner;
            session.point = point;
            session.released |= released;
        }
    }
    pub(crate) fn prepare(&mut self, owner: Option<Panel>, state: &Value, input: &Input) -> bool {
        self.window = owner;
        self.target = None;
        if self
            .session
            .as_ref()
            .is_some_and(|s| !s.active && s.source_window == owner)
            && input.pressed.is_some()
        {
            self.cancel();
        }
        let owned = self.pending();
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.source != KeyframeSource::of(state))
            || input.focus_lost
            || input.key_presses().any(|(key, _, _)| key == Key::Escape)
        {
            self.cancel();
        }
        owned
    }
    pub(crate) fn target(&mut self, area: Rect, offset: f32, ppf: f32, frames: u32) {
        if self.pending() && area.width() > 0. && area.height() > 0. && ppf.is_finite() && ppf > 0.
        {
            self.target = Some((area, offset, ppf, frames));
        }
    }
}
pub(super) fn finish(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    panels: &mut Panels,
    state: &Value,
    out: &mut Vec<Action>,
) {
    let controls = &mut panels.project_drag;
    let Some(session) = &controls.session else {
        return;
    };
    if session.owner != controls.window {
        return;
    }
    if !session.active {
        if session.released {
            controls.cancel();
        }
        return;
    }
    let target = controls
        .target
        .filter(|(area, _, _, _)| area.contains(session.point));
    let frame = target.map(|(area, offset, ppf, frames)| {
        (offset + (session.point[0] - area.min[0]) / ppf)
            .round()
            .clamp(0., frames.saturating_sub(1) as f32) as u32
    });
    if session.released {
        if let Some(at_frame) = frame {
            out.push(match session.asset {
                Asset::Media(target) => Action::ProjectPlace {
                    source: session.source.clone(), target, at_frame,
                },
                Asset::Model(asset) => Action::ProjectPlaceModel {
                    source: session.source.clone(), asset, at_frame,
                },
            });
        }
        controls.cancel();
        return;
    }
    let label = match (panels.locale, frame) {
        (Locale::Zh, Some(frame)) => format!("在第 {frame} 帧创建图层"),
        (Locale::En, Some(frame)) => format!("Create layer at frame {frame}"),
        (Locale::Zh, None) => "拖到时间轴右侧以创建图层".into(),
        (Locale::En, None) => "Drop on the timeline track to create a layer".into(),
    };
    let styles = list.styles();
    if let Some((area, _, _, _)) = target {
        paint::line(
            list,
            vec![
                [session.point[0], area.min[1]],
                [session.point[0], area.max[1]],
            ],
            styles.colors().ACCENT,
        );
        panels.cursor = Some(motion_ui::cursor::Cursor::Grabbing);
    }
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("icon.slot", 20.);
    let height = styles.metric("row.height", 22.) + pad;
    let width = (atlas.measure(&label) + pad * 2.).min((size[0] - pad * 2.).max(0.));
    let rect = Rect::new(
        (session.point[0] + gap).clamp(pad, (size[0] - pad - width).max(pad)),
        (session.point[1] + gap).clamp(pad, (size[1] - pad - height).max(pad)),
        width,
        height,
    );
    let style = styles.resolve("tooltip", Default::default()).unwrap();
    motion_ui::style::box_style(list, rect, style);
    motion_ui::style::text_style(list, atlas, rect.shrink(pad), &label, style, Align::Left);
    let _ = state;
}

#[cfg(test)]
mod tests;
