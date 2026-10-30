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
}

#[derive(Clone, Debug)]
pub struct SpatialAnimation<T> {
    keys: Vec<SpatialAnimationKey<T>>,
    next_id: u64,
    prepared: PreparedSpatialTiming,
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
    /// Imports explicit geometry and linear temporal policy. This constructor
    /// does not claim to convert automatic, Hold, elastic or authored easing;
    /// those policies require their own continuous distance-time representation.
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
            source
                .keys
                .iter()
                .all(|key| !key.auto_bezier && key.curve.is_none() && key.ease == Ease::Linear),
            "spatial animation import requires explicit linear temporal policy",
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
        Ok(Self {
            keys,
            next_id,
            prepared,
        })
    }

    pub fn keys(&self) -> &[SpatialAnimationKey<T>] {
        &self.keys
    }
    pub fn prepared(&self) -> &PreparedSpatialTiming {
        &self.prepared
    }
    pub fn sample(&self, frame: f64) -> Result<SpatialPathSample> {
        self.prepared.sample(frame)
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
        let prepared = times.resolve(self.prepared.path().clone())?;
        *self = Self {
            keys,
            next_id: self.next_id,
            prepared,
        };
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
        let prepared = times.resolve(self.prepared.path().clone())?;
        *self = Self {
            keys,
            next_id: self.next_id,
            prepared,
        };
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
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| crate::Error::Invalid("spatial key identity space exhausted".into()))?;
        let reference = self
            .keys
            .get(index + 1)
            .map_or(self.keys[index].reference + 1., |next| {
                self.keys[index].reference + (next.reference - self.keys[index].reference) * 0.5
            });
        let mut keys = self.keys.clone();
        keys.try_reserve(1)
            .map_err(|e| crate::Error::Invalid(format!("spatial key allocation: {e}")))?;
        keys.insert(
            index + 1,
            SpatialAnimationKey {
                id: self.next_id,
                reference,
                fixed: Some(time),
                value,
                spatial: None,
            },
        );
        let next = Self::prepare(keys, next_id)?;
        let id = self.next_id;
        *self = next;
        Ok(id)
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
