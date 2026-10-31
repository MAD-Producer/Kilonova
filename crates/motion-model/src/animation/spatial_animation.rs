//! Owns authored spatial values and stable-key time constraints together.
//! Prepared geometry is rebuilt on edits/deserialization, never during sampling.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
pub struct SpatialAnimationKey<T> {
    pub id: u64,
    pub reference: f64,
    #[serde(deserialize_with = "super::spatial_schedule::required_option")]
    pub fixed: Option<f64>,
    pub value: T,
    pub spatial: Option<SpatialTangents<T>>,
    /// Outgoing interpolation stays authored outside Roving's incident spans.
    pub easing: Easing,
    /// Persist arc parameterization after freezing a formerly free interval.
    pub arc: bool,
}

#[derive(Clone, Debug)]
pub struct SpatialAnimation<T> {
    keys: Vec<SpatialAnimationKey<T>>,
    next_id: u64,
    prepared: std::sync::Arc<PreparedSpatialTiming>,
    prepared_bytes: usize,
}

/// An immutable allocation identity for runtime budget accounting, never a
/// persisted key identity. Keeping this handle keeps its allocation alive.
#[derive(Clone)]
pub struct SpatialCacheUsage {
    prepared: std::sync::Arc<PreparedSpatialTiming>,
    bytes: usize,
}
impl SpatialCacheUsage {
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
}
impl PartialEq for SpatialCacheUsage {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.prepared, &other.prepared)
    }
}
impl Eq for SpatialCacheUsage {}
impl std::hash::Hash for SpatialCacheUsage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&std::sync::Arc::as_ptr(&self.prepared), state)
    }
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn spatial_cache_usage_owns_shared_allocations_and_releases_edited_versions() {
        let mut track = Track::constant([0.; 3]);
        for (frame, x) in [(0, 0.), (4, 1.), (10, 8.)] {
            track.upsert(frame, [x, 0., 0.], Ease::Linear).unwrap();
        }
        track.enable_roving(&[4]).unwrap();
        let mut animation = track.spatial_animation.take().unwrap();
        let usage = animation.cache_usage();
        let weak = std::sync::Arc::downgrade(&animation.prepared);
        assert!(usage.retained_bytes() > std::mem::size_of::<PreparedSpatialTiming>());
        let clone = animation.clone();
        let mut identities = std::collections::HashSet::new();
        assert!(identities.insert(usage.clone()));
        assert!(!identities.insert(clone.cache_usage()));
        animation.set_fixed_time(2, 2.5).unwrap();
        assert!(identities.insert(animation.cache_usage()));
        assert_eq!(identities.len(), 2);
        drop(clone);
        assert!(
            weak.upgrade().is_some(),
            "budget handle must keep identity alive"
        );
        drop(identities);
        drop(usage);
        assert!(
            weak.upgrade().is_none(),
            "retired geometry must be released without a global registry"
        );
        let current = std::sync::Arc::downgrade(&animation.prepared);
        drop(animation);
        assert!(current.upgrade().is_none());
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct Definition<T> {
    #[serde(deserialize_with = "super::spatial_schedule::deserialize_keys")]
    keys: Vec<SpatialAnimationKey<T>>,
    next_id: u64,
}

impl<T: PartialEq> PartialEq for SpatialAnimation<T> {
    fn eq(&self, other: &Self) -> bool {
        self.keys == other.keys && self.next_id == other.next_id
    }
}

impl<T: Serialize> Serialize for SpatialAnimation<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SpatialAnimation", 2)?;
        state.serialize_field("keys", &self.keys)?;
        state.serialize_field("next_id", &self.next_id)?;
        state.end()
    }
}
impl<'de, T: Tween + Deserialize<'de>> Deserialize<'de> for SpatialAnimation<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let definition = Definition::<T>::deserialize(d)?;
        Self::prepare(definition.keys, definition.next_id).map_err(serde::de::Error::custom)
    }
}

impl<T: Tween> SpatialAnimation<T> {
    /// Imports geometry and manual timing without changing unaffected intervals.
    /// Automatic endpoints need a separate context-dependent conversion.
    pub fn from_track(source: &Track<T>) -> Result<Self> {
        ensure(
            (2..=16_385).contains(&source.keys.len()),
            "spatial timing key count out of range",
        )?;
        source.validate_local()?;
        ensure(
            source.axes.is_none() && source.keys.len() >= 2,
            "spatial animation requires coupled keyed geometry",
        )?;
        ensure(
            source.keys.iter().all(|key| !key.auto_bezier),
            "freeze automatic endpoints before importing spatial timing",
        )?;
        let keys = source
            .keys
            .iter()
            .enumerate()
            .map(|(index, key)| SpatialAnimationKey {
                id: index as u64 + 1,
                reference: f64::from(key.frame),
                fixed: Some(f64::from(key.frame)),
                value: key.value,
                spatial: key.spatial.clone(),
                easing: Easing {
                    ease: key.ease,
                    curve: key.curve,
                },
                arc: false,
            })
            .collect();
        Self::prepare(keys, source.keys.len() as u64 + 1)
    }

    fn prepare(keys: Vec<SpatialAnimationKey<T>>, next_id: u64) -> Result<Self> {
        let times = SpatialKeyTimes::from_parts(
            keys.iter()
                .map(|key| SpatialKeyTime {
                    id: key.id,
                    reference: key.reference,
                    fixed: key.fixed,
                })
                .collect(),
            next_id,
        )?;
        for key in &keys {
            key.easing.validate()?;
            ensure(
                !key.arc || key.easing == Easing::default(),
                "arc-parameterized intervals require explicit linear distance timing",
            )?;
        }
        let mut first = 0;
        while first + 1 < keys.len() {
            let last = (first + 1..keys.len())
                .find(|i| keys[*i].fixed.is_some())
                .ok_or_else(|| crate::Error::Invalid("spatial fixed endpoint missing".into()))?;
            if last > first + 1 {
                ensure(
                    keys[first..last]
                        .iter()
                        .all(|key| key.arc && key.easing == Easing::default()),
                    "Roving spans require explicit linear distance timing",
                )?;
            }
            first = last;
        }
        ensure(
            keys[0].spatial.as_ref().and_then(|s| s.incoming).is_none(),
            "first spatial key has no incoming geometry",
        )?;
        ensure(
            keys[keys.len() - 1]
                .spatial
                .as_ref()
                .and_then(|s| s.outgoing)
                .is_none(),
            "last spatial key has no outgoing geometry",
        )?;
        let mut geometry = Track::constant(keys[0].value);
        geometry.keys = keys
            .iter()
            .enumerate()
            .map(|(index, key)| Keyframe {
                frame: index as i32,
                value: key.value,
                ease: Ease::Linear,
                auto_bezier: false,
                curve: None,
                spatial: key.spatial.clone(),
            })
            .collect();
        // Geometry's integer slots are internal preparation indices, never
        // author times or key identities. They are not persisted or exposed.
        let path = geometry.prepare_spatial_path(SpatialPathBudget::default())?;
        let prepared = times.resolve(path)?;
        let prepared_bytes = prepared.retained_bytes();
        Ok(Self {
            keys,
            next_id,
            prepared: std::sync::Arc::new(prepared),
            prepared_bytes,
        })
    }

    pub fn keys(&self) -> &[SpatialAnimationKey<T>] {
        &self.keys
    }
    pub fn prepared(&self) -> &PreparedSpatialTiming {
        &self.prepared
    }
    /// O(1), including for unchanged snapshots of a long curved path.
    pub fn cache_usage(&self) -> SpatialCacheUsage {
        SpatialCacheUsage {
            prepared: self.prepared.clone(),
            bytes: self.prepared_bytes,
        }
    }
    pub fn sample(&self, frame: f64) -> Result<SpatialPathSample> {
        ensure(frame.is_finite(), "spatial sample time must be finite")?;
        let endpoints = [
            (0, self.keys[0].fixed),
            (self.keys.len() - 1, self.keys.last().and_then(|k| k.fixed)),
        ];
        for (index, time) in endpoints {
            if time.is_some_and(|time| {
                if index == 0 {
                    frame <= time
                } else {
                    frame >= time
                }
            }) {
                return Ok(SpatialPathSample {
                    interval: if index == 0 { 0 } else { index - 1 },
                    parameter: if index == 0 { 0. } else { 1. },
                    position: self.keys[index]
                        .value
                        .spatial_components()
                        .ok_or_else(|| {
                            crate::Error::Invalid("spatial endpoint components missing".into())
                        })?
                        .map(f64::from),
                });
            }
        }
        let (interval, t) = self.prepared.locate(frame)?;
        let a = &self.keys[interval];
        let b = &self.keys[interval + 1];
        if a.arc {
            return self.prepared.sample(frame);
        }
        let progress = a.easing.sample(f64::from(t as f32)).progress as f32;
        let outgoing = a.spatial.as_ref().and_then(|s| s.outgoing);
        let incoming = b.spatial.as_ref().and_then(|s| s.incoming);
        let value = super::spatial_mix(a.value, b.value, outgoing, incoming, progress);
        Ok(SpatialPathSample {
            interval,
            parameter: f64::from(progress),
            position: value
                .spatial_components()
                .ok_or_else(|| crate::Error::Invalid("spatial sample components missing".into()))?
                .map(f64::from),
        })
    }

    pub fn sample_value(&self, frame: f64) -> Result<T> {
        let sample = self.sample(frame)?;
        T::from_spatial_components(sample.position.map(|v| v as f32)).ok_or_else(|| {
            crate::Error::Invalid("spatial sample reconstruction unsupported".into())
        })
    }

    /// Spatial units per local frame, using the same interval and prepared
    /// inverse metric as `sample`. Interior knots use the outgoing interval;
    /// the last endpoint uses its incoming interval. Outside is stationary.
    pub fn velocity(&self, frame: f64) -> Result<[f64; 3]> {
        ensure(frame.is_finite(), "spatial velocity time must be finite")?;
        if self.prepared.outside(frame) {return Ok([0.; 3]);}
        let (index, progress) = self.prepared.locate(frame)?;
        let duration = self.prepared.interval_duration(index);
        if duration == 0. {return Ok([0.; 3]);}
        let a = &self.keys[index];
        let b = &self.keys[index + 1];
        if a.arc {
            let length = self.prepared.interval_length(index)?;
            return self.prepared.path().velocity_interval_distance(index,
                length * progress, length / duration);
        }
        let easing = a.easing.sample(f64::from(progress as f32));
        let p = f64::from(easing.progress as f32);
        let components = |value: T| value.spatial_components().map(|v| v.map(f64::from))
            .ok_or_else(|| crate::Error::Invalid("spatial velocity components missing".into()));
        let start = components(a.value)?;
        let end = components(b.value)?;
        let outgoing = a.spatial.as_ref().and_then(|s| s.outgoing);
        let incoming = b.spatial.as_ref().and_then(|s| s.incoming);
        // Match spatial_mix's f32 control-point construction exactly.
        let c1 = components(outgoing.map_or_else(|| a.value.mix(b.value, 1. / 3.), |v| a.value.add(v)))?;
        let c2 = components(incoming.map_or_else(|| a.value.mix(b.value, 2. / 3.), |v| b.value.add(v)))?;
        let derivative = std::array::from_fn(|i| {
            let tangent = if outgoing.is_none() && incoming.is_none() {end[i] - start[i]} else {
                3. * (1. - p).powi(2) * (c1[i] - start[i])
                    + 6. * (1. - p) * p * (c2[i] - c1[i])
                    + 3. * p.powi(2) * (end[i] - c2[i])
            };
            tangent * easing.velocity / duration
        });
        ensure(derivative.iter().all(|v: &f64| v.is_finite()), "spatial velocity is non-finite")?;
        Ok(derivative)
    }

    pub fn move_keys(&mut self, ids: &[u64], delta: f64) -> Result<()> {
        let mut times = SpatialKeyTimes::from_parts(
            self.keys
                .iter()
                .map(|key| SpatialKeyTime {
                    id: key.id,
                    reference: key.reference,
                    fixed: key.fixed,
                })
                .collect(),
            self.next_id,
        )?;
        times.move_keys(self.prepared.path(), ids, delta)?;
        if delta == 0. {
            return Ok(());
        }
        let mut keys = self.keys.clone();
        for (key, time) in keys.iter_mut().zip(times.keys()) {
            key.fixed = time.fixed;
        }
        *self = Self::prepare(keys, self.next_id)?;
        Ok(())
    }

    pub fn set_roving(&mut self, ids: &[u64], enabled: bool) -> Result<()> {
        let mut times = SpatialKeyTimes::from_parts(
            self.keys
                .iter()
                .map(|key| SpatialKeyTime {
                    id: key.id,
                    reference: key.reference,
                    fixed: key.fixed,
                })
                .collect(),
            self.next_id,
        )?;
        times.set_roving(self.prepared.path(), ids, enabled)?;
        let mut keys = self.keys.clone();
        for (key, time) in keys.iter_mut().zip(times.keys()) {
            key.fixed = time.fixed;
        }
        let mut first = 0;
        while first + 1 < keys.len() {
            let last = (first + 1..keys.len())
                .find(|i| keys[*i].fixed.is_some())
                .ok_or_else(|| crate::Error::Invalid("spatial fixed endpoint missing".into()))?;
            if last > first + 1 {
                for key in &mut keys[first..last] {
                    key.arc = true;
                }
            }
            first = last;
        }
        *self = Self::prepare(keys, self.next_id)?;
        Ok(())
    }

    pub fn set_fixed_time(&mut self, id: u64, time: f64) -> Result<()> {
        let mut times = SpatialKeyTimes::from_parts(
            self.keys
                .iter()
                .map(|key| SpatialKeyTime {
                    id: key.id,
                    reference: key.reference,
                    fixed: key.fixed,
                })
                .collect(),
            self.next_id,
        )?;
        times.set_fixed_time(self.prepared.path(), id, time)?;
        let mut keys = self.keys.clone();
        for (key, time) in keys.iter_mut().zip(times.keys()) {
            key.fixed = time.fixed;
        }
        *self = Self::prepare(keys, self.next_id)?;
        Ok(())
    }

    pub fn set_value(&mut self, id: u64, value: T) -> Result<()> {
        self.edit_key(id, |key| key.value = value)
    }
    /// Insert a fixed key into explicit path order. Existing identities are
    /// retained, and deleting a key never makes its identity available again.
    pub fn insert_fixed_after(&mut self, after: u64, time: f64, value: T) -> Result<u64> {
        let index = self
            .keys
            .iter()
            .position(|key| key.id == after)
            .ok_or_else(|| {
                crate::Error::Invalid(format!("spatial animation key={after} is missing"))
            })?;
        self.insert_fixed(index + 1, time, value)
    }

    fn insert_fixed(&mut self, index: usize, time: f64, value: T) -> Result<u64> {
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| crate::Error::Invalid("spatial key identity space exhausted".into()))?;
        let reference = if index == 0 {
            self.keys[0].reference - 1.
        } else if index == self.keys.len() {
            self.keys[index - 1].reference + 1.
        } else {
            let a = &self.keys[index - 1];
            let b = &self.keys[index];
            let start = self.prepared.effective_time(a.id)?;
            let end = self.prepared.effective_time(b.id)?;
            let progress = if start < time && time < end {
                (time - start) / (end - start)
            } else {
                0.5
            };
            a.reference + (b.reference - a.reference) * progress
        };
        let arc = index > 0 && self.keys[index - 1].arc;
        let mut keys = self.keys.clone();
        keys.try_reserve(1)
            .map_err(|e| crate::Error::Invalid(format!("spatial key allocation: {e}")))?;
        keys.insert(
            index,
            SpatialAnimationKey {
                id: self.next_id,
                reference,
                fixed: Some(time),
                value,
                spatial: None,
                easing: Easing::default(),
                arc,
            },
        );
        let next = Self::prepare(keys, next_id)?;
        let id = self.next_id;
        *self = next;
        Ok(id)
    }

    /// Playhead edits preserve the identity of an exactly addressed key. At a
    /// coincident boundary callers must choose an identity explicitly.
    pub fn set_at_time(&mut self, time: f64, value: T) -> Result<()> {
        ensure(time.is_finite(), "spatial edit time must be finite")?;
        let id = {
            let mut matches = self.prepared.keys().filter(|(_, at)| *at == time);
            let id = matches.next().map(|(id, _)| id);
            ensure(
                matches.next().is_none(),
                "coincident spatial keys require an explicit identity",
            )?;
            id
        };
        if let Some(id) = id {
            return self.set_value(id, value);
        }
        let index = self
            .prepared
            .keys()
            .take_while(|(_, at)| *at < time)
            .count();
        self.insert_fixed(index, time, value)?;
        Ok(())
    }

    pub fn validate_bound(&self, bound: f32) -> Result<()> {
        for key in &self.keys {
            ensure(
                key.value.bounded(bound),
                "transform value exceeds its numeric range",
            )?;
            if let Some(spatial) = &key.spatial {
                for delta in spatial.incoming.iter().chain(&spatial.outgoing) {
                    ensure(
                        key.value.add(*delta).bounded(bound),
                        "spatial control exceeds its numeric range",
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn delete_key(&mut self, id: u64) -> Result<()> {
        ensure(
            self.keys.len() > 2,
            "deleting the final spatial segment requires an owner-level conversion to static",
        )?;
        let index = self
            .keys
            .iter()
            .position(|key| key.id == id)
            .ok_or_else(|| {
                crate::Error::Invalid(format!("spatial animation key={id} is missing"))
            })?;
        let mut keys = self.keys.clone();
        keys.remove(index);
        for endpoint in [0, keys.len() - 1] {
            if keys[endpoint].fixed.is_none() {
                keys[endpoint].fixed = Some(self.prepared.effective_time(keys[endpoint].id)?);
            }
            if let Some(spatial) = &mut keys[endpoint].spatial {
                if endpoint == 0 {
                    spatial.incoming = None;
                } else {
                    spatial.outgoing = None;
                }
                if spatial.incoming.is_none() && spatial.outgoing.is_none() {
                    keys[endpoint].spatial = None;
                }
            }
        }
        let next = Self::prepare(keys, self.next_id)?;
        *self = next;
        Ok(())
    }
    pub fn set_spatial(&mut self, id: u64, spatial: Option<SpatialTangents<T>>) -> Result<()> {
        self.edit_key(id, |key| key.spatial = spatial)
    }
    fn edit_key(&mut self, id: u64, edit: impl FnOnce(&mut SpatialAnimationKey<T>)) -> Result<()> {
        let index = self
            .keys
            .iter()
            .position(|key| key.id == id)
            .ok_or_else(|| {
                crate::Error::Invalid(format!("spatial animation key={id} is missing"))
            })?;
        let mut keys = self.keys.clone();
        edit(&mut keys[index]);
        let next =
            Self::prepare(keys, self.next_id).map_err(|source| crate::Error::SpatialAnimation {
                key: Some(id),
                phase: "prepare_edit",
                source: Box::new(source),
            })?;
        *self = next;
        Ok(())
    }
}

impl<T: Tween> Track<T> {
    pub(super) fn validate_spatial_storage(&self) -> Result<()> {
        ensure(
            self.spatial_animation.is_none() || (self.keys.is_empty() && self.axes.is_none()),
            "spatial animation cannot coexist with ordinary keys or axes",
        )
    }
    pub(super) fn require_frame_keys(&self) -> Result<()> {
        ensure(
            self.spatial_animation.is_none(),
            "spatial key edits require stable key identities",
        )
    }

    /// First conversion addresses existing integer keys exactly. All following
    /// edits use persistent IDs; effective times are never rounded back to keys.
    pub fn enable_roving(&mut self, frames: &[i32]) -> Result<()> {
        self.require_frame_keys()?;
        let mut animation = SpatialAnimation::from_track(self)?;
        let ids = frames
            .iter()
            .map(|frame| {
                self.keys
                    .binary_search_by_key(frame, |key| key.frame)
                    .map(|index| animation.keys[index].id)
                    .map_err(|_| {
                        crate::Error::Invalid(format!("Roving source key frame={frame} missing"))
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        animation.set_roving(&ids, true)?;
        self.spatial_animation = Some(Box::new(animation));
        self.keys.clear();
        Ok(())
    }
}
