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
    anchors: [DepthAnchor; 2],
    curve: CalibrationCurve,
    extrapolation: CalibrationExtrapolation,
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
            anchors,
            curve,
            extrapolation,
            pixels_per_metre,
            reference: None,
        })
    }
    pub fn validate(&self) -> Result<()> {
        let p = &self.0;
        ensure(
            p.pixels_per_metre.is_finite() && p.pixels_per_metre > 0.,
            "saved depth calibration requires explicit positive world pixels per metre",
        )?;
        p.input.validate(&p.interpretation.geometry)?;
        if let Some(reference) = &p.reference { reference.validate()?; }
        RelativeDepthCalibration::new(
            p.interpretation.descriptor(p.asset, p.pts),
            p.anchors,
            p.curve,
            p.extrapolation,
        )?;
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
        let p = &self.0;
        let actual = resource.descriptor();
        ensure(actual.source.asset == p.asset && actual.source.pts.same_instant(p.pts)
            && actual_input == p.input && Interpretation::from_descriptor(actual) == p.interpretation,
            &format!("saved_depth_calibration source={} phase=rebind: actual source pixels, time, model or interpretation differ", p.asset))?;
        // This new immutable live binding keeps the exact current source revision.
        RelativeDepthCalibration::new(actual.clone(), p.anchors, p.curve, p.extrapolation)
    }
}
