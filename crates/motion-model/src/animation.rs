use crate::{ensure, Curve, Easing, Result};
use serde::{Deserialize, Serialize};

#[path = "animation/automatic.rs"]
mod automatic;
#[path = "animation/path_metric.rs"]
mod path_metric;
pub use path_metric::{SpatialPath, SpatialPathBudget, SpatialPathSample};
#[path = "animation/spatial_schedule.rs"]
mod spatial_schedule;
pub use spatial_schedule::{PreparedSpatialTiming, SpatialKeyTime, SpatialKeyTimes};
#[path = "animation/spatial_animation.rs"]
mod spatial_animation;
#[path = "animation/track_serde.rs"]
mod track_serde;
pub use spatial_animation::{SpatialAnimation, SpatialAnimationKey, SpatialCacheUsage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ease {
    #[default]
    Linear,
    In,
    Out,
    InOut,
    Hold,
}

impl Ease {
    pub fn map(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::In => t * t,
            Self::Out => 1.0 - (1.0 - t) * (1.0 - t),
            Self::InOut => t * t * (3.0 - 2.0 * t),
            Self::Hold => {
                if t < 1.0 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

pub trait Tween: Copy + PartialEq {
    fn mix(self, other: Self, t: f32) -> Self;
    fn finite(self) -> bool;
    fn bounded(self, bound: f32) -> bool;
    fn add(self, other: Self) -> Self;
    fn spatial_supported() -> bool { false }
    /// Spatial timing is restricted to explicit 2D/3D geometry. A four-channel
    /// color or packed parameter must be projected by its semantic owner first.
    fn spatial_dimensions() -> Option<usize> { None }
    fn from_spatial_components(_: [f32;3]) -> Option<Self> { None }
    fn spatial_components(self) -> Option<[f32;3]> { None }
    fn components(self) -> Option<[f32; 3]> {
        None
    }
    fn from_components(_: [f32; 3]) -> Option<Self> {
        None
    }
    /// Signed scalar delta, or Euclidean distance in stored vector coordinates.
    /// Non-numeric track types deliberately provide no automatic timing metric.
    fn timing_delta(self, _other: Self) -> Option<f64> { None }
}
impl Tween for f32 {
    fn timing_delta(self, other: Self) -> Option<f64> {
        Some(f64::from(other) - f64::from(self))
    }
    fn add(self, other: Self) -> Self { (f64::from(self) + f64::from(other)) as f32 }
    fn bounded(self, bound: f32) -> bool {
        self.is_finite() && self.abs() <= bound
    }
    fn mix(self, other: Self, t: f32) -> Self {
        // Avoid overflow in (other - self) for opposite finite endpoints.
        (f64::from(self) * f64::from(1.0 - t) + f64::from(other) * f64::from(t)) as f32
    }
    fn finite(self) -> bool {
        self.is_finite()
    }
}
impl<const N: usize> Tween for [f32; N] {
    fn timing_delta(self, other: Self) -> Option<f64> {
        (2..=4).contains(&N).then(|| self.into_iter().zip(other)
            .fold(0f64, |length, (a,b)| length.hypot(f64::from(b)-f64::from(a))))
    }
    fn add(self, other: Self) -> Self { std::array::from_fn(|i| self[i].add(other[i])) }
    fn spatial_supported() -> bool { (2..=4).contains(&N) }
    fn spatial_dimensions() -> Option<usize> { (2..=3).contains(&N).then_some(N) }
    fn from_spatial_components(value:[f32;3])->Option<Self> {
        (2..=3).contains(&N).then(||std::array::from_fn(|i|value[i]))
    }
    fn spatial_components(self) -> Option<[f32;3]> {
        Self::spatial_supported().then(|| std::array::from_fn(|i| self.get(i).copied().unwrap_or(0.)))
    }
    fn bounded(self, bound: f32) -> bool {
        self.into_iter().all(|v| v.is_finite() && v.abs() <= bound)
    }
    fn components(self) -> Option<[f32; 3]> {
        self.as_slice().try_into().ok()
    }
    fn from_components(value: [f32; 3]) -> Option<Self> {
        value.as_slice().try_into().ok()
    }
    fn mix(self, other: Self, t: f32) -> Self {
        std::array::from_fn(|i| self[i].mix(other[i], t))
    }
    fn finite(self) -> bool {
        self.into_iter().all(f32::is_finite)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
pub struct SpatialTangents<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outgoing: Option<T>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
pub struct Keyframe<T> {
    pub frame: i32,
    pub value: T,
    pub ease: Ease,
    /// Persistent endpoint policy. Both incident intervals derive this key's
    /// temporal slope from its current neighbours; values/times remain authored.
    #[serde(default, skip_serializing_if = "automatic::disabled")]
    pub auto_bezier: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<Curve>,
    /// Offsets from this key's value, independent of temporal easing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial: Option<SpatialTangents<T>>,
}

/// Find the insertion point before an edit removes or changes its source key.
/// Value-setting APIs deliberately keep their separate upsert semantics.
pub(crate) fn vacant_key_index<T>(keys: &[Keyframe<T>], frame: i32) -> Result<usize> {
    keys.binary_search_by_key(&frame, |key| key.frame)
        .err()
        .ok_or_else(|| crate::Error::Invalid("keyframe destination is occupied".into()))
}

#[derive(Clone, Debug)]
pub struct Track<T> {
    pub value: T,
    pub keys: Vec<Keyframe<T>>,
    /// Present only after an explicit user edit. These scalar tracks are then
    /// authoritative; legacy vector keys are removed and are not serialized.
    pub axes: Option<Box<AxisTracks>>,
    /// Alternative authoritative storage. Ordinary keys/axes must be empty.
    pub spatial_animation: Option<Box<SpatialAnimation<T>>>,
}
impl<T: PartialEq> PartialEq for Track<T> {
    fn eq(&self, other: &Self) -> bool {
        match (&self.spatial_animation,&other.spatial_animation) {
            (Some(a),Some(b))=>return self.value==other.value && self.keys==other.keys && self.axes==other.axes && a==b,
            (None,None)=>{},
            _=>return false,
        }
        match (&self.axes, &other.axes) {
            (Some(a), Some(b)) => a == b,
            (None, None) => self.value == other.value && self.keys == other.keys,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    pub fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisTracks {
    pub x: Track<f32>,
    pub y: Track<f32>,
    pub z: Track<f32>,
}
impl AxisTracks {
    pub fn get(&self, axis: Axis) -> &Track<f32> {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
            Axis::Z => &self.z,
        }
    }
    pub fn get_mut(&mut self, axis: Axis) -> &mut Track<f32> {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
            Axis::Z => &mut self.z,
        }
    }
    pub fn tracks(&self) -> [&Track<f32>; 3] {
        [&self.x, &self.y, &self.z]
    }
}
impl<T: Tween + Serialize> Serialize for Track<T> {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        if let Some(spatial)=&self.spatial_animation {
            if !self.keys.is_empty() || self.axes.is_some() {
                return Err(serde::ser::Error::custom("spatial animation cannot coexist with ordinary keys or axes"));
            }
            let mut map=serializer.serialize_map(Some(2))?;
            map.serialize_entry("value",&self.value)?;
            map.serialize_entry("spatial_animation",spatial)?;
            return map.end();
        }
        let mut map = serializer.serialize_map(Some(if self.axes.is_some() { 1 } else { 2 }))?;
        if let Some(axes) = &self.axes {
            map.serialize_entry("axes", axes)?;
        } else {
            map.serialize_entry("value", &self.value)?;
            map.serialize_entry("keys", &self.keys)?;
        }
        map.end()
    }
}
impl<'de, T: Tween + Deserialize<'de>> Deserialize<'de> for Track<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_map(track_serde::Visitor(std::marker::PhantomData))
    }
}

impl<T: Tween> Track<T> {
    /// Project the authoritative static value, rejecting invalid separated storage.
    pub fn static_value(&self) -> Result<T> {
        self.validate_spatial_storage()?;
        match &self.axes {
            None => Ok(self.value),
            Some(a) => {
                ensure(
                    self.keys.is_empty(),
                    "separated tracks cannot contain vector keys",
                )?;
                ensure(
                    a.tracks().iter().all(|t| t.axes.is_none()),
                    "scalar axis tracks cannot contain separated dimensions",
                )?;
                T::from_components([a.x.value, a.y.value, a.z.value]).ok_or_else(|| {
                    crate::Error::Invalid(format!(
                        "track static value: separated dimensions require a three-component value type={}",
                        std::any::type_name::<T>().chars().take(128).collect::<String>(),
                    ))
                })
            },
        }
    }
    pub fn is_animated(&self) -> bool {
        self.spatial_animation.is_some() || !self.keys.is_empty()
            || self
                .axes
                .as_ref()
                .is_some_and(|a| a.tracks().iter().any(|t| !t.keys.is_empty()))
    }
    pub fn key_frames(&self) -> impl Iterator<Item = f64> + '_ {
        self.keys.iter().map(|k| f64::from(k.frame)).chain(
            self.axes
                .iter()
                .flat_map(|a| a.tracks())
                .flat_map(|t| t.keys.iter().map(|k| f64::from(k.frame))),
        ).chain(self.spatial_animation.iter().flat_map(|a|a.prepared().keys().map(|(_,time)|time)))
    }
    pub fn constant(value: T) -> Self {
        Self {
            value,
            keys: Vec::new(),
            axes: None,
            spatial_animation:None,
        }
    }
    /// Healthy sampling copies only scalars/arrays and allocates no memory.
    /// Non-finite results retain their value and produce bounded diagnostics.
    /// NaN time falls back to static values; hosts must reject it first.
    pub fn sample(&self, frame: f64) -> Result<T> {
        let value = self.sample_value(frame)?;
        if !value.finite() {
            static FAILURES: crate::observability::Failures = crate::observability::Failures::new();
            FAILURES.record("track.sample", || {
                format!(
                    "frame={frame} first_key={:?} last_key={:?} keys={} value_type={} reason=non_finite_sample",
                    self.keys.first().map(|k| k.frame),
                    self.keys.last().map(|k| k.frame),
                    self.keys.len(),
                    std::any::type_name::<T>().chars().take(128).collect::<String>(),
                )
            });
        }
        Ok(value)
    }
    fn sample_value(&self, frame: f64) -> Result<T> {
        if let Some(spatial)=&self.spatial_animation {
            self.validate_spatial_storage()?;
            if frame.is_nan() { return Ok(self.value); }
            if frame==f64::NEG_INFINITY { return Ok(spatial.keys()[0].value); }
            if frame==f64::INFINITY { return Ok(spatial.keys()[spatial.keys().len()-1].value); }
            return spatial.sample_value(frame);
        }
        let invalid = |phase: &str, reason: &str| crate::Error::Invalid(format!(
            "track sample: phase={phase} frame={frame} keys={} type={} {reason}",
            self.keys.len(), std::any::type_name::<T>().chars().take(128).collect::<String>(),
        ));
        if let Some(a) = &self.axes {
            if !self.keys.is_empty() {
                return Err(invalid("storage", "separated tracks cannot contain vector keys"));
            }
            if a.tracks().iter().any(|t| t.axes.is_some()) {
                return Err(invalid("storage", "scalar axis tracks cannot contain separated dimensions"));
            }
            return T::from_components([
                a.x.sample_value(frame)?, a.y.sample_value(frame)?, a.z.sample_value(frame)?,
            ]).ok_or_else(|| invalid("reconstruct", "cannot reconstruct sampled dimensions"));
        }
        if frame.is_nan() {
            return Ok(self.value);
        }
        let Some(first) = self.keys.first() else {
            return Ok(self.value);
        };
        if frame <= f64::from(first.frame) {
            if first.auto_bezier { automatic::validate_key(&self.keys, first.frame)?; }
            return Ok(first.value);
        }
        let upper = self.keys.partition_point(|k| f64::from(k.frame) <= frame);
        if upper >= self.keys.len() {
            if let Some(last) = self.keys.last().filter(|k| k.auto_bezier) {
                automatic::validate_key(&self.keys, last.frame)?;
            }
            return self.keys.last().map(|key| key.value)
                .ok_or_else(|| invalid("interval", "sample endpoint is missing"));
        }
        let a = upper.checked_sub(1).and_then(|i| self.keys.get(i))
            .ok_or_else(|| invalid("interval", "sample predecessor is missing"))?;
        let b = self.keys.get(upper)
            .ok_or_else(|| invalid("interval", "sample successor is missing"))?;
        let t = ((frame - f64::from(a.frame)) / (f64::from(b.frame) - f64::from(a.frame))) as f32;
        let progress = self.effective_easing(upper - 1)?.sample(f64::from(t)).progress as f32;
        let outgoing = a.spatial.as_ref().and_then(|s| s.outgoing);
        let incoming = b.spatial.as_ref().and_then(|s| s.incoming);
        Ok(spatial_mix(a.value,b.value,outgoing,incoming,progress))
    }
    pub fn validate(&self, frame_count: u32) -> Result<()> {
        self.validate_range(frame_count as usize, Some(frame_count), true)
    }
    /// Layer keys use signed local time and survive trimming or moving outside
    /// the composition. Their count remains bounded independently of the clip.
    pub fn validate_local(&self) -> Result<()> {
        self.validate_range(crate::MAX_FRAMES as usize, None, true)
    }
    /// A clipboard contains selected metadata, not the source's neighbours.
    /// Validate intrinsic policy here; validate the complete context on paste.
    pub(crate) fn validate_clipboard(&self) -> Result<()> {
        self.validate_range(crate::MAX_FRAMES as usize, None, false)
    }
    fn validate_range(&self, max_keys: usize, frame_count: Option<u32>, context: bool) -> Result<()> {
        if let Some(spatial)=&self.spatial_animation {
            self.validate_spatial_storage()?;
            ensure(self.value.finite(),"track contains a non-finite static value")?;
            ensure(spatial.keys().len()<=max_keys,"too many spatial keys")?;
            if let Some(count)=frame_count {
                ensure(spatial.prepared().keys().all(|(_,time)|time>=0. && time<f64::from(count)),
                    "spatial keyframe outside the composition")?;
            }
            return Ok(());
        }
        if let Some(axes) = &self.axes {
            ensure(
                T::from_components([0.0; 3]).is_some(),
                "only vector properties can separate dimensions",
            )?;
            ensure(
                self.keys.is_empty(),
                "separated tracks cannot contain vector keys",
            )?;
            for t in axes.tracks() {
                t.validate_range(max_keys, frame_count, context)?;
            }
            return Ok(());
        }
        ensure(
            self.value.finite(),
            "track contains a non-finite static value",
        )?;
        ensure(self.keys.len() <= max_keys, "too many keys")?;
        let mut previous = None;
        for key in &self.keys {
            if let Some(count) = frame_count {
                ensure(
                    key.frame >= 0 && i64::from(key.frame) < i64::from(count),
                    "keyframe outside the composition",
                )?;
            }
            ensure(key.value.finite(), "keyframe contains a non-finite value")?;
            if key.auto_bezier {
                if context { automatic::validate_key(&self.keys, key.frame)?; }
                else { automatic::validate_metadata(key)?; }
            }
            if let Some(curve) = key.curve {
                curve.validate()?;
            }
            if let Some(s) = &key.spatial {
                ensure(T::spatial_supported(), "spatial tangents require a vector track")?;
                ensure(s.incoming.is_some() || s.outgoing.is_some(), "empty spatial tangents")?;
                for v in s.incoming.iter().chain(&s.outgoing) {
                    ensure(v.finite() && key.value.add(*v).finite(), "non-finite spatial tangent")?;
                }
            }
            ensure(
                previous.is_none_or(|p| p < key.frame),
                "keyframes must be unique and sorted",
            )?;
            previous = Some(key.frame);
        }
        Ok(())
    }
    pub fn upsert(&mut self, frame: impl TryInto<i32>, value: T, ease: Ease) -> Result<()> {
        let frame = stored_frame(frame)?;
        ensure(value.finite(), "non-finite property value")?;
        self.validate_spatial_storage()?;
        self.require_frame_keys()?;
        if let Some(axes) = &self.axes {
            self.validate_separated_storage()?;
            let v = value
                .components()
                .ok_or_else(|| crate::Error::Invalid("expected vector value".into()))?;
            ensure(T::from_components(v).is_some(), "expected reconstructible vector value")?;
            let mut next = axes.clone();
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                next.get_mut(axis).upsert(frame, v[axis.index()], ease)?;
            }
            self.axes = Some(next);
            return Ok(());
        }
        let key = Keyframe {
            auto_bezier: false,
            frame,
            value,
            ease,
            curve: None,
            spatial: None,
        };
        match self.keys.binary_search_by_key(&frame, |k| k.frame) {
            Ok(index) => {
                let mut key = key;
                key.spatial = self.keys[index].spatial.clone();
                key.auto_bezier = self.keys[index].auto_bezier;
                self.keys[index] = key;
            },
            Err(index) => self.keys.insert(index, key),
        }
        Ok(())
    }
    pub fn set_at(&mut self, frame: impl TryInto<i32>, value: T) -> Result<()> {
        let frame = stored_frame(frame)?;
        ensure(value.finite(), "non-finite property value")?;
        self.validate_spatial_storage()?;
        if let Some(spatial)=&mut self.spatial_animation {
            return spatial.set_at_time(f64::from(frame),value);
        }
        if let Some(axes) = &self.axes {
            self.validate_separated_storage()?;
            let v = value
                .components()
                .ok_or_else(|| crate::Error::Invalid("expected vector value".into()))?;
            ensure(T::from_components(v).is_some(), "expected reconstructible vector value")?;
            let mut next = axes.clone();
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                next.get_mut(axis).set_at(frame, v[axis.index()])?;
            }
            self.axes = Some(next);
            return Ok(());
        }
        if self.keys.is_empty() {
            self.value = value;
            return Ok(());
        }
        if let Ok(index) = self.keys.binary_search_by_key(&frame, |k| k.frame) {
            self.keys[index].value = value;
            Ok(())
        } else {
            self.upsert(frame, value, Ease::Linear)
        }
    }
    pub fn set_animated(&mut self, frame: impl TryInto<i32>, enabled: bool) -> Result<()> {
        let frame = stored_frame(frame)?;
        if self.spatial_animation.is_some() {
            self.validate_spatial_storage()?;
            if !enabled {
                let value=self.sample(f64::from(frame))?;
                self.spatial_animation=None;
                self.value=value;
            }
            return Ok(());
        }
        if let Some(a) = &self.axes {
            if enabled {
                ensure(
                    a.tracks().iter().all(|t| t.is_animated())
                        || a.tracks().iter().all(|t| !t.is_animated()),
                    "whole animation edit requires matching axis animation states",
                )?;
            }
        }
        if self.edit_axes(|t| t.set_animated(frame, enabled))? {
            return Ok(());
        }
        let value = self.sample(f64::from(frame))?;
        if enabled && self.keys.is_empty() {
            self.upsert(frame, value, Ease::Linear)?;
        } else if !enabled {
            self.keys.clear();
            self.value = value;
        }
        Ok(())
    }
    pub fn move_key(&mut self, from: i32, to: i32) -> Result<()> {
        self.require_frame_keys()?;
        self.validate_axis_collision(to)?;
        if self.edit_axes(|t| t.move_key(from, to))? {
            return Ok(());
        }
        if from == to {
            return ensure(
                self.keys.iter().any(|k| k.frame == from),
                "keyframe not found",
            );
        }
        let index = self
            .keys
            .binary_search_by_key(&from, |k| k.frame)
            .map_err(|_| crate::Error::Invalid("keyframe not found".into()))?;
        let destination = vacant_key_index(&self.keys, to)?;
        let mut key = self.keys.remove(index);
        key.frame = to;
        // Removing a source before the destination shifts its insertion point.
        self.keys
            .insert(destination - usize::from(index < destination), key);
        Ok(())
    }
    pub fn delete_key(&mut self, frame: i32) -> Result<()> {
        self.require_frame_keys()?;
        if self.edit_axes(|t| t.delete_key(frame))? {
            return Ok(());
        }
        let index = self
            .keys
            .binary_search_by_key(&frame, |k| k.frame)
            .map_err(|_| crate::Error::Invalid("keyframe not found".into()))?;
        // Removing the last key keeps its sampled value instead of jumping to
        // the obsolete static value from before animation was enabled.
        if self.keys.len() == 1 {
            self.value = self.keys[index].value;
        }
        self.keys.remove(index);
        if self.keys.len() == 1 {
            self.keys[0].auto_bezier = false;
        }
        Ok(())
    }
    pub fn copy_key(&mut self, from: i32, to: i32) -> Result<()> {
        self.require_frame_keys()?;
        self.validate_axis_collision(to)?;
        if self.edit_axes(|t| t.copy_key(from, to))? {
            return Ok(());
        }
        let mut key = self
            .keys
            .iter()
            .find(|k| k.frame == from)
            .ok_or_else(|| crate::Error::Invalid("keyframe not found".into()))?
            .clone();
        if from == to {
            return Ok(());
        }
        key.frame = to;
        self.insert_key(key)
    }
    pub fn set_ease(&mut self, frame: i32, ease: Ease) -> Result<()> {
        self.require_frame_keys()?;
        if self.edit_axes(|t| t.set_ease(frame, ease))? {
            return Ok(());
        }
        let index = self
            .keys
            .binary_search_by_key(&frame, |k| k.frame)
            .map_err(|_| crate::Error::Invalid("keyframe not found".into()))?;
        if self.keys[index].auto_bezier {
            self.set_auto_bezier(frame, false)?;
        }
        self.keys[index].ease = ease;
        self.keys[index].curve = None;
        Ok(())
    }
    fn insert_key(&mut self, key: Keyframe<T>) -> Result<()> {
        let index = vacant_key_index(&self.keys, key.frame)?;
        self.keys.insert(index, key);
        Ok(())
    }
    pub fn set_curve(&mut self, frame: i32, easing: Easing) -> Result<()> {
        self.set_curve_sides(frame, easing, true, true)
    }
    /// The interval's selected endpoint(s) become manual. The opposite
    /// endpoint retains its automatic policy and is still derived at sampling.
    pub fn set_curve_sides(&mut self, frame: i32, easing: Easing, start: bool, end: bool) -> Result<()> {
        self.require_frame_keys()?;
        ensure(start || end, "choose a temporal endpoint to edit")?;
        if self.edit_axes(|t| t.set_curve_sides(frame, easing, start, end))? {
            return Ok(());
        }
        easing.validate()?;
        let index = self
            .keys
            .binary_search_by_key(&frame, |k| k.frame)
            .map_err(|_| crate::Error::Invalid("keyframe not found".into()))?;
        ensure(
            index + 1 < self.keys.len(),
            "curve needs two adjacent keyframes",
        )?;
        if self.keys[index].auto_bezier || self.keys[index + 1].auto_bezier {
            let mut next = self.clone();
            for key in start.then_some(index).into_iter().chain(end.then_some(index + 1)) {
                let frame = next.keys[key].frame;
                if next.keys[key].auto_bezier { next.set_auto_bezier(frame, false)?; }
            }
            next.keys[index].ease = easing.ease;
            next.keys[index].curve = easing.curve;
            *self = next;
            return Ok(());
        }
        self.keys[index].ease = easing.ease;
        self.keys[index].curve = easing.curve;
        Ok(())
    }
    fn validate_axis_collision(&self, frame: i32) -> Result<()> {
        if let Some(a) = &self.axes {
            let occupied = a
                .tracks()
                .iter()
                .filter(|t| t.keys.iter().any(|k| k.frame == frame))
                .count();
            ensure(
                occupied == 0 || occupied == 3,
                "whole key edit would overwrite only some axes",
            )?;
        }
        Ok(())
    }
    /// Validate/edit a candidate before replacing any axis. No partial whole
    /// operation and no artificial keys to make a missing source key exist.
    fn edit_axes(&mut self, edit: impl Fn(&mut Track<f32>) -> Result<()>) -> Result<bool> {
        let Some(axes) = &self.axes else {
            return Ok(false);
        };
        self.validate_separated_storage()?;
        let mut next = axes.clone();
        edit(&mut next.x)?;
        edit(&mut next.y)?;
        edit(&mut next.z)?;
        self.axes = Some(next);
        Ok(true)
    }
    fn validate_separated_storage(&self) -> Result<()> {
        self.validate_local()?;
        self.static_value()?;
        Ok(())
    }
    pub fn separate(&mut self) -> Result<()> {
        self.require_frame_keys()?;
        self.validate_local()?;
        if self.axes.is_some() {
            self.static_value()?;
            return Ok(());
        }
        ensure(self.keys.iter().all(|k| k.spatial.is_none()), "reset the spatial path before separating dimensions")?;
        ensure(self.keys.iter().all(|k| !k.auto_bezier), "freeze automatic Bezier before separating dimensions")?;
        let value = self.value.components().ok_or_else(|| {
            crate::Error::Invalid("only vector properties can separate dimensions".into())
        })?;
        ensure(T::from_components(value).is_some(), "only vector properties can separate dimensions")?;
        let mut separated: [Track<f32>; 3] = std::array::from_fn(|index| Track {
            spatial_animation:None,
            value: value[index],
            axes: None,
            keys: Vec::with_capacity(self.keys.len()),
        });
        for (index, key) in self.keys.iter().enumerate() {
            let context = |reason: &str| crate::Error::Invalid(format!(
                "track separate: key index={index} frame={} type={} {reason}",
                key.frame, std::any::type_name::<T>().chars().take(128).collect::<String>(),
            ));
            let value = key.value.components().ok_or_else(|| context("cannot expose three components"))?;
            if T::from_components(value).is_none() {
                return Err(context("cannot reconstruct three components"));
            }
            for (axis, value) in separated.iter_mut().zip(value) {
                axis.keys.push(Keyframe {
                    auto_bezier: false,
                    frame: key.frame,
                    value,
                    ease: key.ease,
                    curve: key.curve,
                    spatial: None,
                });
            }
        }
        for axis in &separated {
            axis.validate_local()?;
        }
        let [x, y, z] = separated;
        self.axes = Some(Box::new(AxisTracks { x, y, z }));
        self.keys.clear();
        Ok(())
    }
    pub fn axis_mut(&mut self, axis: Axis) -> Result<&mut Track<f32>> {
        if self.axes.is_some() {
            self.validate_separated_storage()?;
        }
        self.axes.as_mut().map(|a| a.get_mut(axis)).ok_or_else(|| {
            crate::Error::Invalid("separate dimensions before editing an axis".into())
        })
    }
    /// Recombine aligned scalar tracks without resampling or changing their
    /// arithmetic. Different key grids or segment timing require an explicit
    /// conversion policy; this command leaves them intact instead of baking.
    pub fn merge_dimensions(&mut self) -> Result<()> {
        let vector = |values| T::from_components(values).ok_or_else(|| {
            crate::Error::Invalid("only vector properties can merge dimensions".into())
        });
        vector([0.; 3])?;
        let Some(axes) = &self.axes else {
            return Ok(());
        };
        self.validate_local()?;
        ensure(axes.tracks().iter().all(|t|t.keys.iter().all(|k|!k.auto_bezier)),
            "freeze automatic Bezier before merging dimensions")?;
        ensure(
            axes.x.keys.len() == axes.y.keys.len() && axes.x.keys.len() == axes.z.keys.len(),
            "cannot merge dimensions: X/Y/Z must have matching key times",
        )?;
        let value = vector([axes.x.value, axes.y.value, axes.z.value])?;
        let mut keys = Vec::with_capacity(axes.x.keys.len());
        for (index, ((x, y), z)) in axes.x.keys.iter().zip(&axes.y.keys).zip(&axes.z.keys).enumerate() {
            ensure(
                x.frame == y.frame && x.frame == z.frame,
                "cannot merge dimensions: X/Y/Z must have matching key times",
            )?;
            // The final key has no outgoing segment. A custom curve overrides
            // its key's ease, so only effective timing needs to match.
            if index + 1 < axes.x.keys.len() {
                ensure(
                    x.curve == y.curve && x.curve == z.curve
                        && (x.curve.is_some() || (x.ease == y.ease && x.ease == z.ease)),
                    "cannot merge dimensions: X/Y/Z must share each segment's interpolation",
                )?;
            }
            keys.push(Keyframe {
                auto_bezier: false,
                frame: x.frame,
                value: vector([x.value, y.value, z.value])?,
                ease: x.ease,
                curve: x.curve,
                spatial: None,
            });
        }
        // Install only after every check; model callers get atomic rejection too.
        self.value = value;
        self.keys = keys;
        self.axes = None;
        Ok(())
    }
    pub fn validate_bound(&self, bound: f32) -> Result<()> {
        if let Some(spatial)=&self.spatial_animation {
            self.validate_spatial_storage()?;
            ensure(self.value.bounded(bound),"transform value exceeds its numeric range")?;
            return spatial.validate_bound(bound);
        }
        if let Some(axes) = &self.axes {
            for t in axes.tracks() {
                t.validate_bound(bound)?;
            }
        } else {
            for v in std::iter::once(self.value).chain(self.keys.iter().map(|k| k.value)) {
                ensure(
                    v.bounded(bound),
                    "transform value exceeds its numeric range",
                )?;
            }
            for k in &self.keys {
                if let Some(s) = &k.spatial {
                    for v in s.incoming.iter().chain(&s.outgoing) {
                        ensure(k.value.add(*v).bounded(bound), "spatial control exceeds its numeric range")?;
                    }
                }
            }
        }
        Ok(())
    }
    pub fn set_spatial(&mut self, frame: i32, tangents: Option<SpatialTangents<T>>) -> Result<()> {
        self.require_frame_keys()?;
        ensure(T::spatial_supported() && self.axes.is_none(), "spatial paths require a coupled vector track")?;
        let index = self.keys.binary_search_by_key(&frame, |k| k.frame)
            .map_err(|_| crate::Error::Invalid("spatial keyframe not found".into()))?;
        if let Some(s) = &tangents {
            ensure(self.keys[index.saturating_sub(1)..(index+2).min(self.keys.len())]
                .iter().all(|key| !key.auto_bezier),
                "freeze automatic temporal Bezier before editing a manual spatial path")?;
            ensure(s.incoming.is_some() || s.outgoing.is_some(), "empty spatial tangents")?;
            ensure(s.incoming.is_none() || index > 0, "first key has no incoming segment")?;
            ensure(s.outgoing.is_none() || index + 1 < self.keys.len(), "last key has no outgoing segment")?;
            for v in s.incoming.iter().chain(&s.outgoing) {
                ensure(v.finite() && self.keys[index].value.add(*v).finite(), "non-finite spatial tangent")?;
            }
        }
        self.keys[index].spatial = tangents;
        Ok(())
    }
}
fn stored_frame(frame: impl TryInto<i32>) -> Result<i32> {
    frame
        .try_into()
        .map_err(|_| crate::Error::Invalid("local keyframe time overflow".into()))
}

fn spatial_mix<T:Tween>(a:T,b:T,outgoing:Option<T>,incoming:Option<T>,progress:f32)->T {
    if outgoing.is_none() && incoming.is_none() { return a.mix(b,progress); }
    let c1=outgoing.map_or_else(||a.mix(b,1.0/3.0),|v|a.add(v));
    let c2=incoming.map_or_else(||a.mix(b,2.0/3.0),|v|b.add(v));
    let p0=a.mix(c1,progress);
    let p1=c1.mix(c2,progress);
    let p2=c2.mix(b,progress);
    p0.mix(p1,progress).mix(p1.mix(p2,progress),progress)
}
