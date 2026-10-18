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

fn scope(state: &Value) -> (String, String) {
    (
        state["root"].as_str().unwrap_or("").to_owned(),
        state["composition"]
            .as_str()
            .or_else(|| state["project"]["composition_id"].as_str())
            .unwrap_or(motion_core::composition::MAIN_COMPOSITION)
            .to_owned(),
    )
}

impl Controls {
    pub(in crate::panels) fn choose_track_in(
        &mut self,
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<Axis>,
    ) {
        let next = scope(state);
        if self.track_scope.as_ref().is_some_and(|old| old != &next) {
            self.clear_key();
        }
        self.choose_track(object, property, axis);
        self.track_scope = Some(next);
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
                .is_some_and(|old| old != &scope(state))
            || !state["project"]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|layer| layer["id"].as_u64() == Some(object))
            || !crate::panels::timeline::property_rows(state, object)
                .iter()
                .any(|row| row.property == property && row.axis == axis.map(Axis::index))
        {
            return TrackSelection::Invalid;
        }
        TrackSelection::Current(SelectedTrack {
            object,
            property,
            axis,
        })
    }
}
