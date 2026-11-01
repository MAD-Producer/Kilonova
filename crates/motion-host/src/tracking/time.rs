//! Exact source-PTS lookup. Project clocks are supplied explicitly; this module
//! neither changes project fps nor interpolates observations across tracking gaps.
use super::asset::{self, Asset, Rate};
use std::{cmp::Ordering, fmt};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Timebase,
    Speed,
    NumericRange,
    Asset(asset::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Timebase => f.write_str("tracking timebase must be in 1..=1000000000"),
            Self::Speed => f.write_str(
                "tracking speed requires bounded signed numerator and positive denominator",
            ),
            Self::NumericRange => {
                f.write_str("tracking exact time arithmetic exceeded representable range")
            }
            Self::Asset(cause) => write!(f, "tracking time lookup: {cause}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Asset(cause) => Some(cause),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fraction {
    n: i128,
    d: i128,
}
fn gcd(mut a: u128, mut b: u128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    // Every caller includes a positive i128 denominator, so the gcd fits i128.
    a as i128
}
impl Fraction {
    fn new(n: i128, d: i128) -> Self {
        debug_assert!(d > 0);
        let g = gcd(n.unsigned_abs(), d as u128);
        Self { n: n / g, d: d / g }
    }
    fn integer(n: i64) -> Self {
        Self {
            n: i128::from(n),
            d: 1,
        }
    }
    fn combine(self, other: Self, subtract: bool) -> Result<Self, Error> {
        let g = gcd(self.d as u128, other.d as u128);
        let left = self.n.checked_mul(other.d / g).ok_or(Error::NumericRange)?;
        let right = other.n.checked_mul(self.d / g).ok_or(Error::NumericRange)?;
        let n = if subtract {
            left.checked_sub(right)
        } else {
            left.checked_add(right)
        }
        .ok_or(Error::NumericRange)?;
        let d = self.d.checked_mul(other.d / g).ok_or(Error::NumericRange)?;
        Ok(Self::new(n, d))
    }
    fn multiply(self, n: i128, d: i128) -> Result<Self, Error> {
        debug_assert!(d > 0);
        let g1 = gcd(self.n.unsigned_abs(), d as u128);
        let g2 = gcd(n.unsigned_abs(), self.d as u128);
        Ok(Self::new(
            (self.n / g1)
                .checked_mul(n / g2)
                .ok_or(Error::NumericRange)?,
            (self.d / g2)
                .checked_mul(d / g1)
                .ok_or(Error::NumericRange)?,
        ))
    }
    fn compare_integer(self, value: i64) -> Ordering {
        // Division avoids multiplying a large signed source PTS by a timebase.
        let whole = self.n.div_euclid(self.d);
        match whole.cmp(&i128::from(value)) {
            Ordering::Equal if self.n.rem_euclid(self.d) != 0 => Ordering::Greater,
            result => result,
        }
    }
}

/// Exact seconds on an explicitly declared clock, including negative origins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time(Fraction);
impl Time {
    pub fn ticks(value: i64, ticks_per_second: u32) -> Result<Self, Error> {
        if !(1..=1_000_000_000).contains(&ticks_per_second) {
            return Err(Error::Timebase);
        }
        Ok(Self(Fraction::new(
            i128::from(value),
            i128::from(ticks_per_second),
        )))
    }
    /// Frame time on a caller-supplied rational clock. Source nominal rate is
    /// deliberately never consulted when looking up actual source timestamps.
    pub fn frame(value: i64, frames_per_second: Rate) -> Self {
        let [n, d] = frames_per_second.ratio();
        Self(Fraction::new(
            i128::from(value) * i128::from(d),
            i128::from(n),
        ))
    }
}

/// Signed source-seconds per project-second. Zero explicitly freezes time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Speed {
    n: i128,
    d: i128,
}
impl Speed {
    pub fn new(numerator: i32, denominator: u32) -> Result<Self, Error> {
        if numerator.unsigned_abs() > 1_000_000 || !(1..=1_000_000).contains(&denominator) {
            return Err(Error::Speed);
        }
        let f = Fraction::new(i128::from(numerator), i128::from(denominator));
        Ok(Self { n: f.n, d: f.d })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Mapping {
    project_anchor: Time,
    source_anchor_pts: i64,
    speed: Speed,
}
impl Mapping {
    pub fn new(project_anchor: Time, source_anchor_pts: i64, speed: Speed) -> Self {
        Self {
            project_anchor,
            source_anchor_pts,
            speed,
        }
    }
    fn source_time(self, time: Time, source_ticks_per_second: u32) -> Result<Fraction, Error> {
        if self.speed.n == 0 {
            return Ok(Fraction::integer(self.source_anchor_pts));
        }
        let delta = time.0.combine(self.project_anchor.0, true)?;
        delta
            .multiply(self.speed.n, self.speed.d)?
            .multiply(i128::from(source_ticks_per_second), 1)?
            .combine(Fraction::integer(self.source_anchor_pts), false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Before,
    At(u32),
    /// Exact interpolation weight in source PTS space, strictly between 0 and 1.
    /// This is a temporal bracket, not permission to bridge lost/occluded data,
    /// sparse observations or manual discontinuities in any particular track.
    Between {
        before: u32,
        after: u32,
        numerator: u128,
        denominator: u128,
    },
    After,
}

/// Validates once, then borrows immutable PTS for allocation-free binary lookup.
/// Normal before/after-range results are dispositions, not logged failures.
pub struct Timeline<'a> {
    asset: &'a Asset,
    mapping: Mapping,
}
impl<'a> Timeline<'a> {
    pub fn new(asset: &'a Asset, mapping: Mapping) -> Result<Self, Error> {
        asset.validate().map_err(Error::Asset)?;
        Ok(Self { asset, mapping })
    }
    pub fn locate(&self, time: Time) -> Result<Location, Error> {
        let q = self
            .mapping
            .source_time(time, self.asset.ticks_per_second)?;
        let pts = &self.asset.source_pts;
        let after = pts.partition_point(|v| q.compare_integer(*v) == Ordering::Greater);
        if after == pts.len() {
            return Ok(Location::After);
        }
        if q.compare_integer(pts[after]) == Ordering::Equal {
            return Ok(Location::At(after as u32));
        }
        if after == 0 {
            return Ok(Location::Before);
        }
        let before = after - 1;
        let span = i128::from(pts[after]) - i128::from(pts[before]);
        let weight = q
            .combine(Fraction::integer(pts[before]), true)?
            .multiply(1, span)?;
        Ok(Location::Between {
            before: before as u32,
            after: after as u32,
            numerator: weight.n as u128,
            denominator: weight.d as u128,
        })
    }
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
