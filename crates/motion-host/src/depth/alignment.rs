//! Same-sample relative-depth alignment for an explicit quality transition.
//!
//! This module does not select a quality tier, infer capability from a backend,
//! enable automatic switching, or establish model accuracy. Callers supply
//! qualified bounds. A proof belongs to the two immutable resource instances;
//! it cannot be reused just because model/source metadata happen to match.

use motion_model::depth::{DepthMeasure, DepthResource, DepthResourceIdentity};

pub const VERSION: u32 = 1;
const MAX_FIT_PAIRS: usize = 4096;

/// All limits are explicit; resolution and backend names imply no quality bound.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub minimum_fit_pairs: usize,
    pub minimum_validation_pairs: usize,
    /// Fraction of the entire scalar grid with a valid, eligible paired sample.
    pub minimum_pair_fraction: f64,
    /// Require both confidence planes when supplied; absence is not confidence 1.
    pub minimum_confidence: Option<f32>,
    pub minimum_reference_stddev: f64,
    pub scale_bounds: [f64; 2],
    /// Residuals are divided by the fit reference population's standard deviation.
    pub maximum_normalized_rms: f64,
    pub maximum_normalized_error: f64,
}

#[derive(Debug, thiserror::Error)]
pub enum Rejection {
    #[error("invalid depth alignment limits: {0}")]
    InvalidLimits(&'static str),
    #[error("depth alignment requires compatible resources: {0}")]
    Incompatible(&'static str),
    #[error("depth alignment {population} has too few eligible pairs: {actual}/{minimum}")]
    InsufficientPairs {
        population: &'static str,
        actual: usize,
        minimum: usize,
    },
    #[error("depth alignment paired coverage={eligible}/{pixels} below fraction={minimum}")]
    Coverage {
        eligible: usize,
        pixels: usize,
        minimum: f64,
    },
    #[error("depth alignment is degenerate: {0}")]
    Degenerate(&'static str),
    #[error("depth alignment scale={actual} outside [{minimum}, {maximum}]")]
    Scale {
        actual: f64,
        minimum: f64,
        maximum: f64,
    },
    #[error(
        "depth alignment {population} residual rms={rms}/{rms_limit} max={maximum}/{maximum_limit}"
    )]
    Residual {
        population: &'static str,
        rms: f64,
        rms_limit: f64,
        maximum: f64,
        maximum_limit: f64,
    },
    #[error("depth alignment canceled")]
    Canceled,
    #[error("depth alignment proof belongs to different resource instances")]
    Stale,
}

type Result<T> = std::result::Result<T, Rejection>;

#[derive(Clone, Copy, Debug)]
pub struct Statistics {
    pub pixels: usize,
    pub fit_pairs: usize,
    pub validation_pairs: usize,
    pub skipped_pairs: usize,
    pub reference_stddev: f64,
    pub fit_normalized_rms: f64,
    pub fit_normalized_maximum: f64,
    pub validation_normalized_rms: f64,
    pub validation_normalized_maximum: f64,
}

#[derive(Clone, Debug)]
pub struct Alignment {
    reference: DepthResourceIdentity,
    candidate: DepthResourceIdentity,
    scale: f64,
    offset: f64,
    statistics: Statistics,
}

impl Limits {
    pub(crate) fn validate(self) -> Result<()> {
        if !(3..=MAX_FIT_PAIRS).contains(&self.minimum_fit_pairs)
            || self.minimum_validation_pairs < 3
        {
            return Err(Rejection::InvalidLimits("pair counts"));
        }
        if !self.minimum_pair_fraction.is_finite()
            || self.minimum_pair_fraction <= 0.
            || self.minimum_pair_fraction > 1.
        {
            return Err(Rejection::InvalidLimits("paired coverage fraction"));
        }
        if self
            .minimum_confidence
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(Rejection::InvalidLimits("confidence"));
        }
        let [low, high] = self.scale_bounds;
        if !low.is_finite() || !high.is_finite() || low <= 0. || high < low {
            return Err(Rejection::InvalidLimits("positive scale interval"));
        }
        if !self.minimum_reference_stddev.is_finite() || self.minimum_reference_stddev <= 0. {
            return Err(Rejection::InvalidLimits("reference standard deviation"));
        }
        if !self.maximum_normalized_rms.is_finite()
            || self.maximum_normalized_rms < 0.
            || !self.maximum_normalized_error.is_finite()
            || self.maximum_normalized_error < 0.
        {
            return Err(Rejection::InvalidLimits("residual bounds"));
        }
        Ok(())
    }
}

#[derive(Default)]
struct Moments {
    count: usize,
    x: f64,
    y: f64,
    xx: f64,
    yy: f64,
    xy: f64,
}
impl Moments {
    fn add(&mut self, x: f64, y: f64) {
        self.count += 1;
        let dx = x - self.x;
        let dy = y - self.y;
        self.x += dx / self.count as f64;
        self.y += dy / self.count as f64;
        self.xx += dx * (x - self.x);
        self.yy += dy * (y - self.y);
        self.xy += dx * (y - self.y);
    }
}

#[derive(Default)]
struct Residuals {
    count: usize,
    squared: f64,
    maximum: f64,
}
impl Residuals {
    fn add(&mut self, error: f64) -> Result<()> {
        self.count += 1;
        self.squared += error * error;
        self.maximum = self.maximum.max(error.abs());
        if !self.squared.is_finite() {
            return Err(Rejection::Degenerate("residual arithmetic overflow"));
        }
        Ok(())
    }
    fn rms(&self) -> f64 {
        (self.squared / self.count as f64).sqrt()
    }
    fn check(&self, population: &'static str, limits: Limits) -> Result<()> {
        let rms = self.rms();
        if rms > limits.maximum_normalized_rms || self.maximum > limits.maximum_normalized_error {
            return Err(Rejection::Residual {
                population,
                rms,
                rms_limit: limits.maximum_normalized_rms,
                maximum: self.maximum,
                maximum_limit: limits.maximum_normalized_error,
            });
        }
        Ok(())
    }
}

fn pair(
    reference: &DepthResource,
    candidate: &DepthResource,
    index: usize,
    confidence: Option<f32>,
) -> Option<(f64, f64)> {
    if let Some(threshold) = confidence {
        if reference.confidence()?[index] < threshold || candidate.confidence()?[index] < threshold
        {
            return None;
        }
    }
    Some((candidate.sample(index)?, reference.sample(index)?))
}

impl Alignment {
    /// Fit at most 4096 fixed, evenly spaced pairs. Validate every other eligible
    /// pair, including the last pixel; no holdout point enters the fit. Also check
    /// the fit population's residuals. Constant extra memory; linear bounded work.
    /// Cancellation is polled before work and at most 256 iterations apart.
    pub fn fit(
        reference: &DepthResource,
        candidate: &DepthResource,
        limits: Limits,
        mut canceled: impl FnMut() -> bool,
    ) -> Result<Self> {
        limits.validate()?;
        if canceled() {
            return Err(Rejection::Canceled);
        }
        let first = reference.descriptor();
        let second = candidate.descriptor();
        if !first.source.same_sample(&second.source)
            || first.geometry != second.geometry
            || first.size != second.size
        {
            return Err(Rejection::Incompatible(
                "source, PTS, geometry or scalar grid differs",
            ));
        }
        if !matches!(first.measure, DepthMeasure::Relative { .. })
            || first.measure != second.measure
        {
            return Err(Rejection::Incompatible(
                "same relative-depth near direction required",
            ));
        }
        if limits.minimum_confidence.is_some()
            && (reference.confidence().is_none() || candidate.confidence().is_none())
        {
            return Err(Rejection::Incompatible(
                "requested confidence plane is absent",
            ));
        }
        let pixels = candidate.values().len();
        let stride = pixels.div_ceil(MAX_FIT_PAIRS).max(2);
        let mut moments = Moments::default();
        for (slot, index) in (0..pixels).step_by(stride).enumerate() {
            if slot % 256 == 0 && canceled() {
                return Err(Rejection::Canceled);
            }
            if let Some((x, y)) = pair(reference, candidate, index, limits.minimum_confidence) {
                moments.add(x, y);
            }
        }
        if moments.count < limits.minimum_fit_pairs {
            return Err(Rejection::InsufficientPairs {
                population: "fit",
                actual: moments.count,
                minimum: limits.minimum_fit_pairs,
            });
        }
        let sigma = (moments.yy / moments.count as f64).sqrt();
        if !sigma.is_finite()
            || sigma < limits.minimum_reference_stddev
            || !moments.xx.is_finite()
            || moments.xx <= 0.
            || !moments.xy.is_finite()
        {
            return Err(Rejection::Degenerate(
                "constant or numerically unstable population",
            ));
        }
        let scale = moments.xy / moments.xx;
        let offset = moments.y - scale * moments.x;
        if !scale.is_finite() || !offset.is_finite() {
            return Err(Rejection::Degenerate("nonfinite affine coefficients"));
        }
        let [minimum, maximum] = limits.scale_bounds;
        if scale < minimum || scale > maximum {
            return Err(Rejection::Scale {
                actual: scale,
                minimum,
                maximum,
            });
        }
        let mut fit = Residuals::default();
        let mut validation = Residuals::default();
        for index in 0..pixels {
            if index % 256 == 0 && canceled() {
                return Err(Rejection::Canceled);
            }
            if let Some((x, y)) = pair(reference, candidate, index, limits.minimum_confidence) {
                let error = (scale * x + offset - y) / sigma;
                if !error.is_finite() {
                    return Err(Rejection::Degenerate("nonfinite aligned residual"));
                }
                if index % stride == 0 {
                    fit.add(error)?;
                } else {
                    validation.add(error)?;
                }
            }
        }
        if validation.count < limits.minimum_validation_pairs {
            return Err(Rejection::InsufficientPairs {
                population: "held-out",
                actual: validation.count,
                minimum: limits.minimum_validation_pairs,
            });
        }
        let eligible = fit.count + validation.count;
        if (eligible as f64) / (pixels as f64) < limits.minimum_pair_fraction {
            return Err(Rejection::Coverage {
                eligible,
                pixels,
                minimum: limits.minimum_pair_fraction,
            });
        }
        fit.check("fit", limits)?;
        validation.check("held-out", limits)?;
        Ok(Self {
            reference: reference.cache_identity(),
            candidate: candidate.cache_identity(),
            scale,
            offset,
            statistics: Statistics {
                pixels,
                fit_pairs: fit.count,
                validation_pairs: validation.count,
                skipped_pairs: pixels - fit.count - validation.count,
                reference_stddev: sigma,
                fit_normalized_rms: fit.rms(),
                fit_normalized_maximum: fit.maximum,
                validation_normalized_rms: validation.rms(),
                validation_normalized_maximum: validation.maximum,
            },
        })
    }

    /// Diagnostic coefficients only; use `bind` to preserve instance identity.
    pub fn coefficients(&self) -> [f64; 2] {
        [self.scale, self.offset]
    }
    pub fn statistics(&self) -> Statistics {
        self.statistics
    }

    pub fn bind<'a>(
        &'a self,
        reference: &DepthResource,
        candidate: &'a DepthResource,
    ) -> Result<Aligned<'a>> {
        if self.reference != reference.cache_identity()
            || self.candidate != candidate.cache_identity()
        {
            return Err(Rejection::Stale);
        }
        Ok(Aligned {
            proof: self,
            resource: candidate,
        })
    }
}

/// Borrows the original scalar resource; does not relabel its model or allocate
/// a second scalar plane. This is not a serialized cache/export policy.
pub struct Aligned<'a> {
    proof: &'a Alignment,
    resource: &'a DepthResource,
}
impl Aligned<'_> {
    /// Original metadata, validity and confidence; scalar values are still raw.
    pub fn resource(&self) -> &DepthResource {
        self.resource
    }
    pub fn sample(&self, index: usize) -> Result<Option<f64>> {
        let Some(raw) = self.resource.sample(index) else {
            return Ok(None);
        };
        let value = self.proof.scale * raw + self.proof.offset;
        if !value.is_finite() {
            return Err(Rejection::Degenerate("nonfinite aligned sample"));
        }
        Ok(Some(value))
    }
}
