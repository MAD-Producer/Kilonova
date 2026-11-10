//! Authoring identity is independent of a session's live editing revision.
use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Interpretation {
    size: [u32; 2],
    geometry: DepthGeometry,
    model: DepthModel,
    measure: DepthMeasure,
    scale: f64,
    offset: f64,
    valid_range: Option<[f64; 2]>,
}
impl Interpretation {
    fn from_descriptor(d: &DepthDescriptor) -> Self {
        Self {
            size: d.size,
            geometry: d.geometry.clone(),
            model: d.model.clone(),
            measure: d.measure,
            scale: d.scale,
            offset: d.offset,
            valid_range: d.valid_range,
        }
    }
    fn descriptor(&self, asset: u64, pts: DepthTime) -> DepthDescriptor {
        DepthDescriptor {
            size: self.size,
            source: DepthSource {
                asset,
                revision: 0,
                pts,
            },
            geometry: self.geometry.clone(),
            model: self.model.clone(),
            measure: self.measure,
            scale: self.scale,
            offset: self.offset,
            valid_range: self.valid_range,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parameters {
    asset: u64,
    pts: DepthTime,
    input: PreparedDepthInput,
    interpretation: Interpretation,
    // Relative-only observations. Physical descriptors already supply units;
    // they must never acquire fabricated anchors or an extra affine calibration.
    #[serde(skip_serializing_if = "Option::is_none")]
    anchors: Option<[DepthAnchor; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    curve: Option<CalibrationCurve>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extrapolation: Option<CalibrationExtrapolation>,
    pixels_per_metre: f32,
    /// None denotes algebra-only calibration, without a placement for rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reference: Option<DepthReference>,
}

/// Bounded saved authoring data, never an installed inference result. Rebinding
/// needs the actual prepared RGB identity and a separately admitted live result.
/// The original RelativeDepthCalibration revision guard remains unchanged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Parameters")]
pub struct SavedDepthCalibration(Parameters);

impl TryFrom<Parameters> for SavedDepthCalibration {
    type Error = crate::Error;
    fn try_from(parameters: Parameters) -> Result<Self> {
        let result = Self(parameters);
        result.validate()?;
        Ok(result)
    }
}
impl SavedDepthCalibration {
    pub fn new(
        resource: &DepthResource,
        input: PreparedDepthInput,
        anchors: [DepthAnchor; 2],
        curve: CalibrationCurve,
        extrapolation: CalibrationExtrapolation,
        pixels_per_metre: f32,
    ) -> Result<Self> {
        let descriptor = resource.descriptor();
        Self::try_from(Parameters {
            asset: descriptor.source.asset,
            pts: descriptor.source.pts,
            input,
            interpretation: Interpretation::from_descriptor(descriptor),
            anchors: Some(anchors),
            curve: Some(curve),
            extrapolation: Some(extrapolation),
            pixels_per_metre,
            reference: None,
        })
    }
    /// Preserve declared metric/inverse-metric camera-Z units as authoring data.
    /// This establishes identity and placement, not learned-model accuracy.
    /// Ray distance needs an explicit camera-ray conversion before this API.
    pub fn physical(
        resource: &DepthResource,
        input: PreparedDepthInput,
        pixels_per_metre: f32,
    ) -> Result<Self> {
        let descriptor = resource.descriptor();
        ensure(!matches!(descriptor.measure, DepthMeasure::Relative { .. }),
            "physical depth placement requires declared physical units")?;
        Self::try_from(Parameters {
            asset: descriptor.source.asset,
            pts: descriptor.source.pts,
            input,
            interpretation: Interpretation::from_descriptor(descriptor),
            anchors: None,
            curve: None,
            extrapolation: None,
            pixels_per_metre,
            reference: None,
        })
    }
    pub fn is_physical(&self) -> bool {
        !matches!(self.0.interpretation.measure, DepthMeasure::Relative { .. })
    }
    pub fn validate(&self) -> Result<()> {
        let p = &self.0;
        ensure(
            p.pixels_per_metre.is_finite() && p.pixels_per_metre > 0.,
            "saved depth calibration requires explicit positive world pixels per metre",
        )?;
        p.input.validate(&p.interpretation.geometry)?;
        if let Some(reference) = &p.reference { reference.validate()?; }
        let descriptor = p.interpretation.descriptor(p.asset, p.pts);
        descriptor.validate()?;
        if self.is_physical() {
            ensure(matches!(descriptor.measure,
                DepthMeasure::Metric { axis: DepthAxis::CameraZ, .. }
                | DepthMeasure::InverseMetric { axis: DepthAxis::CameraZ, .. }),
                "physical depth placement requires camera-Z units")?;
            ensure(p.anchors.is_none() && p.curve.is_none() && p.extrapolation.is_none(),
                "physical depth placement cannot contain relative calibration")?;
        } else {
            self.rebind_descriptor(&descriptor, p.input)?;
        }
        Ok(())
    }
    pub fn asset(&self) -> u64 {
        self.0.asset
    }
    pub fn source_time(&self) -> DepthTime {
        self.0.pts
    }
    pub fn source_size(&self) -> [u32; 2] {
        self.0.interpretation.geometry.source_size
    }
    pub fn input(&self) -> PreparedDepthInput {
        self.0.input
    }
    pub fn geometry(&self) -> &DepthGeometry { &self.0.interpretation.geometry }
    pub fn model(&self) -> &DepthModel { &self.0.interpretation.model }
    pub fn pixels_per_metre(&self) -> f32 {
        self.0.pixels_per_metre
    }
    pub fn reference(&self) -> Option<&DepthReference> { self.0.reference.as_ref() }
    pub fn with_reference(mut self, reference: DepthReference) -> Result<Self> {
        reference.validate()?;
        self.0.reference = Some(reference);
        Ok(self)
    }
    /// Clipboard remapping changes the managed reference, not the expected pixels.
    pub fn remap_asset(&mut self, asset: u64) {
        self.0.asset = asset;
    }

    pub fn rebind(
        &self,
        resource: &DepthResource,
        actual_input: PreparedDepthInput,
    ) -> Result<RelativeDepthCalibration> {
        self.rebind_descriptor(resource.descriptor(), actual_input)
    }

    pub fn validate_binding(&self, resource: &DepthResource,
        actual_input: PreparedDepthInput) -> Result<()> {
        self.validate_binding_descriptor(resource.descriptor(), actual_input)
    }

    /// Validate the exact saved interpretation without inventing a relative
    /// calibration for metric data. Immutable file planes are checked on read.
    pub fn validate_binding_descriptor(&self, actual: &DepthDescriptor,
        actual_input: PreparedDepthInput) -> Result<()> {
        let p = &self.0;
        ensure(actual.source.asset == p.asset && actual.source.pts.same_instant(p.pts)
            && actual_input == p.input && Interpretation::from_descriptor(actual) == p.interpretation,
            &format!("saved_depth_calibration source={} phase=rebind: actual source pixels, time, model or interpretation differ", p.asset))?;
        actual.validate().map(|_| ())
    }

    /// Metadata-only binding for a validated immutable file-backed resource.
    /// Plane integrity is independently checked when its values are read.
    pub fn rebind_descriptor(&self, actual: &DepthDescriptor,
        actual_input: PreparedDepthInput) -> Result<RelativeDepthCalibration> {
        self.validate_binding_descriptor(actual, actual_input)?;
        let p = &self.0;
        // This new immutable live binding keeps the exact current source revision.
        RelativeDepthCalibration::new(actual.clone(),
            p.anchors.ok_or_else(|| Error::Invalid("relative depth calibration anchors are missing".into()))?,
            p.curve.ok_or_else(|| Error::Invalid("relative depth calibration curve is missing".into()))?,
            p.extrapolation.ok_or_else(|| Error::Invalid("relative depth calibration extrapolation is missing".into()))?)
    }
}
