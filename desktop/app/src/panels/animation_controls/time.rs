//! Keep legacy composition addresses distinct from signed authored addresses.
use super::*;
use motion_core::{layer_time::LayerClock, Error, KeyframeTarget, LocalKeyEdit};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Context {
    pub mapped: bool,
    pub clock: LayerClock,
    pub offset: i32,
    pub composition: f64,
    pub local: f64,
}
/// Author identity is never encoded in a display coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::panels) enum Address {
    Frame(i64),
    Local(i32),
    Spatial(u64),
}
#[derive(Clone, Copy, Debug)]
pub(super) struct KeyTime {
    pub address: Address,
    pub local: f64,
}
impl Address {
    pub(super) fn selected(
        self,
        target: KeyframeTarget,
    ) -> Option<super::super::key_selection::AddressedKey> {
        use super::super::key_selection::{AddressedKey, Key, LocalKey, SpatialKey};
        Some(match self {
            Self::Frame(frame) => AddressedKey::Frame(Key {
                target,
                frame: frame.try_into().ok()?,
            }),
            Self::Local(frame) => AddressedKey::Local(LocalKey { target, frame }),
            Self::Spatial(id) => AddressedKey::Spatial(SpatialKey { target, id }),
        })
    }
}
pub(super) struct Entry<'a> {
    pub value: &'a Value,
    pub address: Address,
    pub local: f64,
    pub composition: f64,
}
#[cfg(test)]
pub(super) fn mapped(state: &Value, target: &KeyframeTarget) -> bool {
    super::super::keyframe_clipboard::layer(state, target.object())
        .is_some_and(|layer| !layer["time_mapping"].is_null())
}
impl Context {
    pub fn read(state: &Value, object: u64) -> motion_core::Result<Self> {
        let mapped = super::super::property_authoring::precise(state, object);
        if !mapped {
            let composition = state["frame"].as_f64().unwrap_or(0.).floor();
            return Ok(Self {
                mapped,
                clock: LayerClock::default(),
                offset: 0,
                composition,
                local: composition,
            });
        }
        let composition = state["frame"]
            .as_f64()
            .filter(|frame| frame.is_finite())
            .ok_or_else(|| Error::Invalid("invalid curve playhead".into()))?;
        let (clock, offset) = super::super::property_authoring::clock(state, object)?;
        let local = clock.key_edit_time(composition, offset)?;
        Ok(Self {
            mapped,
            clock,
            offset,
            composition,
            local,
        })
    }
    pub fn keys<'a>(&self, track: &'a Value) -> motion_core::Result<Vec<Entry<'a>>> {
        if let Some(keys) = track.get("spatial_keys") {
            let keys = keys
                .as_array()
                .filter(|keys| keys.len() <= 16_385)
                .ok_or_else(|| Error::Invalid("invalid mini spatial key array".into()))?;
            let mut result = Vec::with_capacity(keys.len());
            let mut ids = std::collections::BTreeSet::new();
            for value in keys {
                if !value["roving"].is_boolean() {
                    return Err(Error::Invalid("mini spatial Roving state missing".into()));
                }
                let id = value["id"]
                    .as_u64()
                    .filter(|id| *id > 0 && ids.insert(*id))
                    .ok_or_else(|| Error::Invalid("invalid mini spatial key identity".into()))?;
                let local = value["local_frame"]
                    .as_f64()
                    .filter(|t| t.is_finite())
                    .ok_or_else(|| Error::Invalid("invalid mini spatial key time".into()))?;
                if result
                    .last()
                    .is_some_and(|entry: &Entry<'_>| entry.local > local)
                {
                    return Err(Error::Invalid(
                        "mini spatial keys are not in authored order".into(),
                    ));
                }
                let composition = self.clock.projected_composition_frame(local, self.offset)?;
                if value["frame"].as_f64() != Some(composition) {
                    return Err(Error::Invalid(
                        "mini spatial key projection disagrees with clock".into(),
                    ));
                }
                result.push(Entry {
                    value,
                    address: Address::Spatial(id),
                    local,
                    composition,
                });
            }
            return Ok(result);
        }
        let keys = match track["keys"].as_array() {
            Some(keys) => keys.as_slice(),
            None if !self.mapped => &[],
            None => return Err(Error::Invalid("curve key array missing".into())),
        };
        if keys.len() > motion_core::MAX_FRAMES as usize {
            return Err(Error::Invalid("curve key array exceeds frame limit".into()));
        }
        let mut result = Vec::with_capacity(keys.len());
        for value in keys {
            let (frame, composition) = if self.mapped {
                let local = value["local_frame"]
                    .as_i64()
                    .and_then(|f| i32::try_from(f).ok())
                    .ok_or_else(|| Error::Invalid("curve local key identity missing".into()))?;
                let composition = self
                    .clock
                    .projected_composition_frame(f64::from(local), self.offset)?;
                if value["frame"].as_f64() != Some(composition) {
                    return Err(Error::Invalid(
                        "curve key projection disagrees with layer clock".into(),
                    ));
                }
                if result
                    .last()
                    .is_some_and(|entry: &Entry<'_>| entry.local >= f64::from(local))
                {
                    return Err(Error::Invalid(
                        "curve local keys are not ordered and unique".into(),
                    ));
                }
                (i64::from(local), composition)
            } else {
                let frame = value["frame"].as_i64().unwrap_or(0);
                (frame, frame as f64)
            };
            result.push(Entry {
                value,
                address: if self.mapped {
                    Address::Local(frame as i32)
                } else {
                    Address::Frame(frame)
                },
                local: if self.mapped {
                    frame as f64
                } else {
                    self.clock.key_edit_time(composition, self.offset)?
                },
                composition,
            });
        }
        Ok(result)
    }
    pub fn integer(&self) -> Option<i32> {
        if self.mapped {
            self.clock
                .integer_key_frame(self.composition, self.offset)
                .ok()
        } else {
            i32::try_from(self.composition as i64).ok()
        }
    }
    pub fn curve(
        &self,
        object: u64,
        property: Property,
        axis: Option<Axis>,
        address: Address,
        easing: Easing,
    ) -> Command {
        if let Address::Spatial(key) = address {
            return Command::SpatialKeyframes {
                target: KeyframeTarget::Property {
                    object,
                    property,
                    axis,
                },
                edit: motion_core::SpatialKeyEdit::Curve { key, easing },
            };
        }
        let frame = match address {
            Address::Frame(frame) => frame,
            Address::Local(frame) => i64::from(frame),
            Address::Spatial(_) => unreachable!(),
        };
        if self.mapped {
            Command::LocalKeyframes {
                target: KeyframeTarget::Property {
                    object,
                    property,
                    axis,
                },
                edit: LocalKeyEdit::Curve {
                    frames: vec![frame as i32],
                    easing,
                },
            }
        } else {
            Command::Curve {
                object,
                property,
                axis,
                frame: frame as u32,
                easing,
            }
        }
    }
}

impl Controls {
    pub(super) fn select_display_key(
        &mut self,
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<Axis>,
        frame: i64,
        mapped: bool,
    ) {
        self.select_key_in(state, object, property, axis, frame);
        self.selected_key = Some(if mapped {
            Address::Local(frame as i32)
        } else {
            Address::Frame(frame)
        });
        if mapped {
            self.local_selection = Some(Some(super::super::key_selection::LocalKey {
                target: KeyframeTarget::Property {
                    object,
                    property,
                    axis,
                },
                frame: frame as i32,
            }));
        }
    }
    pub(in crate::panels) fn select_address(
        &mut self,
        state: &Value,
        object: u64,
        property: Property,
        axis: Option<Axis>,
        address: Address,
    ) {
        self.choose_track_in(state, object, property, axis);
        self.selected_key = Some(address);
        self.key_explicit = true;
        match address {
            Address::Local(frame) => {
                self.local_selection = Some(Some(super::super::key_selection::LocalKey {
                    target: KeyframeTarget::Property {
                        object,
                        property,
                        axis,
                    },
                    frame,
                }))
            }
            Address::Spatial(id) => {
                self.spatial_selection = Some(Some(super::super::key_selection::SpatialKey {
                    target: KeyframeTarget::Property {
                        object,
                        property,
                        axis,
                    },
                    id,
                }))
            }
            Address::Frame(_) => {}
        }
    }
    pub(in crate::panels) fn take_spatial_selection(
        &mut self,
    ) -> Option<Option<super::super::key_selection::SpatialKey>> {
        self.spatial_selection.take()
    }
    pub(in crate::panels) fn take_local_selection(
        &mut self,
    ) -> Option<Option<super::super::key_selection::LocalKey>> {
        self.local_selection.take()
    }
}
