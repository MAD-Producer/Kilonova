//! Explicit geometric aperture in composition-world units. No implicit metre
//! conversion is inferred from any model or relative-depth source.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthOfField {
    /// Axial distance from the resolved camera eye, in world units.
    pub focus_distance: Track<f32>,
    /// Entrance pupil diameter in world units; zero disables the aperture.
    pub aperture_diameter: Track<f32>,
    /// Bounded disk-gather radius in output pixels, independently of viewport size.
    pub max_radius_pixels: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampledDepthOfField {
    pub focus_distance: f32,
    pub aperture_diameter: f32,
    pub max_radius_pixels: f32,
}

impl DepthOfField {
    pub(super) fn validate(&self, frames: Option<u32>) -> Result<()> {
        for track in [&self.focus_distance, &self.aperture_diameter] {
            if let Some(frames) = frames {
                track.validate(frames)?;
            } else {
                track.validate_local()?;
            }
        }
        for focus in std::iter::once(self.focus_distance.value)
            .chain(self.focus_distance.keys.iter().map(|key| key.value))
        {
            ensure(
                focus.is_finite() && (0.001..=10_000_000.).contains(&focus),
                "depth of field focus distance must be 0.001..10000000 world units",
            )?;
        }
        for aperture in std::iter::once(self.aperture_diameter.value)
            .chain(self.aperture_diameter.keys.iter().map(|key| key.value))
        {
            ensure(
                aperture.is_finite() && (0. ..=100_000.).contains(&aperture),
                "depth of field aperture diameter must be 0..100000 world units",
            )?;
        }
        self.sample(0.)?.validate()
    }
    pub fn sample(&self, frame: f64) -> Result<SampledDepthOfField> {
        let sample = SampledDepthOfField {
            focus_distance: self.focus_distance.sample(frame)?,
            aperture_diameter: self.aperture_diameter.sample(frame)?,
            max_radius_pixels: self.max_radius_pixels,
        };
        sample.validate()?;
        Ok(sample)
    }
}
impl SampledDepthOfField {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.focus_distance.is_finite() && (0.001..=10_000_000.).contains(&self.focus_distance),
            "depth of field focus distance must be 0.001..10000000 world units",
        )?;
        ensure(
            self.aperture_diameter.is_finite()
                && (0. ..=100_000.).contains(&self.aperture_diameter),
            "depth of field aperture diameter must be 0..100000 world units",
        )?;
        ensure(
            self.max_radius_pixels.is_finite() && (0.5..=16.).contains(&self.max_radius_pixels),
            "depth of field maximum radius must be 0.5..16 output pixels",
        )
    }
}
