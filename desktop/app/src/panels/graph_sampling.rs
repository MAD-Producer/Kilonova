//! Physical velocity is the derivative of the model's value sampler, in units/s.
use motion_core::{Easing, Track, Tween};
pub(super) fn velocity<T: Tween>(
    track: &Track<T>,
    frame: f64,
    fps: f64,
    component: impl Fn(T) -> f64,
) -> f64 {
    if track.keys.len() < 2
        || frame < f64::from(track.keys[0].frame)
        || frame > f64::from(track.keys.last().unwrap().frame)
    {
        return 0.;
    }
    let upper = track
        .keys
        .partition_point(|key| f64::from(key.frame) <= frame)
        .min(track.keys.len() - 1)
        .max(1);
    let a = &track.keys[upper - 1];
    let b = &track.keys[upper];
    let span = f64::from(b.frame) - f64::from(a.frame);
    if span <= 0. {
        return 0.;
    }
    let sample = Easing {
        ease: a.ease,
        curve: a.curve,
    }
    .sample(((frame - f64::from(a.frame)) / span).clamp(0., 1.));
    let av = component(a.value);
    let bv = component(b.value);
    let outgoing = a.spatial.as_ref().and_then(|s| s.outgoing);
    let incoming = b.spatial.as_ref().and_then(|s| s.incoming);
    let tangent = if outgoing.is_none() && incoming.is_none() {
        bv - av
    } else {
        let c1 = outgoing.map_or(av + (bv - av) / 3., |v| av + component(v));
        let c2 = incoming.map_or(av + (bv - av) * 2. / 3., |v| bv + component(v));
        let p = sample.progress;
        3. * (1. - p).powi(2) * (c1 - av)
            + 6. * (1. - p) * p * (c2 - c1)
            + 3. * p.powi(2) * (bv - c2)
    };
    tangent * sample.velocity * fps / span
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_core::{Ease, Keyframe, SpatialTangents};
    #[test]
    fn physical_speed_and_hold_do_not_use_a_discontinuous_finite_difference() {
        let mut track = Track::constant(0.);
        track.keys = vec![
            Keyframe {
                frame: 0,
                value: 0.,
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            },
            Keyframe {
                frame: 30,
                value: 600.,
                ease: Ease::Linear,
                curve: None,
                spatial: None,
            },
        ];
        assert_eq!(velocity(&track, 15., 30., |v| v as f64), 600.);
        track.keys[0].ease = Ease::InOut;
        assert_eq!(velocity(&track, 15., 30., |v| v as f64), 900.);
        track.keys[0].ease = Ease::Hold;
        assert_eq!(velocity(&track, 29.999, 30., |v| v as f64), 0.);
        assert_eq!(velocity(&track, 31., 30., |v| v as f64), 0.);
    }
    #[test]
    fn vector_spatial_path_derivative_matches_the_actual_sampler() {
        let mut track = Track::constant([0., 0., 0.]);
        track.keys = vec![
            Keyframe {
                frame: 0,
                value: [0., 0., 0.],
                ease: Ease::InOut,
                curve: None,
                spatial: Some(SpatialTangents {
                    incoming: None,
                    outgoing: Some([200., 300., 0.]),
                }),
            },
            Keyframe {
                frame: 30,
                value: [600., 300., 0.],
                ease: Ease::Linear,
                curve: None,
                spatial: Some(SpatialTangents {
                    incoming: Some([-200., -100., 0.]),
                    outgoing: None,
                }),
            },
        ];
        let derivative = (track.sample(15.02).unwrap()[1] - track.sample(14.98).unwrap()[1]) as f64 / 0.04 * 30.;
        assert!((velocity(&track, 15., 30., |v| v[1] as f64) - derivative).abs() < 0.2);
    }
}
