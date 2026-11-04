use super::{FrameRate, RateError};

/// Author-entered decimal values are exact decimal rates, never NTSC aliases.
/// The JSON wire contract remains the required numerator/denominator object.
impl std::str::FromStr for FrameRate {
    type Err = RateError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let input = input.trim();
        if input.len() > 64 {
            return Err(RateError::Syntax);
        }
        let digits = |text: &str| -> Result<u128, RateError> {
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(RateError::Syntax);
            }
            text.parse().map_err(|_| RateError::NumericRange)
        };
        let (mut numerator, mut denominator) = if let Some((left, right)) = input.split_once('/') {
            (digits(left)?, digits(right)?)
        } else if let Some((whole, fraction)) = input.split_once('.') {
            let whole = digits(whole)?;
            let fraction_value = digits(fraction)?;
            let denominator = 10u128
                .checked_pow(fraction.len() as u32)
                .ok_or(RateError::NumericRange)?;
            (
                whole
                    .checked_mul(denominator)
                    .and_then(|value| value.checked_add(fraction_value))
                    .ok_or(RateError::NumericRange)?,
                denominator,
            )
        } else {
            (digits(input)?, 1)
        };
        if numerator == 0 || denominator == 0 {
            return Err(RateError::Zero);
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        numerator /= a;
        denominator /= a;
        Self::new(
            u32::try_from(numerator).map_err(|_| RateError::ComponentRange)?,
            u32::try_from(denominator).map_err(|_| RateError::ComponentRange)?,
        )
    }
}
