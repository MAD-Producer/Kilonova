//! Capture a spatial identity once, independently of its changing solved time.
use super::*;
use motion_core::Track;

#[derive(Clone, Debug)]
pub(crate) struct SpatialDraft {
    target: KeyframeTarget,
    key: u64,
    axis: Option<usize>,
}
impl SpatialDraft {
    pub(in crate::panels) fn read(
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<usize>,
    ) -> motion_core::Result<Option<Self>> {
        if !matches!(
            property,
            Property::Position | Property::Anchor | Property::Target
        ) {
            return Ok(None);
        }
        let target = KeyframeTarget::Property {
            object,
            property,
            axis: None,
        };
        let Some((raw, _)) = super::super::key_selection::authored_track(state, &target) else {
            return Ok(None);
        };
        if raw["spatial_animation"].is_null() {
            return Ok(None);
        }
        if raw["spatial_animation"]["keys"]
            .as_array()
            .is_none_or(|keys| keys.len() > 16_385)
        {
            return Err(Error::Invalid("invalid spatial numeric key array".into()));
        }
        if axis.is_some_and(|axis| axis >= if property == Property::Anchor { 2 } else { 3 }) {
            return Err(Error::Invalid("invalid spatial numeric axis".into()));
        }
        let (clock, offset) = clock(state, object)?;
        let frame = state["frame"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| Error::Invalid("invalid spatial numeric playhead".into()))?;
        let times: Vec<_> = if property == Property::Anchor {
            let track: Track<[f32; 2]> = serde_json::from_value(raw.clone())?;
            track.validate_local()?;
            track
                .spatial_animation
                .as_ref()
                .ok_or_else(|| Error::Invalid("spatial key storage missing".into()))?
                .prepared()
                .keys()
                .collect()
        } else {
            let track: Track<[f32; 3]> = serde_json::from_value(raw.clone())?;
            track.validate_local()?;
            track
                .spatial_animation
                .as_ref()
                .ok_or_else(|| Error::Invalid("spatial key storage missing".into()))?
                .prepared()
                .keys()
                .collect()
        };
        let mut key = None;
        for (id, local) in times {
            if clock.projected_composition_frame(local, offset)? == frame {
                if key.replace(id).is_some() {
                    return Err(Error::Invalid(
                        "coincident spatial keys require an explicit identity".into(),
                    ));
                }
            }
        }
        Ok(key.map(|key| Self { target, key, axis }))
    }
    fn authored_key<'a>(&self, state: &'a Value) -> motion_core::Result<Option<&'a Value>> {
        let Some((track, _)) = super::super::key_selection::authored_track(state, &self.target)
        else {
            return Ok(None);
        };
        if track["spatial_animation"].is_null() {
            return Ok(None);
        }
        let keys = track["spatial_animation"]["keys"]
            .as_array()
            .filter(|v| v.len() <= 16_385)
            .ok_or_else(|| Error::Invalid("invalid spatial numeric key array".into()))?;
        let mut found = None;
        for key in keys {
            if key["id"].as_u64() == Some(self.key) {
                if found.replace(key).is_some() {
                    return Err(Error::Invalid("duplicate spatial numeric identity".into()));
                }
            }
        }
        Ok(found)
    }
    pub(in crate::panels) fn current(
        captured: &Option<Self>,
        state: &Value,
    ) -> motion_core::Result<bool> {
        captured.as_ref().map_or(Ok(true), |draft| {
            draft.authored_key(state).map(|key| key.is_some())
        })
    }
    pub(in crate::panels) fn command(
        captured: &Option<Self>,
        state: &Value,
        command: Command,
    ) -> motion_core::Result<Command> {
        let command = at_playhead(state, command);
        let Some(draft) = captured else {
            return Ok(command);
        };
        let Command::TrackAt {
            target,
            frame,
            edit: TrackEdit::Set { mut value },
        } = command
        else {
            return Err(Error::Invalid("spatial numeric command mismatch".into()));
        };
        if target != draft.target {
            return Err(Error::Invalid("spatial numeric target changed".into()));
        }
        let key = draft
            .authored_key(state)?
            .ok_or_else(|| Error::Invalid("spatial numeric key no longer exists".into()))?;
        if let Some(axis) = draft.axis {
            let raw = key["value"]
                .as_array()
                .ok_or_else(|| Error::Invalid("spatial numeric value missing".into()))?;
            let copy_other = |result: &mut [f32]| -> motion_core::Result<()> {
                if raw.len() != result.len() {
                    return Err(Error::Invalid(
                        "spatial numeric value dimensions changed".into(),
                    ));
                }
                for (i, component) in result.iter_mut().enumerate() {
                    if i != axis {
                        *component = raw[i]
                            .as_f64()
                            .map(|v| v as f32)
                            .filter(|v| v.is_finite())
                            .ok_or_else(|| {
                                Error::Invalid("invalid spatial numeric component".into())
                            })?;
                    }
                }
                Ok(())
            };
            match &mut value {
                TrackValue::Vec2(v) => copy_other(v)?,
                TrackValue::Vec3(v) => copy_other(v)?,
                _ => return Err(Error::Invalid("spatial numeric value type mismatch".into())),
            }
        }
        Ok(Command::TrackAt {
            target,
            frame,
            edit: TrackEdit::SetSpatialKey {
                key: draft.key,
                value,
            },
        })
    }
}
