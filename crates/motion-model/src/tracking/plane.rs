//! Independent planar observations drive an explicit corner-pin instance.
//! Interpolation operates on projected corners, never scale-ambiguous H coefficients.
use super::{
    asset::{Kind, Observation, Rate, Visibility},
    plate::{Extent, PlateTransform, Rotation},
    point::{Interpolation, Missing},
    time::{Location, Mapping, Speed, Time, Timeline},
};
use crate::{ensure, Error, Layer, Project, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneBinding {
    pub asset: u64,
    pub track: String,
    pub effect: u64,
    pub anchor_local_frame: i32,
    pub source_anchor_pts: i64,
    pub speed_numerator: i32,
    pub speed_denominator: u32,
    /// Clockwise TL, TR, BR, BL in the original reference-image pixels.
    pub reference_quad: [[f64; 2]; 4],
    pub crop: [f64; 4],
    pub rotation: Rotation,
    pub interpolation: Interpolation,
    pub max_gap_ticks: u64,
    pub minimum_confidence: f64,
    pub missing: Missing,
}
fn invalid(cause: impl std::fmt::Display) -> Error {
    Error::Invalid(cause.to_string())
}
fn convex(quad: [[f64; 2]; 4]) -> Result<()> {
    let mut sign = 0.;
    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let c = quad[(i + 2) % 4];
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        ensure(
            cross.is_finite() && cross.abs() > 1e-4,
            "tracking plane quad is degenerate",
        )?;
        if sign == 0. {
            sign = cross.signum();
        }
        ensure(
            cross.signum() == sign,
            "tracking plane quad is crossed or nonconvex",
        )?;
    }
    Ok(())
}
/// Homogeneous scale is arbitrary, including a negative sign. A projective pole
/// crossing the quad is a failure, not a transparent successful render.
pub fn project_quad(matrix: [[f64; 3]; 3], quad: [[f64; 2]; 4]) -> Result<[[f64; 2]; 4]> {
    let scale = matrix.iter().flatten().fold(0f64, |a, b| a.max(b.abs()));
    ensure(
        scale.is_finite() && scale > 0.,
        "invalid tracking homography scale",
    )?;
    let m = matrix.map(|row| row.map(|v| v / scale));
    let mut result = [[0.; 2]; 4];
    let mut sign = 0.;
    for (i, [x, y]) in quad.into_iter().enumerate() {
        let d = m[2][0] * x + m[2][1] * y + m[2][2];
        ensure(
            d.is_finite() && d.abs() > 1e-12,
            "tracking plane corner meets projective pole",
        )?;
        if sign == 0. {
            sign = d.signum();
        }
        ensure(
            d.signum() == sign,
            "tracking plane pole crosses reference quad",
        )?;
        result[i] = [
            (m[0][0] * x + m[0][1] * y + m[0][2]) / d,
            (m[1][0] * x + m[1][1] * y + m[1][2]) / d,
        ];
        ensure(
            result[i].iter().all(|v| v.is_finite()),
            "tracking plane projection overflow",
        )?;
    }
    convex(result)?;
    Ok(result)
}
impl PlaneBinding {
    pub fn validate(&self, project: &Project, layer: &Layer) -> Result<()> {
        ensure(
            self.asset != 0
                && self.effect != 0
                && !self.track.trim().is_empty()
                && self.track.len() <= 128
                && !self.track.chars().any(char::is_control),
            "invalid plane binding identity",
        )?;
        ensure(
            self.anchor_local_frame.unsigned_abs() <= crate::MAX_FRAMES,
            "plane anchor frame out of range",
        )?;
        ensure(
            self.minimum_confidence.is_finite() && (0. ..=1.).contains(&self.minimum_confidence),
            "invalid plane confidence threshold",
        )?;
        Speed::new(self.speed_numerator, self.speed_denominator).map_err(invalid)?;
        ensure(
            !layer.three_d
                && !matches!(
                    layer.content,
                    crate::Content::Null
                        | crate::Content::Audio { .. }
                        | crate::Content::Camera { .. }
                        | crate::Content::Adjustment
                        | crate::Content::Model { .. }
                ),
            "planar corner binding requires a flat visual layer",
        )?;
        let asset = project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| invalid("plane tracking asset missing"))?;
        asset.validate_source_digest()?;
        let track = asset
            .data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| invalid("plane track missing"))?;
        ensure(
            track.kind == Kind::Plane,
            "plane binding requires a plane track",
        )?;
        ensure(
            self.reference_quad
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1e7),
            "plane reference pixels out of range",
        )?;
        convex(self.reference_quad)?;
        let effect = layer
            .effects
            .first()
            .filter(|e| {
                e.id == self.effect
                    && e.effect == "corner_pin"
                    && e.plugin == motion_effects::builtin::PLUGIN_ID
            })
            .ok_or_else(|| invalid("plane binding requires the first corner_pin effect"))?;
        for id in ["tl", "tr", "br", "bl"] {
            let p = effect
                .params
                .get(id)
                .ok_or_else(|| invalid("corner_pin parameter missing"))?;
            ensure(
                p.kind == motion_effects::ParamKind::Vec2 && p.implemented,
                "corner_pin requires implemented vec2 corners",
            )?;
        }
        self.plate(asset.data.source_extent, layer)?;
        Ok(())
    }
    fn plate(&self, extent: Extent, layer: &Layer) -> Result<PlateTransform> {
        ensure(
            layer
                .size
                .iter()
                .all(|v| v.is_finite() && v.fract() == 0. && *v >= 1. && *v <= 32768.),
            "plane output requires integral source dimensions",
        )?;
        let output = Extent::new(layer.size[0] as u32, layer.size[1] as u32).map_err(invalid)?;
        PlateTransform::new(extent, self.crop, self.rotation, output).map_err(invalid)
    }
    /// Returns displacements from the mapped reference, added to authored corners.
    pub fn displacement(
        &self,
        project: &Project,
        layer: &Layer,
        frame: f64,
    ) -> Result<[[f32; 2]; 4]> {
        let asset = project
            .tracking_assets
            .iter()
            .find(|a| a.id == self.asset)
            .ok_or_else(|| invalid("plane tracking asset missing"))?;
        let data = &asset.data;
        let track = data
            .tracks
            .iter()
            .find(|t| t.id == self.track)
            .ok_or_else(|| invalid("plane track missing"))?;
        let [numerator, denominator] = project.fps.ratio();
        let rate = Rate::new(numerator, denominator).map_err(invalid)?;
        let mapping = Mapping::new(
            Time::frame(i64::from(self.anchor_local_frame), rate),
            self.source_anchor_pts,
            Speed::new(self.speed_numerator, self.speed_denominator).map_err(invalid)?,
        );
        let location = Timeline::from_shared(data, mapping)
            .locate(Time::fractional_frame(layer.local_frame(frame), rate).map_err(invalid)?)
            .map_err(invalid)?;
        let sample = |index| -> Result<[[f64; 2]; 4]> {
            let i = track
                .samples
                .binary_search_by_key(&index, |s| s.source_index)
                .map_err(|_| invalid(format!("plane observation absent source_index={index}")))?;
            let s = &track.samples[i];
            if s.visibility != Visibility::Visible {
                return Err(invalid(format!(
                    "plane not visible source_index={index} state={:?}",
                    s.visibility
                )));
            }
            if s.confidence < self.minimum_confidence {
                return Err(invalid(format!(
                    "plane below confidence threshold source_index={index}"
                )));
            }
            let Some(Observation::Plane(h)) = s.observation else {
                return Err(invalid("plane observation kind mismatch"));
            };
            project_quad(h, self.reference_quad)
        };
        // Missing policy covers unavailable observations, not geometric poles or
        // nonconvex output: those must not become a seemingly successful sample.
        let available = |index| -> Result<()> {
            let i = track
                .samples
                .binary_search_by_key(&index, |s| s.source_index)
                .map_err(|_| invalid(format!("plane observation absent source_index={index}")))?;
            let s = &track.samples[i];
            if s.visibility != Visibility::Visible {
                return Err(invalid(format!(
                    "plane observation unavailable source_index={index} visibility={:?}",
                    s.visibility
                )));
            }
            if s.confidence < self.minimum_confidence {
                return Err(invalid(format!(
                    "plane observation unavailable source_index={index} confidence={} threshold={}",
                    s.confidence, self.minimum_confidence
                )));
            }
            Ok(())
        };
        let usable = match location {
            Location::At(i) => available(i),
            Location::Between { before, after, .. } => {
                let gap = i128::from(data.source_pts[after as usize])
                    - i128::from(data.source_pts[before as usize]);
                if gap > i128::from(self.max_gap_ticks) {
                    Err(invalid(format!("plane interpolation gap exceeds policy before={before} after={after} gap_ticks={gap} max_gap_ticks={}",self.max_gap_ticks)))
                } else {
                    available(before).and_then(|_| {
                        if self.interpolation == Interpolation::Hold {
                            Ok(())
                        } else {
                            available(after)
                        }
                    })
                }
            }
            _ => Err(invalid(format!(
                "plane observation outside source PTS range location={location:?}"
            ))),
        };
        if let Err(cause) = usable {
            if self.missing == Missing::AuthoredOnly {
                return Ok([[0.; 2]; 4]);
            }
            return Err(cause);
        }
        let corners = match location {
            Location::At(i) => sample(i)?,
            Location::Between {
                before,
                after,
                numerator,
                denominator,
            } => {
                let a = sample(before)?;
                if self.interpolation == Interpolation::Hold {
                    a
                } else {
                    let b = sample(after)?;
                    let t = numerator as f64 / denominator as f64;
                    std::array::from_fn(|i| {
                        std::array::from_fn(|j| a[i][j] + (b[i][j] - a[i][j]) * t)
                    })
                }
            }
            _ => return Err(invalid("invalid usable plane bracket")),
        };
        convex(corners)?;
        let plate = self.plate(data.source_extent, layer)?;
        let mut delta = [[0.; 2]; 4];
        for i in 0..4 {
            let a = plate.apply(corners[i]).map_err(invalid)?;
            let b = plate.apply(self.reference_quad[i]).map_err(invalid)?;
            delta[i] = [(a[0] - b[0]) as f32, (a[1] - b[1]) as f32];
            ensure(
                delta[i].iter().all(|v| v.is_finite()),
                "plane displacement overflow",
            )?;
        }
        Ok(delta)
    }
}
impl Layer {
    pub fn apply_tracking_plane(
        &self,
        project: &Project,
        frame: f64,
        sampled: &mut crate::SampledEffect,
    ) -> Result<()> {
        let Some(binding) = &self.tracking_plane else {
            return Ok(());
        };
        // Scene samples effect records before culling layers. A clip that has
        // ended must not demand observations outside its original source range.
        // Visibility is deliberately not tested: hidden image/matte sources
        // can still contribute to another visible layer.
        let interval = self.interval(project.frames);
        if frame < interval.in_frame || frame >= interval.out_frame {
            return Ok(());
        }
        if sampled.instance != binding.effect || !sampled.enabled {
            return Ok(());
        }
        (|| {
            let delta = binding.displacement(project, self, frame)?;
            for (corner, id) in ["tl", "tr", "br", "bl"].into_iter().enumerate() {
                let index = sampled
                    .param_ids
                    .iter()
                    .position(|p| p == id)
                    .ok_or_else(|| invalid("sampled corner missing"))?;
                let param = self
                    .effects
                    .iter()
                    .find(|e| e.id == binding.effect)
                    .and_then(|e| e.params.get(id))
                    .ok_or_else(|| invalid("authored corner missing"))?;
                for axis in 0..2 {
                    sampled.values[index][axis] += delta[corner][axis];
                }
                ensure(
                    param
                        .kind
                        .valid_value(&sampled.values[index], param.min, param.max),
                    "tracked corner exceeds effect parameter range",
                )?;
            }
            let mut quad = [[0.; 2]; 4];
            for (i, id) in ["tl", "tr", "br", "bl"].into_iter().enumerate() {
                let index = sampled
                    .param_ids
                    .iter()
                    .position(|p| p == id)
                    .ok_or_else(|| invalid("sampled corner missing"))?;
                quad[i] = [
                    f64::from(sampled.values[index][0]),
                    f64::from(sampled.values[index][1]),
                ];
            }
            convex(quad)
        })()
        .map_err(|source| Error::TrackingPlane {
            object: self.id,
            asset: binding.asset,
            track: binding.track.clone(),
            frame,
            source: Box::new(source),
        })
    }
}
