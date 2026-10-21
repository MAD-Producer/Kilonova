use crate::Result;

pub(crate) fn checked_progress(value: Option<f64>) -> Result<Option<f64>> {
    if value.is_some_and(|value| !value.is_finite()) {
        return Err("invalid media task progress: expected a finite ratio or unknown".into());
    }
    Ok(value.map(|value| value.clamp(0.0, 1.0)))
}
