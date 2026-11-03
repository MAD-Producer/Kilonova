//! Independently authored frame labels, following public ST 12-1 counting rules.
//! This is an editing/display format, not the packed LTC/VITC frame-pair wire.
//! It never drops pictures or changes a media/renderer clock. Durations extend
//! beyond 24 hours without wrapping; a leading minus denotes a negative frame.
use crate::FrameRate;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimecodeMode {
    NonDrop,
    Drop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimecodeError {
    UnsupportedRate,
    Syntax,
    FieldRange,
    DroppedLabel,
    NumericRange,
}
impl fmt::Display for TimecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedRate => "timecode requires an integer or exact x000/1001 rate; drop mode requires 30000/1001 or 60000/1001",
            Self::Syntax => "timecode requires hours:minutes:seconds:frames or drop-mode hours:minutes:seconds;frames",
            Self::FieldRange => "timecode field outside the selected nominal rate or minute/second range",
            Self::DroppedLabel => "timecode uses a label omitted at the start of this drop-frame minute",
            Self::NumericRange => "timecode exceeds signed 64-bit frame range",
        })
    }
}
impl std::error::Error for TimecodeError {}

impl FrameRate {
    fn label_clock(self, mode: TimecodeMode) -> Result<(u128, u128), TimecodeError> {
        let [numerator, denominator] = self.ratio();
        if !self.is_between(1, crate::MAX_COMPOSITION_FPS) {
            return Err(TimecodeError::UnsupportedRate);
        }
        if mode == TimecodeMode::Drop {
            return match [numerator, denominator] {
                [30000, 1001] => Ok((30, 2)),
                [60000, 1001] => Ok((60, 4)),
                _ => Err(TimecodeError::UnsupportedRate),
            };
        }
        if denominator == 1 {
            return Ok((u128::from(numerator), 0));
        }
        let adjusted = u128::from(numerator) * 1001;
        let divisor = u128::from(denominator) * 1000;
        if adjusted % divisor != 0 {
            return Err(TimecodeError::UnsupportedRate);
        }
        Ok((adjusted / divisor, 0))
    }

    pub fn format_timecode(self, frame: i64, mode: TimecodeMode) -> Result<String, TimecodeError> {
        let (nominal, drop) = self.label_clock(mode)?;
        let mut label = u128::from(frame.unsigned_abs());
        if drop != 0 {
            let ten_minutes = nominal * 600 - drop * 9;
            let blocks = label / ten_minutes;
            let remainder = label % ten_minutes;
            label +=
                blocks * drop * 9 + drop * (remainder.saturating_sub(drop) / (nominal * 60 - drop));
        }
        let seconds = label / nominal;
        Ok(format!(
            "{}{:02}:{:02}:{:02}{}{:02}",
            if frame < 0 { "-" } else { "" },
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60,
            if mode == TimecodeMode::Drop { ';' } else { ':' },
            label % nominal
        ))
    }

    pub fn parse_timecode(self, text: &str, mode: TimecodeMode) -> Result<i64, TimecodeError> {
        let (nominal, drop) = self.label_clock(mode)?;
        if text.len() > 64 {
            return Err(TimecodeError::Syntax);
        }
        let (negative, text) = text
            .strip_prefix('-')
            .map_or((false, text), |text| (true, text));
        let separator = if mode == TimecodeMode::Drop { ';' } else { ':' };
        let (clock, frames) = text.rsplit_once(separator).ok_or(TimecodeError::Syntax)?;
        let mut parts = clock.split(':');
        let digits = |value: &str| -> Result<u128, TimecodeError> {
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(TimecodeError::Syntax);
            }
            value.parse().map_err(|_| TimecodeError::NumericRange)
        };
        let hours = digits(parts.next().ok_or(TimecodeError::Syntax)?)?;
        let minutes = digits(parts.next().ok_or(TimecodeError::Syntax)?)?;
        let seconds = digits(parts.next().ok_or(TimecodeError::Syntax)?)?;
        if parts.next().is_some() {
            return Err(TimecodeError::Syntax);
        }
        let frames = digits(frames)?;
        if minutes >= 60 || seconds >= 60 || frames >= nominal {
            return Err(TimecodeError::FieldRange);
        }
        if drop != 0 && minutes % 10 != 0 && seconds == 0 && frames < drop {
            return Err(TimecodeError::DroppedLabel);
        }
        let total_minutes = hours
            .checked_mul(60)
            .and_then(|v| v.checked_add(minutes))
            .ok_or(TimecodeError::NumericRange)?;
        let label = total_minutes
            .checked_mul(60)
            .and_then(|v| v.checked_add(seconds))
            .and_then(|v| v.checked_mul(nominal))
            .and_then(|v| v.checked_add(frames))
            .ok_or(TimecodeError::NumericRange)?;
        let omitted = drop
            .checked_mul(total_minutes - total_minutes / 10)
            .ok_or(TimecodeError::NumericRange)?;
        let frame = label
            .checked_sub(omitted)
            .ok_or(TimecodeError::NumericRange)?;
        if negative && frame == (1u128 << 63) {
            return Ok(i64::MIN);
        }
        let frame = i64::try_from(frame).map_err(|_| TimecodeError::NumericRange)?;
        Ok(if negative { -frame } else { frame })
    }
}
