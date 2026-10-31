//! Continuous layer/property/source clocks. No decoder time-base rounding is
//! allowed here: it belongs at the final video/PCM boundary, after nesting.
use crate::{CurveSample, Ease, Easing, Error, Result};
use serde::{Deserialize, Serialize};

mod document;
mod media;
pub use document::{apply, LayerTimeAction, LayerTimeMapping};

pub const MAX_TIME_REMAP_KEYS: usize = crate::MAX_FRAMES as usize;
/// A rate bound, independent of the number of samples a consumer may prepare.
pub const MAX_TIME_STRETCH: u32 = 1_000;
const MAX_RATIO_PART: u32 = 1_000_000;
const MAX_LOCAL_FRAME: f64 = i32::MAX as f64;

fn invalid(phase: &'static str, message: &str) -> Error {
    Error::LayerTime {
        phase,
        source: Box::new(Error::Invalid(message.into())),
    }
}

fn check_time(frame: f64, phase: &'static str) -> Result<f64> {
    if frame.is_finite() && frame.abs() <= MAX_LOCAL_FRAME {
        Ok(frame)
    } else {
        Err(invalid(
            phase,
            "time is non-finite or outside the local frame range",
        ))
    }
}

/// Duration multiplier; 2/1 means 200% duration (half speed). Canonical, bounded
/// integers avoid storing a rounded reciprocal as the authored stretch value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Ratio", into = "Ratio")]
pub struct TimeStretch {
    numerator: u32,
    denominator: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ratio {
    numerator: u32,
    denominator: u32,
}

impl TryFrom<Ratio> for TimeStretch {
    type Error = Error;
    fn try_from(value: Ratio) -> Result<Self> {
        Self::new(value.numerator, value.denominator)
    }
}
impl From<TimeStretch> for Ratio {
    fn from(value: TimeStretch) -> Self {
        Self {
            numerator: value.numerator,
            denominator: value.denominator,
        }
    }
}
impl Default for TimeStretch {
    fn default() -> Self {
        Self {
            numerator: 1,
            denominator: 1,
        }
    }
}
impl TimeStretch {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self> {
        if numerator == 0
            || denominator == 0
            || numerator > MAX_RATIO_PART
            || denominator > MAX_RATIO_PART
            || u64::from(numerator) > u64::from(denominator) * u64::from(MAX_TIME_STRETCH)
            || u64::from(denominator) > u64::from(numerator) * u64::from(MAX_TIME_STRETCH)
        {
            return Err(invalid(
                "stretch",
                "duration ratio must be within 1/1000..1000 and have bounded positive parts",
            ));
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }
    pub fn ratio(self) -> [u32; 2] {
        [self.numerator, self.denominator]
    }
    pub fn duration_scale(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
    pub fn speed(self) -> f64 {
        f64::from(self.denominator) / f64::from(self.numerator)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeDirection {
    Forward,
    Reverse,
}
impl TimeDirection {
    fn sign(self) -> f64 {
        match self {
            Self::Forward => 1.,
            Self::Reverse => -1.,
        }
    }
}

/// An affine map from E = C - clip.offset to property time L. Trimming changes
/// visibility, never this map. Pivots are explicit so an edit can preserve the
/// current sampled property time without rewriting authored keys.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ClockDefinition", into = "ClockDefinition")]
pub struct LayerClock {
    definition: ClockDefinition,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClockDefinition {
    stretch: TimeStretch,
    direction: TimeDirection,
    elapsed_pivot: f64,
    local_pivot: f64,
}
impl TryFrom<ClockDefinition> for LayerClock {
    type Error = Error;
    fn try_from(value: ClockDefinition) -> Result<Self> {
        Self::new(
            value.stretch,
            value.direction,
            value.elapsed_pivot,
            value.local_pivot,
        )
    }
}
impl From<LayerClock> for ClockDefinition {
    fn from(value: LayerClock) -> Self {
        value.definition
    }
}
impl Default for LayerClock {
    fn default() -> Self {
        Self {
            definition: ClockDefinition {
                stretch: TimeStretch::default(),
                direction: TimeDirection::Forward,
                elapsed_pivot: 0.,
                local_pivot: 0.,
            },
        }
    }
}
impl LayerClock {
    pub fn new(
        stretch: TimeStretch,
        direction: TimeDirection,
        elapsed_pivot: f64,
        local_pivot: f64,
    ) -> Result<Self> {
        check_time(elapsed_pivot, "elapsed_pivot")?;
        check_time(local_pivot, "local_pivot")?;
        if stretch == TimeStretch::default()
            && direction == TimeDirection::Forward
            && elapsed_pivot == local_pivot
        {
            return Ok(Self::default());
        }
        Ok(Self {
            definition: ClockDefinition {
                stretch,
                direction,
                elapsed_pivot,
                local_pivot,
            },
        })
    }
    pub fn stretch(self) -> TimeStretch {
        self.definition.stretch
    }
    pub fn direction(self) -> TimeDirection {
        self.definition.direction
    }
    pub fn rate(self) -> f64 {
        self.definition.direction.sign() * self.definition.stretch.speed()
    }
    pub fn local_frame(self, composition_frame: f64, offset: i32) -> Result<f64> {
        check_time(composition_frame, "composition_input")?;
        check_time(
            self.sample_local_frame(composition_frame, offset),
            "property_output",
        )
    }
    /// Algebra used by the existing infallible Layer.local_frame query after
    /// project/request validation. No fallible result is discarded here.
    pub(crate) fn sample_local_frame(self, composition_frame: f64, offset: i32) -> f64 {
        let elapsed = composition_frame - f64::from(offset);
        // Preserve the old C-offset expression exactly for identity maps.
        if self.definition.stretch == TimeStretch::default()
            && self.definition.direction == TimeDirection::Forward
            && self.definition.elapsed_pivot == self.definition.local_pivot
        {
            elapsed
        } else {
            self.definition.local_pivot + (elapsed - self.definition.elapsed_pivot) * self.rate()
        }
    }
    /// Inverse PROPERTY time, deliberately independent of a possibly frozen or
    /// non-monotonic source remap. Keys retain their local storage identity.
    pub fn composition_frame(self, local_frame: f64, offset: i32) -> Result<f64> {
        check_time(local_frame, "property_input")?;
        let elapsed = if self == Self::default() {
            local_frame
        } else {
            self.definition.elapsed_pivot
                + (local_frame - self.definition.local_pivot) / self.rate()
        };
        check_time(elapsed + f64::from(offset), "composition_output")
    }
    /// Existing integer-track commands must explicitly refuse an unrepresentable
    /// local instant. Rounding would edit a different key or merge two keys.
    pub fn integer_key_frame(self, composition_frame: f64, offset: i32) -> Result<i32> {
        let local = self.local_frame(composition_frame, offset)?;
        if local.fract() != 0. {
            return Err(invalid(
                "integer_key",
                "local time requires continuous key addressing",
            ));
        }
        Ok(local as i32)
    }
    pub fn with_rate_at(
        self,
        stretch: TimeStretch,
        direction: TimeDirection,
        composition_frame: f64,
        offset: i32,
    ) -> Result<Self> {
        let local = self.local_frame(composition_frame, offset)?;
        Self::new(
            stretch,
            direction,
            composition_frame - f64::from(offset),
            local,
        )
    }
}

/// Stored in the same local frame units as property keys, but with f64 values
/// and instants. f32 source frame values lose PCM samples in long compositions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeRemapKey {
    pub frame: f64,
    pub source_frame: f64,
    pub easing: Easing,
}

/// Validated at construction/deserialization; immutable in sampling. A remap
/// never feeds its output back into itself. Outside keys, source time holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RemapDefinition", into = "RemapDefinition")]
pub struct TimeRemap {
    keys: Vec<TimeRemapKey>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemapDefinition {
    #[serde(deserialize_with = "deserialize_keys")]
    keys: Vec<TimeRemapKey>,
}

fn deserialize_keys<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<TimeRemapKey>, D::Error> {
    use serde::de::{self, DeserializeSeed, SeqAccess, Visitor};
    struct Keys;
    // Reject the first excess item before decoding/allocating its payload.
    struct KeySeed {
        available: bool,
    }
    impl<'de> DeserializeSeed<'de> for KeySeed {
        type Value = TimeRemapKey;
        fn deserialize<D: serde::Deserializer<'de>>(
            self,
            d: D,
        ) -> std::result::Result<Self::Value, D::Error> {
            if !self.available {
                return Err(de::Error::custom("source remap key budget exceeded"));
            }
            TimeRemapKey::deserialize(d)
        }
    }
    impl<'de> Visitor<'de> for Keys {
        type Value = Vec<TimeRemapKey>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a bounded source remap key array")
        }
        fn visit_seq<S: SeqAccess<'de>>(
            self,
            mut sequence: S,
        ) -> std::result::Result<Self::Value, S::Error> {
            let mut keys =
                Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(MAX_TIME_REMAP_KEYS));
            while let Some(key) = sequence.next_element_seed(KeySeed {
                available: keys.len() < MAX_TIME_REMAP_KEYS,
            })? {
                keys.push(key);
            }
            Ok(keys)
        }
    }
    d.deserialize_seq(Keys)
}
impl TryFrom<RemapDefinition> for TimeRemap {
    type Error = Error;
    fn try_from(value: RemapDefinition) -> Result<Self> {
        Self::new(value.keys)
    }
}
impl From<TimeRemap> for RemapDefinition {
    fn from(value: TimeRemap) -> Self {
        Self { keys: value.keys }
    }
}

/// A continuous coordinate and derivative. Keep this through nested clocks;
/// reverse endpoint quantization happens once, in the final media time base.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceTimeSample {
    pub frame: f64,
    pub rate: f64,
}
impl SourceTimeSample {
    pub fn new(frame: f64, rate: f64) -> Result<Self> {
        check_time(frame, "source_output")?;
        if !rate.is_finite() {
            return Err(invalid("source_rate", "source derivative is non-finite"));
        }
        Ok(Self { frame, rate })
    }
    /// Apply a child clock to an unquantized parent time. The chain rule also
    /// preserves two reversals as a forward source, including remap turns.
    pub fn through(
        self,
        clock: LayerClock,
        offset: i32,
        remap: Option<&TimeRemap>,
    ) -> Result<Self> {
        let local = clock.local_frame(self.frame, offset)?;
        let sample = match remap {
            Some(remap) => remap.sample(local)?,
            None => Self::new(local, 1.)?,
        };
        Self::new(sample.frame, self.rate * clock.rate() * sample.rate)
    }
}
impl TimeRemap {
    pub fn new(keys: Vec<TimeRemapKey>) -> Result<Self> {
        if keys.is_empty() || keys.len() > MAX_TIME_REMAP_KEYS {
            return Err(invalid(
                "remap_keys",
                "source remap key count is outside its budget",
            ));
        }
        let mut previous = None;
        for key in &keys {
            check_time(key.frame, "remap_key_time")?;
            check_time(key.source_frame, "remap_key_value")?;
            key.easing.validate().map_err(|source| Error::LayerTime {
                phase: "remap_easing",
                source: Box::new(source),
            })?;
            if previous.is_some_and(|time| key.frame <= time) {
                return Err(invalid(
                    "remap_key_order",
                    "source remap keys must have strictly increasing times",
                ));
            }
            previous = Some(key.frame);
        }
        Ok(Self { keys })
    }
    pub fn keys(&self) -> &[TimeRemapKey] {
        &self.keys
    }
    pub fn sample(&self, local_frame: f64) -> Result<SourceTimeSample> {
        check_time(local_frame, "remap_input")?;
        let upper = self.keys.partition_point(|key| key.frame <= local_frame);
        // Construction makes the first/last element total, even for one key.
        if upper == 0 {
            return SourceTimeSample::new(self.keys[0].source_frame, 0.);
        }
        let a = &self.keys[upper - 1];
        if upper == self.keys.len() {
            return SourceTimeSample::new(a.source_frame, 0.);
        }
        let b = &self.keys[upper];
        if a.source_frame == b.source_frame
            || (a.easing.curve.is_none() && a.easing.ease == Ease::Hold)
        {
            return SourceTimeSample::new(a.source_frame, 0.);
        }
        let duration = b.frame - a.frame;
        let sample = precise_easing(a.easing, (local_frame - a.frame) / duration);
        let delta = b.source_frame - a.source_frame;
        SourceTimeSample::new(
            if local_frame == a.frame {
                a.source_frame
            } else {
                a.source_frame + delta * sample.progress
            },
            if sample.velocity == 0. {
                0.
            } else {
                delta / duration * sample.velocity
            },
        )
    }
}

// Same ease definitions as ordinary properties, evaluated in f64 throughout.
// Existing property sampling/render semantics are intentionally unchanged.
fn precise_easing(easing: Easing, t: f64) -> CurveSample {
    if let Some(curve) = easing.curve {
        return curve.sample(t);
    }
    let t = t.clamp(0., 1.);
    let (progress, velocity) = match easing.ease {
        Ease::Linear => (t, 1.),
        Ease::In => (t * t, 2. * t),
        Ease::Out => (1. - (1. - t) * (1. - t), 2. * (1. - t)),
        Ease::InOut => (t * t * (3. - 2. * t), 6. * t * (1. - t)),
        Ease::Hold => (if t < 1. { 0. } else { 1. }, 0.),
    };
    CurveSample { progress, velocity }
}
