//! Shared linear scalar arithmetic, without colour, alpha or validity policy.
use std::ops::{Add, Mul, Sub};

/// The existing graph Lerp operation, preserving its input arithmetic type and
/// allowing extrapolation. Resource owners validate weights and decide which
/// invalid operands may be skipped at endpoints before calling this kernel.
pub fn lerp<T>(reference: T, value: T, factor: T) -> T
where
    T: Copy + Add<Output = T> + Sub<Output = T> + Mul<Output = T>,
{
    reference + (value - reference) * factor
}
