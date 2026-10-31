use motion_model::{Ease, SpatialAnimation, SpatialTangents, Track};

fn original() -> Track<[f32; 3]> {
    let mut track = Track::constant([0.; 3]);
    for (time, x, y) in [(0, 0., 0.), (4, 1., 2.), (10, 7., 5.), (20, 15., -3.)] {
        track.upsert(time, [x, y, 0.], Ease::Linear).unwrap();
    }
    track
}

#[test]
fn manual_velocity_matches_value_derivative_with_easing_spatial_handles_and_hold() {
    let mut track = original();
    track.set_ease(0, Ease::InOut).unwrap();
    track.set_ease(10, Ease::Hold).unwrap();
    track
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([4., 8., 0.]),
            }),
        )
        .unwrap();
    track
        .set_spatial(
            4,
            Some(SpatialTangents {
                incoming: Some([-2., 3., 0.]),
                outgoing: None,
            }),
        )
        .unwrap();
    let animation = SpatialAnimation::from_track(&track).unwrap();
    for t in [0.3, 1., 2.5, 3.9, 4.1, 7., 9.9, 10.1, 15., 19.99] {
        let rate = animation.velocity(t).unwrap();
        let h = 0.002;
        let a = animation.sample_value(t - h).unwrap();
        let b = animation.sample_value(t + h).unwrap();
        for i in 0..3 {
            let derivative = f64::from(b[i] - a[i]) / (2. * h);
            assert!(
                (derivative - rate[i]).abs() < 0.004,
                "t={t} i={i} derivative={derivative} rate={rate:?}"
            );
        }
    }
    assert_eq!(
        animation.velocity(19.999).unwrap(),
        [0.; 3],
        "Hold has no finite-difference spike"
    );
    assert_eq!(animation.velocity(-1.).unwrap(), [0.; 3]);
    assert_eq!(animation.velocity(21.).unwrap(), [0.; 3]);
    assert!(animation.velocity(f64::NAN).is_err());
}

#[test]
fn roving_velocity_follows_actual_arc_sampler_and_reopens_identically() {
    let mut track = original();
    track
        .set_spatial(
            0,
            Some(SpatialTangents {
                incoming: None,
                outgoing: Some([5., 10., 0.]),
            }),
        )
        .unwrap();
    track
        .set_spatial(
            4,
            Some(SpatialTangents {
                incoming: Some([-3., 4., 0.]),
                outgoing: None,
            }),
        )
        .unwrap();
    track.enable_roving(&[4]).unwrap();
    let animation = track.spatial_animation.as_ref().unwrap();
    let reopened: Track<[f32; 3]> =
        serde_json::from_slice(&serde_json::to_vec(&track).unwrap()).unwrap();
    for tick in 1..999 {
        let t = f64::from(tick) / 100.;
        let rate = animation.velocity(t).unwrap();
        assert_eq!(
            rate,
            reopened
                .spatial_animation
                .as_ref()
                .unwrap()
                .velocity(t)
                .unwrap()
        );
        // The prepared inverse metric is piecewise linear. A wide secant
        // can cross a lookup-node slope change and is not its local derivative.
        // Probe the f64 arc sampler narrowly, avoiding f32 value quantization.
        let h = 0.000001;
        let a = animation.sample(t - h).unwrap().position;
        let b = animation.sample(t + h).unwrap().position;
        for i in 0..3 {
            let derivative = (b[i] - a[i]) / (2. * h);
            assert!(
                (derivative - rate[i]).abs() < 0.0001,
                "t={t} derivative={derivative} rate={rate:?}"
            );
        }
    }
    let mut linear = original();
    linear.enable_roving(&[4]).unwrap();
    let animation = linear.spatial_animation.as_ref().unwrap();
    let speed = animation
        .velocity(1.)
        .unwrap()
        .iter()
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt();
    let other = animation
        .velocity(7.)
        .unwrap()
        .iter()
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt();
    assert!((speed - other).abs() < 1e-12);
    assert!(
        (speed - (5f64.sqrt() / 4.)).abs() > 0.1,
        "old authored timing is a failing control"
    );
}

#[test]
fn coincident_stationary_keys_do_not_create_nan_speed_or_key_identity_loss() {
    let mut track = Track::constant([0.; 3]);
    for (time, x) in [(0, 0.), (4, 1.), (10, 1.), (20, 8.)] {
        track.upsert(time, [x, 0., 0.], Ease::Linear).unwrap();
    }
    track.enable_roving(&[4, 10]).unwrap();
    let animation = track.spatial_animation.as_ref().unwrap();
    let times = animation.prepared().keys().collect::<Vec<_>>();
    assert_eq!(times[1].1, times[2].1);
    assert_ne!(times[1].0, times[2].0);
    assert_eq!(animation.velocity(times[1].1).unwrap(), [0.4, 0., 0.]);
    let mut stationary = Track::constant([2.; 3]);
    for time in [0, 4, 10] {
        stationary.upsert(time, [2.; 3], Ease::Linear).unwrap();
    }
    stationary.enable_roving(&[4]).unwrap();
    for time in [-1., 0., 2.5, 4., 10., 11.] {
        assert_eq!(
            stationary
                .spatial_animation
                .as_ref()
                .unwrap()
                .velocity(time)
                .unwrap(),
            [0.; 3]
        );
    }
}
