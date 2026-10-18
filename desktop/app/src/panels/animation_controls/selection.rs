//! Explicit selection extends the existing track context; drawing is not selection.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::panels) struct SelectedTrack {
    pub object: u64,
    pub property: Property,
    pub axis: Option<Axis>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::panels) enum TrackSelection {
    Unselected,
    Current(SelectedTrack),
    Invalid,
}

/// Opt-in acceptance geometry for one target; normal editor frames retain none.
pub(super) struct RowProbe {
    target: SelectedTrack,
    caption: Option<Rect>,
    numbers: Vec<(usize, Rect)>,
}

fn scope(state: &Value) -> (&str, &str) {
    (
        state["root"].as_str().unwrap_or(""),
        state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .unwrap_or(motion_core::composition::MAIN_COMPOSITION),
    )
}

impl Controls {
    pub(crate) fn begin_row_probe(&mut self, object: u64, property: Property, axis: Option<Axis>) {
        self.row_probe = Some(RowProbe {
            target: SelectedTrack {
                object,
                property,
                axis,
            },
            caption: None,
            numbers: vec![],
        });
    }

    pub(crate) fn probed_row(&self, component: Option<usize>) -> Option<Rect> {
        let probe = self.row_probe.as_ref()?;
        if let Some(component) = component {
            probe
                .numbers
                .iter()
                .find(|(index, _)| *index == component)
                .map(|(_, rect)| *rect)
        } else {
            probe.caption
        }
    }

    pub(in crate::panels) fn record_row(
        &mut self,
        target: SelectedTrack,
        caption: Rect,
        numbers: impl Iterator<Item = (usize, Rect)>,
    ) {
        if let Some(probe) = self
            .row_probe
            .as_mut()
            .filter(|probe| probe.target == target)
        {
            probe.caption = Some(caption);
            probe.numbers.clear();
            // Property tracks expose at most three components. Bound the probe
            // independently of any malformed provider without retaining payloads.
            probe.numbers.extend(numbers.take(3));
        }
    }

    pub(in crate::panels) fn choose_track_in(
        &mut self,
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<Axis>,
    ) {
        let next = scope(state);
        if self
            .track_scope
            .as_ref()
            .is_some_and(|old| (old.0.as_str(), old.1.as_str()) != next)
        {
            self.clear_key();
        }
        self.choose_track(object, property, axis);
        self.track_scope = Some((next.0.to_owned(), next.1.to_owned()));
    }

    pub(in crate::panels) fn select_key_in(
        &mut self,
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<Axis>,
        frame: i64,
    ) {
        self.choose_track_in(state, object, property, axis);
        self.selected_key = Some(frame);
        self.key_explicit = true;
    }

    /// A default graph projection must not replace an explicit or stale selection.
    pub(in crate::panels) fn display_track(
        &mut self,
        object: u64,
        property: Property,
        axis: Option<Axis>,
    ) {
        if !self.track_explicit {
            self.display_track_inner(object, property, axis);
        }
    }

    /// Invalid selections remain distinct from no selection, preventing fallback.
    /// A missing/changed channel and user navigation are normal UI transitions.
    pub(in crate::panels) fn track_selection(
        &self,
        state: &Value,
        selected: Option<u64>,
    ) -> TrackSelection {
        if !self.track_explicit {
            return TrackSelection::Unselected;
        }
        let Some((object, property, axis)) = self.key_context else {
            return TrackSelection::Invalid;
        };
        if selected != Some(object)
            || self.property != Some(property)
            || self.axis != axis
            || self
                .track_scope
                .as_ref()
                .is_some_and(|old| (old.0.as_str(), old.1.as_str()) != scope(state))
            || !crate::panels::timeline::property_row_exists(state, object, property, axis)
        {
            return TrackSelection::Invalid;
        }
        TrackSelection::Current(SelectedTrack {
            object,
            property,
            axis,
        })
    }

    /// Read-only validation keeps an invalid key's keyboard ownership intact.
    /// Callers must not turn a stale key gesture into a layer-level operation.
    pub(in crate::panels) fn user_key_in(
        &self,
        state: &Value,
        selected: Option<u64>,
    ) -> Option<KeySelection> {
        let key = self.user_key()?;
        if !matches!(
            self.track_selection(state, selected),
            TrackSelection::Current(_)
        ) {
            return None;
        }
        crate::panels::animation_data::channel(state, key.object, key.property, key.axis)?["keys"]
            .as_array()?
            .iter()
            .any(|entry| entry["frame"].as_u64() == Some(u64::from(key.frame)))
            .then_some(key)
    }
}

/// Reuse the row state without filling numeric fields or changing hit geometry.
pub(in crate::panels) fn highlight_row(list: &mut PaintList, area: Rect, selected: bool) {
    if selected {
        let styles = list.styles();
        let style = styles
            .resolve(
                "row",
                motion_ui::style::States::default().with(motion_ui::style::State::Selected),
            )
            .unwrap_or_else(|| {
                // Row is an optional semantic class in custom themes. Selection is
                // required by StyleSheet::from_json and supplies the same contract.
                styles
                    .resolve("selection", motion_ui::style::States::default())
                    .expect("validated framework selection style")
            });
        motion_ui::style::box_style(list, area, style);
    }
}
