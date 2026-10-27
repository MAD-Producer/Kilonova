//! Physical velocity is the derivative of the model's value sampler, in units/s.
use motion_core::{Easing, Track, Tween};

/// Ordered composition-frame seeds. Shape seeds prevent an elastic period
/// from aliasing the flatness probes; they do not set the drawing resolution.
/// Hold's left limit and exact key time retain its discontinuous jump.
pub(super) fn knots<T: Tween>(
    track: &Track<T>, start: f64, end: f64, layer_offset: f64,
) -> impl Iterator<Item=f64> + '_ {
    std::iter::once(start).chain(track.keys.windows(2).flat_map(move |pair| {
        let a=f64::from(pair[0].frame)+layer_offset;
        let b=f64::from(pair[1].frame)+layer_offset;
        let hold=pair[0].ease==motion_core::Ease::Hold;
        let sections=if pair[0].curve.is_some_and(|curve| matches!(curve.shape,motion_core::CurveShape::Elastic {..})) {
            // Model validation bounds elastic frequency to eight periods.
            32
        } else if hold || (pair[0].curve.is_none() && pair[0].ease==motion_core::Ease::Linear) {
            1
        } else {4};
        (0..sections).map(move |i|a+(b-a)*f64::from(i)/f64::from(sections))
            .chain(hold.then(||b.next_down())).chain(std::iter::once(b))
    }).filter(move |frame|*frame>start && *frame<end)).chain(std::iter::once(end))
}
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
    fn adaptive_knots_keep_signed_local_keys_offsets_and_hold_boundaries() {
        let mut track=Track::constant(0f32);
        track.keys=vec![
            Keyframe {frame:-10,value:0.,ease:Ease::Hold,curve:None,spatial:None},
            Keyframe {frame:10,value:100.,ease:Ease::Linear,curve:None,spatial:None},
            Keyframe {frame:40,value:200.,ease:Ease::Linear,curve:None,spatial:None},
        ];
        track.validate_local().unwrap();
        let seeds=knots(&track,0.,100.,17.).collect::<Vec<_>>();
        assert_eq!(seeds.first(),Some(&0.));
        assert_eq!(seeds.last(),Some(&100.));
        assert!(seeds.windows(2).all(|p|p[0]<p[1]));
        assert!(seeds.contains(&27.));
        assert!(seeds.contains(&27f64.next_down()));
        assert!(seeds.contains(&57.));
        let points=super::super::graph_adaptive::sample(seeds,[8.,2.],0.35,
            |frame|track.sample(frame-17.).map(f64::from)).unwrap();
        let jump=points.windows(2).find(|pair|pair[0].1==0. && pair[1].1==100.).unwrap();
        assert!((jump[1].0-jump[0].0)*8.<0.36);
        assert_eq!(points.first(),Some(&(0.,0.)));
        assert_eq!(points.last(),Some(&(100.,200.)));
    }

    #[test]
    fn adaptive_real_model_values_and_physical_speeds_match_dense_screen_oracles() {
        use motion_core::{Curve,CurveShape,CurveSpace};
        let shapes=[None,
            Some(Curve {space:CurveSpace::Progress,shape:CurveShape::Quadratic {control:[0.2,1.5],start:0.,end:1.}}),
            Some(Curve {space:CurveSpace::Progress,shape:CurveShape::Cubic {control1:[0.001,-1.],control2:[0.999,2.],start:0.,end:1.}}),
            Some(Curve {space:CurveSpace::Progress,shape:CurveShape::Elastic {oscillations:8.,damping:0.5}}),
        ];
        for curve in shapes {
            let mut track=Track::constant(-50f32);
            track.keys=vec![
                Keyframe {frame:0,value:-50.,ease:if curve.is_none(){Ease::InOut}else{Ease::Linear},curve,spatial:None},
                Keyframe {frame:60,value:150.,ease:Ease::Linear,curve:None,spatial:None},
            ];
            track.validate_local().unwrap();
            for speed in [false,true] {
                let value=|frame|if speed {velocity(&track,frame,30.,f64::from)} else {f64::from(track.sample(frame).unwrap())};
                for [start,end] in [[0.,60.],[22.,23.],[0.,0.5],[59.5,60.]] {
                    let scale=[800./(end-start),40.];
                    let points=super::super::graph_adaptive::sample(knots(&track,start,end,0.),scale,0.35,
                        |frame|Ok::<_,motion_core::Error>(value(frame))).unwrap();
                    assert!(points.len()<65536);
                    for segment in points.windows(2) {
                        let delta=[(segment[1].0-segment[0].0)*scale[0],(segment[1].1-segment[0].1)*scale[1]];
                        let squared=delta[0]*delta[0]+delta[1]*delta[1];
                        for i in 0..=128 {
                            let frame=segment[0].0+(segment[1].0-segment[0].0)*f64::from(i)/128.;
                            let probe=[(frame-segment[0].0)*scale[0],(value(frame)-segment[0].1)*scale[1]];
                            let t=if squared>0. {((probe[0]*delta[0]+probe[1]*delta[1])/squared).clamp(0.,1.)} else {0.};
                            let error=(probe[0]-t*delta[0]).hypot(probe[1]-t*delta[1]);
                            assert!(error<=0.36,"curve={curve:?} speed={speed} range={start}..{end} frame={frame} error={error}");
                        }
                    }
                }
            }
        }
    }
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
