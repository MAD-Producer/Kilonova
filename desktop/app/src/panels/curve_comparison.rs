//! A document-scoped display collection, independent of key selection and the
//! authored project. Tracks and samples are cached by revision and viewport.
use super::*;
use motion_core::KeyframeTarget;
use motion_ui::widgets::{Checkbox, Dropdown, DropdownItem, ScrollArea};
use std::sync::Arc;
mod data;
#[cfg(test)]
mod tests;
mod toolbar;
mod view;
use data::{Data, Unavailable};
pub(super) use toolbar::{overlay, toolbar};
pub(super) use view::render;

// This is a display/memory bound, not a model keyframe or clipboard limit.
const MAX_CURVES: usize = 128;
pub(super) fn plot_bounds(area: Rect, styles: &motion_ui::style::StyleSheet) -> Rect {
    let legend = styles.metric("graph.comparison.legend.width", 200.);
    if area.width() >= legend + styles.metric("graph.comparison.plot.min_width", 240.) {
        Rect::new(
            area.min[0],
            area.min[1],
            (area.width() - legend - styles.metric("padding", 5.)).max(0.),
            area.height(),
        )
    } else {
        area
    }
}
#[cfg(test)]
pub(super) fn seed(panels: &mut Panels, state: &Value) {
    panels.select_only(Some(7));
    panels.focused = Some(Panel::Timeline);
    panels.timeline.set_graph_mode(true);
    let controls = &mut panels.timeline.comparison;
    controls.sync(state, panels.locale);
    controls
        .add(
            state,
            panels.locale,
            &[
                KeyframeTarget::Property {
                    object: 7,
                    property: Property::Position,
                    axis: None,
                },
                KeyframeTarget::Property {
                    object: 7,
                    property: Property::Opacity,
                    axis: None,
                },
            ],
        )
        .unwrap();
    controls.enabled = true;
}
#[derive(Clone, PartialEq)]
struct Channel {
    target: KeyframeTarget,
    component: usize,
}
#[derive(Clone)]
struct Entry {
    channel: Channel,
    label: String,
    visible: bool,
    data: Result<Arc<Data>, Arc<Unavailable>>,
    points: Option<([f64; 4], Arc<Vec<(f64, f32)>>)>,
}
#[derive(Default)]
pub(super) struct Controls {
    pub enabled: bool,
    entries: Vec<Entry>,
    scope: Option<(String, String, u64)>,
    stamp: Option<crate::layer_clipboard::KeyframeSource>,
    locale: Option<Locale>,
    legend: ScrollArea,
    groups: ScrollArea,
    bar_focus: Option<usize>,
    unit: Option<data::Unit>,
    single_group: bool,
    compact_groups: bool,
    mode: Dropdown,
    channels_open: bool,
    channels_opened: bool,
    anchors: Option<[Rect; 3]>,
    #[cfg(test)]
    marks: Vec<(Channel, u32, [f32; 2])>,
    #[cfg(test)]
    legend_hits: Vec<(usize, Rect)>,
}
impl Clone for Controls {
    fn clone(&self) -> Self {
        let mut result = Self {
            enabled: self.enabled,
            entries: self.entries.clone(),
            scope: self.scope.clone(),
            stamp: self.stamp.clone(),
            locale: self.locale,
            unit: self.unit.clone(),
            ..Default::default()
        };
        result.legend.offset = self.legend.offset;
        result.groups.offset = self.groups.offset;
        result
    }
}
fn words(locale: Locale, zh: &'static str, en: &'static str) -> &'static str {
    if locale == Locale::Zh {
        zh
    } else {
        en
    }
}
fn selected(panels: &Panels, state: &Value) -> Option<super::keyframe_clipboard::Selection> {
    if panels.selected_keys.owns_keys()
        && panels.selected_keys.status(state) == super::key_selection::Status::Stale
    {
        return None;
    }
    super::keyframe_clipboard::selection(panels, state)
}
pub(super) fn open(panels: &mut Panels, state: &Value, out: &mut Vec<Action>) {
    let mut controls = std::mem::take(&mut panels.timeline.comparison);
    controls.sync(state, panels.locale);
    let selected = selected(panels, state);
    let targets = selected.as_ref().map_or(&[][..], |s| s.targets.as_slice());
    if !targets.is_empty() {
        if let Err(error) = controls.add(state, panels.locale, targets) {
            panels.message = Some(format!(
                "{}: {error}",
                words(
                    panels.locale,
                    "无法加入曲线比较",
                    "Cannot add to curve comparison"
                )
            ));
        }
    }
    out.extend(super::cancel_interaction(panels));
    controls.enabled = true;
    panels.timeline.set_graph_mode(true);
    panels.timeline.comparison = controls;
}
fn scope(state: &Value) -> Option<(String, String, u64)> {
    Some((
        state["root"].as_str()?.into(),
        state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())?
            .into(),
        state["document_generation"].as_u64()?,
    ))
}
fn record_fault(state: &Value, target: &KeyframeTarget, failure: &Unavailable, elapsed_ms: u128) {
    if let Unavailable::Fault(cause) = failure {
        // One record per channel rebuild/revision, not per presentation frame.
        log::error!(
            "curve comparison failed operation=load object={} target={} revision={:?} phase=decode elapsed_ms={elapsed_ms} cause={}",
            target.object(),
            crate::fallback_observation::short(format_args!("{target:?}")),
            state["revision"].as_u64(),
            crate::fallback_observation::error_cause(cause)
        );
    }
}
impl Controls {
    #[cfg(test)]
    pub fn test_anchor(&self, index: usize) -> Rect {
        self.anchors.unwrap()[index]
    }
    pub fn scroll(&self) -> f32 {
        self.groups.offset
    }
    pub fn keyboard_focus(&self) -> bool {
        self.enabled && (self.bar_focus.is_some() || self.popup_open())
    }
    pub fn popup_open(&self) -> bool {
        self.enabled && (self.mode.open || self.channels_open)
    }
    fn sync(&mut self, state: &Value, locale: Locale) {
        let identity = scope(state);
        if self.scope != identity {
            self.entries.clear();
            self.legend = ScrollArea::default();
            self.groups = ScrollArea::default();
            self.scope = identity;
            self.stamp = None;
            self.unit = None;
            self.mode.open = false;
            self.channels_open = false;
            self.bar_focus = None;
        }
        let stamp = crate::layer_clipboard::KeyframeSource::of(state);
        if self.stamp.as_ref() == Some(&stamp) && self.locale == Some(locale) {
            return;
        }
        let mut loaded: Vec<(KeyframeTarget, Result<Arc<Data>, Arc<Unavailable>>)> = Vec::new();
        for entry in &mut self.entries {
            entry.data = if let Some((_, data)) = loaded
                .iter()
                .find(|(target, _)| *target == entry.channel.target)
            {
                data.clone()
            } else {
                let began = std::time::Instant::now();
                let data = data::load(state, &entry.channel.target, locale)
                    .map(Arc::new)
                    .map_err(|failure| {
                        record_fault(
                            state,
                            &entry.channel.target,
                            &failure,
                            began.elapsed().as_millis(),
                        );
                        Arc::new(failure)
                    });
                loaded.push((entry.channel.target.clone(), data.clone()));
                data
            };
            if entry
                .data
                .as_ref()
                .is_ok_and(|data| !data.dimensions.contains(&entry.channel.component))
            {
                entry.data = Err(Arc::new(Unavailable::Unsupported));
            }
            entry.points = None;
            if let Ok(data) = &entry.data {
                entry.label = data.title(entry.channel.component);
            }
        }
        self.stamp = Some(stamp);
        self.locale = Some(locale);
    }
    fn add(
        &mut self,
        state: &Value,
        locale: Locale,
        targets: &[KeyframeTarget],
    ) -> Result<(), Unavailable> {
        if self.scope.is_none() || targets.is_empty() {
            return Err(Unavailable::Missing);
        }
        let began = std::time::Instant::now();
        log::info!(
            "curve comparison start operation=add channels={} first_object={:?} revision={:?} generation={:?}",
            targets.len(),
            targets.first().map(KeyframeTarget::object),
            state["revision"].as_u64(),
            state["document_generation"].as_u64()
        );
        let mut pending = Vec::new();
        for target in targets {
            let data = data::load(state, target, locale).map_err(|failure| {
                record_fault(state, target, &failure, began.elapsed().as_millis());
                failure
            })?;
            let data = Arc::new(data);
            for component in &data.dimensions {
                let channel = Channel {
                    target: target.clone(),
                    component: *component,
                };
                if !self
                    .entries
                    .iter()
                    .chain(pending.iter())
                    .any(|e: &Entry| e.channel == channel)
                {
                    pending.push(Entry {
                        label: data.title(channel.component),
                        channel,
                        visible: true,
                        data: Ok(data.clone()),
                        points: None,
                    });
                }
            }
            if self.entries.len() + pending.len() > MAX_CURVES {
                return Err(Unavailable::Limit);
            }
        }
        self.entries.extend(pending);
        log::debug!(
            "curve comparison done operation=add curves={} elapsed_ms={}",
            self.entries.len(),
            began.elapsed().as_millis()
        );
        Ok(())
    }
}
