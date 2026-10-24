//! Effect library gestures preview targets without editing the project.
use super::effect_controls::Entry;
use super::*;

#[derive(Default)]
pub(crate) struct Controls {
    session: Option<Session>,
    window: Option<Panel>,
    targets: Vec<(Rect, Option<u64>)>,
    status: Option<String>,
}
struct Session {
    entry: Entry,
    root: String,
    composition: String,
    source: Option<Panel>,
    origin: [f32; 2],
    owner: Option<Panel>,
    point: [f32; 2],
    active: bool,
    released: bool,
}
fn context(state: &Value) -> (String, String) {
    (
        state["root"].as_str().unwrap_or("").into(),
        state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .unwrap_or(motion_core::composition::MAIN_COMPOSITION)
            .into(),
    )
}
fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh {
        zh.into()
    } else {
        en.into()
    }
}
impl Controls {
    fn dragging(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.active)
    }
    pub(crate) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }
    pub(crate) fn pending(&self) -> bool {
        self.session.is_some()
    }
    pub(crate) fn cancel(&mut self) {
        self.session = None;
        self.targets.clear();
        self.status = None;
    }
    pub(crate) fn pointer_owner(&self) -> Option<(Option<Panel>, [f32; 2])> {
        self.session.as_ref().map(|s| (s.owner, s.point))
    }
    pub(crate) fn begin(&mut self, entry: Entry, point: [f32; 2], state: &Value) {
        let (root, composition) = context(state);
        self.session = Some(Session {
            entry,
            root,
            composition,
            source: self.window,
            origin: point,
            owner: self.window,
            point,
            active: false,
            released: false,
        });
    }
    /// Called by the native event bridge, never from passive window repainting.
    pub(crate) fn pointer(
        &mut self,
        owner: Option<Panel>,
        point: [f32; 2],
        release: bool,
        threshold: f32,
    ) {
        if let Some(s) = &mut self.session {
            s.active |= owner != s.source
                || (point[0] - s.origin[0]).hypot(point[1] - s.origin[1]) >= threshold;
            s.owner = owner;
            s.point = point;
            s.released |= release;
        }
    }
    pub(crate) fn prepare(&mut self, window: Option<Panel>, state: &Value, input: &Input) -> bool {
        self.window = window;
        self.targets.clear();
        self.status = None;
        // A second press without a supplied release is possible in synthetic panel input.
        // An inactive click candidate must not steal the library's existing double-click.
        if self
            .session
            .as_ref()
            .is_some_and(|s| !s.active && s.source == window)
            && input.pressed.is_some()
        {
            self.cancel();
        }
        let owned = self.pending();
        if let Some(s) = &self.session {
            if context(state) != (s.root.clone(), s.composition.clone())
                || input.focus_lost
                || input.key_presses().any(|(k, _, _)| k == Key::Escape)
            {
                self.cancel();
            }
        }
        owned
    }
    #[cfg(test)]
    pub(crate) fn target_rect(&self, object: Option<u64>) -> Option<Rect> {
        self.targets
            .iter()
            .find(|(_, id)| *id == object)
            .map(|(rect, _)| *rect)
    }
    pub(crate) fn target(&mut self, rect: Rect, object: Option<u64>) {
        if self.pending() && rect.width() > 0. && rect.height() > 0. {
            self.targets.push((rect, object));
        }
    }
    fn resolve(
        &self,
        state: &Value,
        selection: &[u64],
        catalogue: &Value,
        locale: Locale,
    ) -> Result<(Rect, Vec<u64>, String), String> {
        let s = self
            .session
            .as_ref()
            .ok_or_else(|| word(locale, "拖动已取消", "Drag cancelled"))?;
        let &(rect, object) = self
            .targets
            .iter()
            .rev()
            .find(|(r, _)| r.contains(s.point))
            .ok_or_else(|| {
                word(
                    locale,
                    "拖到时间轴图层行，或合成画面以应用到所选图层",
                    "Drop on a timeline layer, or the viewer for selected layers",
                )
            })?;
        let ids = match object {
            Some(id) if !selection.contains(&id) => vec![id],
            _ => selection.to_vec(),
        };
        if ids.is_empty() {
            return Err(word(
                locale,
                "请先选择目标图层",
                "Select target layers first",
            ));
        }
        // Resolve the exact package identity again: replacement/disable during a drag is a normal cancellation.
        if !catalogue["packages"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| {
                p["enabled"] != false
                    && p["hash"] == s.entry.hash
                    && p["manifest"]["id"] == s.entry.plugin
                    && p["manifest"]["version"] == s.entry.version
                    && p["manifest"]["effects"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|e| e["id"] == s.entry.id && e["compatibility"] != "unsupported")
            })
        {
            return Err(word(
                locale,
                "效果包已变更或不可用，请重新拖动",
                "Effect package changed or is unavailable; drag again",
            ));
        }
        let mut first = None;
        for id in &ids {
            let layer = state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|l| l["id"].as_u64() == Some(*id))
                .ok_or_else(|| word(locale, "目标图层已不存在", "Target layer no longer exists"))?;
            if layer["locked"] == true {
                return Err(word(
                    locale,
                    "目标中包含锁定图层，未添加效果",
                    "Target includes a locked layer; no effect added",
                ));
            }
            if !s.entry.can_apply(layer) {
                return Err(word(
                    locale,
                    "效果与目标图层类型不兼容，未添加效果",
                    "Effect is incompatible with a target layer; no effect added",
                ));
            }
            first.get_or_insert(layer["name"].as_str().unwrap_or(""));
        }
        let label = if locale == Locale::Zh {
            format!(
                "添加「{}」→「{}」 · {} 个图层",
                s.entry.name,
                first.unwrap_or(""),
                ids.len()
            )
        } else {
            format!(
                "Add {} → {} · {} layer(s)",
                s.entry.name,
                first.unwrap_or(""),
                ids.len()
            )
        };
        Ok((rect, ids, label))
    }
    pub(crate) fn finish(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        state: &Value,
        selection: &[u64],
        catalogue: &Value,
        locale: Locale,
        out: &mut Vec<Action>,
        message: &mut Option<String>,
        cursor: &mut Option<motion_ui::cursor::Cursor>,
    ) -> Option<String> {
        let s = self.session.as_ref()?;
        if s.owner != self.window {
            return None;
        }
        let active = s.active;
        let released = s.released;
        if !active {
            if released {
                self.cancel();
            }
            return None;
        }
        let point = s.point;
        let resolved = self.resolve(state, selection, catalogue, locale);
        let label = match &resolved {
            Ok((_, _, label)) => label.clone(),
            Err(reason) => reason.clone(),
        };
        self.status = Some(label.clone());
        if released {
            let session = self.session.take()?;
            match resolved {
                Ok((_, ids, _)) => {
                    let entry = &session.entry;
                    let request = serde_json::json!({"op":"add", "objects":ids,
                        "plugin":entry.plugin, "version":entry.version, "hash":entry.hash,
                        "effect":entry.id, "composition":session.composition});
                    out.push(Action::Plugin(request));
                    return Some(session.entry.key);
                }
                Err(reason) => *message = Some(reason),
            }
            return None;
        }
        let styles = list.styles();
        let colors = styles.colors();
        if let Ok((rect, _, _)) = resolved {
            paint::stroke(list, rect, colors.ACCENT, styles.metric("border", 1.));
            *cursor = Some(motion_ui::cursor::Cursor::Grabbing);
        } else {
            *cursor = Some(motion_ui::cursor::Cursor::Default);
        }
        let pad = styles.metric("padding.panel", 8.);
        let gap = styles.metric("icon.slot", 20.);
        let height = styles.metric("row.height", 22.) + pad;
        let width = (atlas.measure(&label) + pad * 2.).min((size[0] - pad * 2.).max(0.));
        let rect = Rect::new(
            (point[0] + gap).clamp(pad, (size[0] - pad - width).max(pad)),
            (point[1] + gap).clamp(pad, (size[1] - pad - height).max(pad)),
            width,
            height,
        );
        let style = styles.resolve("tooltip", Default::default()).unwrap();
        motion_ui::style::box_style(list, rect, style);
        motion_ui::style::text_style(list, atlas, rect.shrink(pad), &label, style, Align::Left);
        None
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
    if panels.effect_drag.dragging() {
        panels.browser.cancel_library_click();
    }
    let selection = panels.selection_ids();
    if let Some(key) = panels.effect_drag.finish(
        list,
        atlas,
        size,
        state,
        &selection,
        &panels.effect_controls.catalogue,
        panels.locale,
        out,
        &mut panels.message,
        &mut panels.cursor,
    ) {
        panels.browser.record_recent(key);
    }
}

#[cfg(test)]
mod tests;
