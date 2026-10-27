//! Explicit, sample-bound two-anchor calibration. Predictions alone are not metres.
use super::{DepthDescriptor, DepthMeasure, DepthResource, NearDirection};
use crate::{ensure, Result};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationCurve {
    /// Camera Z is affine in decoded relative value.
    CameraZ,
    /// Inverse camera Z is affine in decoded relative value.
    InverseCameraZ,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationExtrapolation {
    Reject,
    /// Extrapolation is explicit; nonpositive/nonfinite results still fail.
    AllowPositive,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DepthAnchor {
    /// Decoded value, after the resource's scale and offset.
    pub value: f64,
    /// A supplied calibration observation, not inferred metric accuracy.
    pub camera_z_meters: f64,
}

/// Immutable validated parameters, bound to source/revision/PTS, model processing
/// identity, decoded units and crop/rotation/resize geometry. Deserialization is
/// deliberately absent: callers must validate author-supplied values with `new`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelativeDepthCalibration {
    descriptor: DepthDescriptor,
    anchors: [DepthAnchor; 2],
    curve: CalibrationCurve,
    extrapolation: CalibrationExtrapolation,
}

/// A borrowed valid binding, checked once before scanning pixels. Neither source
/// nor calibration can change underneath it; no per-pixel metadata copies occur.
pub struct CalibratedDepth<'a> {
    resource: &'a DepthResource,
    calibration: &'a RelativeDepthCalibration,
}

impl RelativeDepthCalibration {
    pub fn new(
        descriptor: DepthDescriptor,
        anchors: [DepthAnchor; 2],
        curve: CalibrationCurve,
        extrapolation: CalibrationExtrapolation,
    ) -> Result<Self> {
        descriptor.validate()?;
        let DepthMeasure::Relative { near } = descriptor.measure else {
            return Err(crate::Error::Invalid(
                "relative calibration requires relative depth".into(),
            ));
        };
        ensure(
            anchors.iter().all(|a| {
                a.value.is_finite() && a.camera_z_meters.is_finite() && a.camera_z_meters > 0.
            }),
            "invalid depth calibration anchor",
        )?;
        let dv = anchors[1].value - anchors[0].value;
        let dz = anchors[1].camera_z_meters - anchors[0].camera_z_meters;
        ensure(
            dv.is_finite() && dv != 0. && dz.is_finite() && dz != 0.,
            "depth calibration anchors need distinct finite values and distances",
        )?;
        let increasing_distance = dv.is_sign_positive() == dz.is_sign_positive();
        ensure(
            increasing_distance == matches!(near, NearDirection::Decreasing),
            "depth calibration contradicts declared near direction",
        )?;
        if let Some([low, high]) = descriptor.valid_range {
            ensure(
                anchors.iter().all(|a| (low..=high).contains(&a.value)),
                "depth calibration anchor outside declared decoded range",
            )?;
        }
        if curve == CalibrationCurve::InverseCameraZ {
            let inverse = anchors.map(|a| a.camera_z_meters.recip());
            ensure(
                inverse.iter().all(|d| d.is_finite() && *d > 0.) && inverse[0] != inverse[1],
                "inverse calibration anchors are not representable",
            )?;
        }
        Ok(Self {
            descriptor,
            anchors,
            curve,
            extrapolation,
        })
    }

    pub fn descriptor(&self) -> &DepthDescriptor {
        &self.descriptor
    }
    pub fn anchors(&self) -> &[DepthAnchor; 2] {
        &self.anchors
    }
    pub fn curve(&self) -> CalibrationCurve {
        self.curve
    }
    pub fn extrapolation(&self) -> CalibrationExtrapolation {
        self.extrapolation
    }

    pub fn bind<'a>(&'a self, resource: &'a DepthResource) -> Result<CalibratedDepth<'a>> {
        let actual = resource.descriptor();
        let expected = &self.descriptor;
        ensure(
            actual.source.same_sample(&expected.source)
                && actual.size == expected.size
                && actual.geometry == expected.geometry
                && actual.model == expected.model
                && actual.measure == expected.measure
                && actual.scale == expected.scale
                && actual.offset == expected.offset
                && actual.valid_range == expected.valid_range,
            "depth calibration does not match source, time, model or geometry",
        )?;
        Ok(CalibratedDepth {
            resource,
            calibration: self,
        })
    }

    fn distance(&self, value: f64) -> Result<f64> {
        let [a, b] = self.anchors;
        let t = (value - a.value) / (b.value - a.value);
        ensure(t.is_finite(), "depth calibration interpolation overflow")?;
        if self.extrapolation == CalibrationExtrapolation::Reject {
            ensure(
                (0. ..=1.).contains(&t),
                "depth calibration requires explicit extrapolation",
            )?;
        }
        // Exact anchor observations are not perturbed by interpolation rounding.
        if t == 0. {
            return Ok(a.camera_z_meters);
        }
        if t == 1. {
            return Ok(b.camera_z_meters);
        }
        let (a, b) = match self.curve {
            CalibrationCurve::CameraZ => (a.camera_z_meters, b.camera_z_meters),
            CalibrationCurve::InverseCameraZ => {
                (a.camera_z_meters.recip(), b.camera_z_meters.recip())
            }
        };
        let value = a * (1. - t) + b * t;
        ensure(
            value.is_finite() && value > 0.,
            "nonpositive or nonfinite calibrated depth",
        )?;
        let distance = if self.curve == CalibrationCurve::InverseCameraZ {
            value.recip()
        } else {
            value
        };
        ensure(
            distance.is_finite() && distance > 0.,
            "calibrated camera Z overflow",
        )?;
        Ok(distance)
    }
}

impl CalibratedDepth<'_> {
    pub fn resource(&self) -> &DepthResource {
        self.resource
    }
    pub fn calibration(&self) -> &RelativeDepthCalibration {
        self.calibration
    }

    /// Missing/invalid samples remain normal misses. Confidence and validity are
    /// retained on the source, not invented from the supplied anchor distances.
    pub fn camera_z_meters(&self, index: usize) -> Result<Option<f64>> {
        self.resource
            .sample(index)
            .map(|v| self.calibration.distance(v))
            .transpose()
    }
}
