//! Context-dependent temporal endpoints, shared by sampling and the graph.
//! Weighted harmonic rates prevent scalar extrema from introducing overshoot.
//! Vectors use distance in their stored coordinate system; geometry is unchanged.
use super::*;
use crate::{CurveShape, CurveSpace};

pub(super) fn disabled(value: &bool) -> bool { !*value }

fn base<T>(key: &Keyframe<T>) -> Easing {
    Easing { ease: key.ease, curve: key.curve }
}

fn handles(easing: Easing) -> Result<([f64; 2], [f64; 2])> {
    Ok(match easing.curve {
        Some(Curve { space: CurveSpace::Progress, shape: CurveShape::Cubic {
            control1, control2, ..
        } }) => (control1, control2),
        Some(Curve { space: CurveSpace::Progress, shape: CurveShape::Quadratic {
            control, ..
        } }) => ([control[0]*2./3.,control[1]*2./3.],
            [1./3.+control[0]*2./3.,1./3.+control[1]*2./3.]),
        None => ([1./3., easing.sample(0.).velocity/3.],
            [2./3.,1.-easing.sample(1.).velocity/3.]),
        _ => return Err(crate::Error::Invalid(
            "automatic Bezier requires progress interpolation; convert velocity or elastic curves explicitly".into())),
    })
}

pub(super) fn validate_metadata<T: Tween>(key: &Keyframe<T>) -> Result<()> {
    ensure(key.value.timing_delta(key.value).is_some(),
        "automatic Bezier requires a numeric scalar or coupled 2D/3D/4D vector")?;
    ensure(key.spatial.is_none(), "automatic temporal Bezier is unavailable on manual spatial paths")?;
    if key.ease != Ease::Hold || key.curve.is_some() { handles(base(key))?; }
    Ok(())
}

pub(super) fn validate_key<T: Tween>(keys: &[Keyframe<T>], frame: i32) -> Result<()> {
    let index = keys.binary_search_by_key(&frame, |k| k.frame)
        .map_err(|_| crate::Error::Invalid("automatic Bezier keyframe missing".into()))?;
    ensure(keys.len() >= 2, "automatic Bezier requires at least two keys")?;
    validate_metadata(&keys[index])?;
    // Manual spatial handles have a separate physical arc-length metric. Do not
    // silently replace that metric with a chord or change the authored path.
    for interval in index.checked_sub(1).into_iter()
        .chain((index+1<keys.len()).then_some(index)) {
        ensure(keys[interval].spatial.is_none() && keys[interval+1].spatial.is_none(),
            "automatic temporal Bezier is unavailable on manual spatial paths")?;
        if keys[interval].ease != Ease::Hold || keys[interval].curve.is_some() {
            handles(base(&keys[interval]))?;
        }
    }
    Ok(())
}

fn interval_rate<T: Tween>(keys: &[Keyframe<T>], index: usize) -> Result<f64> {
    let a = &keys[index];
    let b = &keys[index+1];
    let span = f64::from(b.frame)-f64::from(a.frame);
    ensure(span > 0., "automatic Bezier keys must be strictly ordered")?;
    if a.ease == Ease::Hold && a.curve.is_none() { return Ok(0.); }
    let delta = a.value.timing_delta(b.value).ok_or_else(||
        crate::Error::Invalid("automatic Bezier has no numeric metric".into()))?;
    let rate = delta/span;
    ensure(rate.is_finite(), "automatic Bezier rate is non-finite")?;
    Ok(rate)
}

fn rate<T: Tween>(keys: &[Keyframe<T>], index: usize) -> Result<f64> {
    if index == 0 { return interval_rate(keys, 0); }
    if index+1 == keys.len() { return interval_rate(keys, index-1); }
    let left = interval_rate(keys,index-1)?;
    let right = interval_rate(keys,index)?;
    if left == 0. || right == 0. || left.is_sign_negative() != right.is_sign_negative() {
        return Ok(0.);
    }
    let h0 = f64::from(keys[index].frame)-f64::from(keys[index-1].frame);
    let h1 = f64::from(keys[index+1].frame)-f64::from(keys[index].frame);
    let w0 = 2.*h1+h0;
    let w1 = h1+2.*h0;
    Ok((w0+w1)/(w0/left+w1/right))
}

impl<T: Tween> Track<T> {
    /// Effective interval timing. Reading it allocates nothing and never changes
    /// authored metadata, including the opposite endpoint's manual handle.
    pub fn effective_easing(&self, index: usize) -> Result<Easing> {
        let a = self.keys.get(index).ok_or_else(||
            crate::Error::Invalid("timing interval start missing".into()))?;
        let b = self.keys.get(index+1).ok_or_else(||
            crate::Error::Invalid("timing interval end missing".into()))?;
        let easing = base(a);
        if (!a.auto_bezier && !b.auto_bezier)
            || (a.ease == Ease::Hold && a.curve.is_none()) { return Ok(easing); }
        // Validation is also required at this boundary: model clients can build
        // tracks directly without loading a validated Project first.
        if a.auto_bezier { validate_key(&self.keys,a.frame)?; }
        if b.auto_bezier { validate_key(&self.keys,b.frame)?; }
        let (mut c1,mut c2) = handles(easing)?;
        let delta = a.value.timing_delta(b.value).ok_or_else(||
            crate::Error::Invalid("automatic Bezier has no numeric metric".into()))?;
        let span = f64::from(b.frame)-f64::from(a.frame);
        ensure(span>0.,"automatic Bezier keys must be strictly ordered")?;
        if a.auto_bezier {
            c1 = [1./3.,if delta == 0. {0.} else {rate(&self.keys,index)?*span/delta/3.}];
        }
        if b.auto_bezier {
            c2 = [2./3.,if delta == 0. {1.} else {1.-rate(&self.keys,index+1)?*span/delta/3.}];
        }
        let effective = Easing { ease:Ease::Linear, curve:Some(Curve {
            space:CurveSpace::Progress, shape:CurveShape::Cubic {
                control1:c1,control2:c2,start:0.,end:1.
            }
        }) };
        effective.validate()?;
        Ok(effective)
    }

    /// One key owns both its incident automatic endpoints. Turning it off freezes
    /// their current effective curves, never resamples values or replaces a track.
    pub fn set_auto_bezier(&mut self, frame:i32, enabled:bool) -> Result<()> {
        ensure(self.axes.is_none(),"choose a separated keyframe axis explicitly")?;
        self.validate_local()?;
        let index = self.keys.binary_search_by_key(&frame,|k|k.frame)
            .map_err(|_|crate::Error::Invalid("automatic Bezier keyframe missing".into()))?;
        if self.keys[index].auto_bezier == enabled { return Ok(()); }
        if enabled {
            validate_key(&self.keys,frame)?;
            // Hold on the selected outgoing key is an explicitly replaced type.
            // A preceding held interval remains a deliberate discontinuity.
            if self.keys[index].ease == Ease::Hold && self.keys[index].curve.is_none() {
                self.keys[index].ease = Ease::Linear;
            }
            self.keys[index].auto_bezier=true;
        } else {
            let incoming=index.checked_sub(1).map(|i|self.effective_easing(i).map(|e|(i,e))).transpose()?;
            let outgoing=(index+1<self.keys.len()).then(||self.effective_easing(index)
                .map(|e|(index,e))).transpose()?;
            for (interval,easing) in incoming.into_iter().chain(outgoing) {
                self.keys[interval].ease=easing.ease;
                self.keys[interval].curve=easing.curve;
            }
            self.keys[index].auto_bezier=false;
        }
        Ok(())
    }
}
