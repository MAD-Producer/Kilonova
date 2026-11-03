//! Exact positive clock rates. Project adoption and timecode are separate layers.
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

pub const MAX_RATE_COMPONENT: u32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateError {
    Zero,
    ComponentRange,
    Timebase,
    NumericRange,
}
impl fmt::Display for RateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Zero => "frame rate requires positive numerator and denominator",
            Self::ComponentRange => "reduced frame-rate component exceeds 1000000",
            Self::Timebase => "clock timebase must be in 1..=1000000000",
            Self::NumericRange => "clock conversion exceeds signed 64-bit range",
        })
    }
}
impl std::error::Error for RateError {}

/// Rounding is explicit at integer media/sample/frame boundaries. Renderer
/// fractional-frame evaluation must not pass through these quantizing methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockRounding {
    Floor,
    Ceil,
    NearestTiesAwayFromZero,
}

/// Canonical, hashable rate. 2997/100 and 30000/1001 remain distinct clocks.
/// Bounds apply after reduction; no decimal FPS inference or legacy default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}
fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn rounded(n: i128, d: i128, rounding: ClockRounding) -> Result<i64, RateError> {
    let floor = n.div_euclid(d);
    let remainder = n.rem_euclid(d);
    let value = match rounding {
        ClockRounding::Floor => floor,
        ClockRounding::Ceil => floor + i128::from(remainder != 0),
        ClockRounding::NearestTiesAwayFromZero => {
            floor + i128::from(remainder * 2 > d || (remainder * 2 == d && n >= 0))
        }
    };
    i64::try_from(value).map_err(|_| RateError::NumericRange)
}
fn timebase(ticks_per_second: u32) -> Result<i128, RateError> {
    if !(1..=1_000_000_000).contains(&ticks_per_second) {
        return Err(RateError::Timebase);
    }
    Ok(i128::from(ticks_per_second))
}
impl FrameRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, RateError> {
        if numerator == 0 || denominator == 0 {
            return Err(RateError::Zero);
        }
        let common = gcd(numerator, denominator);
        let numerator = numerator / common;
        let denominator = denominator / common;
        if numerator > MAX_RATE_COMPONENT || denominator > MAX_RATE_COMPONENT {
            return Err(RateError::ComponentRange);
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }
    pub fn integer(fps: u32) -> Result<Self, RateError> {
        Self::new(fps, 1)
    }
    pub fn ratio(self) -> [u32; 2] {
        [self.numerator, self.denominator]
    }
    /// Presentation/shader approximation only. Exact boundary conversion uses
    /// integer ratios below, never accumulated floating-point frame durations.
    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }
    /// Original signed frame index -> clock ticks, with no per-frame summation.
    pub fn ticks_at_frame(
        self,
        frame: i64,
        ticks_per_second: u32,
        rounding: ClockRounding,
    ) -> Result<i64, RateError> {
        let ticks = timebase(ticks_per_second)?;
        // i64 * reduced u32<=1e6 * ticks<=1e9 fits i128, including MIN.
        rounded(
            i128::from(frame) * i128::from(self.denominator) * ticks,
            i128::from(self.numerator),
            rounding,
        )
    }
    pub fn frame_at_tick(
        self,
        tick: i64,
        ticks_per_second: u32,
        rounding: ClockRounding,
    ) -> Result<i64, RateError> {
        let ticks = timebase(ticks_per_second)?;
        rounded(
            i128::from(tick) * i128::from(self.numerator),
            ticks * i128::from(self.denominator),
            rounding,
        )
    }
    /// Quantized edit/clip boundary only; fractional nested rendering needs
    /// the unrounded rational mapping and must not use this operation.
    pub fn rescale_frame(
        self,
        frame: i64,
        target: Self,
        rounding: ClockRounding,
    ) -> Result<i64, RateError> {
        rounded(
            i128::from(frame) * i128::from(self.denominator) * i128::from(target.numerator),
            i128::from(self.numerator) * i128::from(target.denominator),
            rounding,
        )
    }
}
impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireRate {
            numerator: u32,
            denominator: u32,
        }
        let wire = WireRate::deserialize(deserializer)?;
        Self::new(wire.numerator, wire.denominator).map_err(serde::de::Error::custom)
    }
}
