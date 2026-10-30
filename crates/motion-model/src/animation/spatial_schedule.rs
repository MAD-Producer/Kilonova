//! Persistent key identity and continuous time ownership, independent of geometry.
//! A prepared schedule owns its path and can outlive edits to both sources.
use super::*;
use std::collections::BTreeSet;

const MAX_KEYS: usize = 16_385;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpatialKeyTime {
    /// Stable within the owning track; never derived from a displayed time.
    pub id: u64,
    /// Monotone reference coordinate for a wholly stationary fixed span.
    /// This is not the effective time or an address for edit commands.
    pub reference: f64,
    /// None releases time ownership to Roving. First and last must be fixed.
    #[serde(deserialize_with = "required_option")]
    pub fixed: Option<f64>,
}

pub(super) fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpatialKeyTimes {
    keys: Vec<SpatialKeyTime>,
    next_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    #[serde(deserialize_with = "deserialize_keys")]
    keys: Vec<SpatialKeyTime>,
    next_id: u64,
}

pub(super) fn deserialize_keys<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    struct Keys<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Keys<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded spatial key array")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> std::result::Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_KEYS) {
                return Err(serde::de::Error::custom(
                    "spatial timing key count out of range",
                ));
            }
            let mut keys = Vec::new();
            while let Some(key) = seq.next_element()? {
                if keys.len() >= MAX_KEYS {
                    return Err(serde::de::Error::custom(
                        "spatial timing key count out of range",
                    ));
                }
                keys.try_reserve(1).map_err(serde::de::Error::custom)?;
                keys.push(key);
            }
            Ok(keys)
        }
    }
    d.deserialize_seq(Keys(std::marker::PhantomData))
}

impl<'de> Deserialize<'de> for SpatialKeyTimes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let definition = Definition::deserialize(d)?;
        let value = Self {
            keys: definition.keys,
            next_id: definition.next_id,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

fn time(value: f64) -> bool {
    value.is_finite() && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value)
}

impl SpatialKeyTimes {
    pub(super) fn from_parts(keys: Vec<SpatialKeyTime>, next_id: u64) -> Result<Self> {
        let value = Self { keys, next_id };
        value.validate()?;
        Ok(value)
    }
    pub fn new(times: &[f64]) -> Result<Self> {
        ensure(
            (2..=MAX_KEYS).contains(&times.len()),
            "spatial timing key count out of range",
        )?;
        let keys = times
            .iter()
            .enumerate()
            .map(|(index, value)| SpatialKeyTime {
                id: index as u64 + 1,
                reference: *value,
                fixed: Some(*value),
            })
            .collect();
        let result = Self {
            keys,
            next_id: times.len() as u64 + 1,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn keys(&self) -> &[SpatialKeyTime] {
        &self.keys
    }

    fn validate(&self) -> Result<()> {
        ensure(
            (2..=MAX_KEYS).contains(&self.keys.len()),
            "spatial timing key count out of range",
        )?;
        ensure(
            self.keys[0].fixed.is_some() && self.keys[self.keys.len() - 1].fixed.is_some(),
            "first and last spatial keys must own their time",
        )?;
        let mut ids = BTreeSet::new();
        let mut previous_reference = None;
        let mut previous_fixed = None;
        for key in &self.keys {
            ensure(
                key.id > 0 && key.id < self.next_id && ids.insert(key.id),
                "spatial key identities must be unique, nonzero and below next_id",
            )?;
            ensure(
                time(key.reference) && previous_reference.is_none_or(|v| v < key.reference),
                "spatial reference coordinates must be finite, bounded and strictly ordered",
            )?;
            if let Some(fixed) = key.fixed {
                ensure(
                    time(fixed) && previous_fixed.is_none_or(|v| v <= fixed),
                    "fixed spatial key times must be finite, bounded and ordered",
                )?;
                previous_fixed = Some(fixed);
            }
            previous_reference = Some(key.reference);
        }
        Ok(())
    }

    fn indices(&self, ids: &[u64]) -> Result<Vec<usize>> {
        ensure(
            !ids.is_empty() && ids.len() <= self.keys.len(),
            "invalid spatial key selection size",
        )?;
        let mut selected = BTreeSet::new();
        for id in ids {
            ensure(selected.insert(*id), "duplicate spatial key selection")?;
        }
        let indices: Vec<_> = self
            .keys
            .iter()
            .enumerate()
            .filter_map(|(index, key)| selected.contains(&key.id).then_some(index))
            .collect();
        ensure(
            indices.len() == ids.len(),
            "selected spatial key identity is missing",
        )?;
        Ok(indices)
    }

    /// The path must be prepared from the same ordered author keys. The returned
    /// snapshot owns its path; it never reads changing live data while sampling.
    pub fn resolve(&self, path: SpatialPath) -> Result<PreparedSpatialTiming> {
        self.validate()?;
        ensure(
            path.key_count() == self.keys.len(),
            "spatial geometry and key times have different counts",
        )?;
        let mut times = Vec::new();
        times.try_reserve_exact(self.keys.len()).map_err(|e| {
            crate::Error::Invalid(format!("spatial effective time allocation: {e}"))
        })?;
        times.resize(self.keys.len(), 0.);
        let mut first = 0;
        while first + 1 < self.keys.len() {
            let last = (first + 1..self.keys.len())
                .find(|i| self.keys[*i].fixed.is_some())
                .ok_or_else(|| crate::Error::Invalid("spatial fixed endpoint missing".into()))?;
            let start = self.keys[first]
                .fixed
                .ok_or_else(|| crate::Error::Invalid("spatial fixed start missing".into()))?;
            let end = self.keys[last]
                .fixed
                .ok_or_else(|| crate::Error::Invalid("spatial fixed end missing".into()))?;
            let mut length = 0.;
            for interval in first..last {
                length += path.interval_length(interval)?;
            }
            ensure(
                start < end || length == 0.,
                "nonstationary spatial span needs positive duration",
            )?;
            times[first] = start;
            let mut traveled = 0.;
            for index in first + 1..last {
                traveled += path.interval_length(index - 1)?;
                let ratio = if length > 0. {
                    traveled / length
                } else {
                    (self.keys[index].reference - self.keys[first].reference)
                        / (self.keys[last].reference - self.keys[first].reference)
                };
                let offset = if length > 0. {
                    let product = (end - start) * traveled;
                    if product.is_finite() && (product >= f64::MIN_POSITIVE || traveled == 0.) {
                        product / length
                    } else {
                        (end - start) * (traveled / length)
                    }
                } else {
                    (end - start) * ratio
                };
                times[index] = start + offset;
            }
            times[last] = end;
            first = last;
        }
        ensure(
            times.windows(2).all(|v| v[0] <= v[1]) && times.iter().all(|v| time(*v)),
            "spatial effective times are invalid",
        )?;
        for (index, pair) in times.windows(2).enumerate() {
            ensure(
                pair[0] < pair[1] || path.interval_length(index)? == 0.,
                "spatial effective time resolution cannot represent a moving interval",
            )?;
        }
        let ids = self.keys.iter().map(|key| key.id).collect();
        Ok(PreparedSpatialTiming { path, times, ids })
    }

    /// Change only the requested keys' ownership. Freezing uses the current
    /// solved f64 times and preserves both geometry and all stable identities.
    pub fn set_roving(&mut self, path: &SpatialPath, ids: &[u64], enabled: bool) -> Result<()> {
        let indices = self.indices(ids)?;
        let previous = self.resolve(path.clone())?;
        let mut next = self.clone();
        for index in indices {
            ensure(
                !enabled || (index > 0 && index + 1 < self.keys.len()),
                "first and last spatial keys cannot rove",
            )?;
            next.keys[index].fixed = if enabled {
                None
            } else {
                Some(previous.times[index])
            };
        }
        next.resolve(path.clone())?;
        *self = next;
        Ok(())
    }

    /// Time edits address a stable key ID, and explicitly release that key's
    /// Roving constraint. Crossing another fixed anchor requires a geometry
    /// reorder transaction in the owner, never an implicit value swap here.
    pub fn set_fixed_time(&mut self, path: &SpatialPath, id: u64, fixed: f64) -> Result<()> {
        let index = self.indices(&[id])?[0];
        let mut next = self.clone();
        next.keys[index].fixed = Some(fixed);
        next.resolve(path.clone())?;
        *self = next;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PreparedSpatialTiming {
    path: SpatialPath,
    times: Vec<f64>,
    ids: Vec<u64>,
}
impl PreparedSpatialTiming {
    pub(super) fn path(&self) -> &SpatialPath {
        &self.path
    }
    pub fn keys(&self) -> impl ExactSizeIterator<Item = (u64, f64)> + '_ {
        self.ids.iter().copied().zip(self.times.iter().copied())
    }
    pub fn effective_time(&self, id: u64) -> Result<f64> {
        self.ids
            .iter()
            .position(|candidate| *candidate == id)
            .map(|index| self.times[index])
            .ok_or_else(|| crate::Error::Invalid("spatial key identity is missing".into()))
    }
    /// Bounded binary search and arc lookup; healthy queries allocate nothing.
    pub fn sample(&self, frame: f64) -> Result<SpatialPathSample> {
        let (interval, progress) = self.locate(frame)?;
        self.path
            .sample_interval_distance(interval, self.path.interval_length(interval)? * progress)
    }

    pub(super) fn locate(&self, frame: f64) -> Result<(usize, f64)> {
        ensure(frame.is_finite(), "spatial sample time must be finite")?;
        if frame < self.times[0] {
            return Ok((0, 0.));
        }
        let upper = self.times.partition_point(|time| *time <= frame);
        let last = self.times.len() - 1;
        if upper >= self.times.len() {
            return Ok((last - 1, 1.));
        }
        let interval = upper - 1;
        let progress = (frame - self.times[interval]) / (self.times[upper] - self.times[interval]);
        Ok((interval, progress))
    }
}
