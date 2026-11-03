use motion_model::{
    tracking::{self, asset::*, point::*, shared::SharedData},
    *,
};
#[path = "support/tracking_plane.rs"]
mod fixture;
use fixture::plane_fixture;
#[path = "tracking_layer_time/plane.rs"]
mod mapped_clock;
#[test]
fn rational_project_clock_keeps_plane_interpolation_on_original_source_pts() {
    for (numerator, denominator) in [(30_000, 1001), (60_000, 1001), (2997, 100), (24, 1)] {
        let mut project = plane_fixture();
        project.fps = FrameRate::new(numerator, denominator).unwrap();
        project.validate().unwrap();
        // The fixture's original -4000/-2000 PTS pair translates by
        // (0,0)/(10,5) output pixels. Sample its physical midpoint.
        let frame = f64::from(numerator) / (48. * f64::from(denominator));
        let layer = &project.layers[0];
        let actual = layer.tracking_plane.as_ref().unwrap()
            .displacement(&project, layer, frame).unwrap();
        for corner in actual {
            for (value, expected) in corner.into_iter().zip([5., 2.5]) {
                assert!((value - expected).abs() < 0.00001, "{numerator}/{denominator}: {actual:?}");
            }
        }
    }
}

#[test]
fn equivalent_homogeneous_scales_preserve_perspective_and_reject_poles() {
    use tracking::plane::project_quad;
    let quad = [[0., 0.], [320., 0.], [320., 240.], [0., 240.]];
    let h = [[1., 0.1, 12.], [0.2, 1., 7.], [0.001, 0.002, 1.]];
    let a = project_quad(h, quad).unwrap();
    for scale in [-1., 1e-100, 1e100] {
        let b = project_quad(h.map(|r| r.map(|v| v * scale)), quad).unwrap();
        for i in 0..4 {
            for j in 0..2 {
                assert!((a[i][j] - b[i][j]).abs() < 1e-10);
            }
        }
    }
    assert!(project_quad([[1., 0., 0.], [0., 1., 0.], [1., 0., -160.]], quad).is_err());
    assert!(project_quad([[1., 0., 0.], [0., 1., 0.], [0., 0., 0.]], quad).is_err());
}
#[test]
fn source_pts_interpolation_and_authored_corner_correction_remain_independent() {
    let mut p = plane_fixture();
    let l = &p.layers[0];
    let b = l.tracking_plane.as_ref().unwrap();
    assert_eq!(b.displacement(&p, l, 0.5).unwrap(), [[5., 2.5]; 4]);
    assert_eq!(b.displacement(&p, l, 3.).unwrap(), [[25., 12.5]; 4]);
    p.layers[0].effects[0]
        .params
        .get_mut("tl")
        .unwrap()
        .track
        .value[0] = 3.;
    let l = &p.layers[0];
    let effect = &l.effects[0];
    let mut sampled = SampledEffect::new(7, effect);
    for (i, param) in effect.params.values().enumerate() {
        sampled.values[i] = param.sample(0.5).unwrap();
    }
    l.apply_tracking_plane(&p, 0.5, &mut sampled).unwrap();
    let i = sampled.param_ids.iter().position(|id| id == "tl").unwrap();
    assert_eq!(&sampled.values[i][..2], &[8., 2.5]);
    assert!(effect.params["tl"].track.keys.is_empty());
    assert_eq!(
        serde_json::from_slice::<Project>(&serde_json::to_vec(&p).unwrap()).unwrap(),
        p
    );
}
#[test]
fn gaps_are_explicit_and_geometric_failure_cannot_be_swallowed_by_missing_policy() {
    let mut p = plane_fixture();
    let mut data = (*p.tracking_assets[0].data).clone();
    data.tracks[0].samples[1].visibility = Visibility::Lost;
    p.tracking_assets[0].data = SharedData::new(data.clone()).unwrap();
    assert!(p.layers[0]
        .tracking_plane
        .as_ref()
        .unwrap()
        .displacement(&p, &p.layers[0], 0.5)
        .is_err());
    p.layers[0].tracking_plane.as_mut().unwrap().missing = Missing::AuthoredOnly;
    assert_eq!(
        p.layers[0]
            .tracking_plane
            .as_ref()
            .unwrap()
            .displacement(&p, &p.layers[0], 0.5)
            .unwrap(),
        [[0.; 2]; 4]
    );
    data.tracks[0].samples[1].visibility = Visibility::Visible;
    data.tracks[0].samples[1].observation = Some(Observation::Plane([
        [1., 0., 0.],
        [0., 1., 0.],
        [1., 0., -160.],
    ]));
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    assert!(p.layers[0]
        .tracking_plane
        .as_ref()
        .unwrap()
        .displacement(&p, &p.layers[0], 1.)
        .unwrap_err()
        .to_string()
        .contains("pole"));
}
#[test]
fn binding_rejects_wrong_effect_or_spatial_target() {
    for case in 0..4 {
        let mut p = plane_fixture();
        match case {
            0 => p.layers[0].three_d = true,
            1 => p.layers[0].tracking_plane.as_mut().unwrap().effect = 100,
            2 => p.layers[0].tracking_plane.as_mut().unwrap().track = "absent".into(),
            _ => p.layers[0].size[0] = 319.5,
        }
        assert!(p.validate().is_err());
    }
    assert!(serde_json::from_str::<Command>(r#"{"op":"set_tracking_plane","object":7}"#).is_err());
}

#[test]
fn projective_corners_interpolate_after_projection_and_respect_plate_axes() {
    let mut p = plane_fixture();
    let mut data = (*p.tracking_assets[0].data).clone();
    let h = [[1., 0.1, 12.], [0.2, 1., 7.], [0.001, 0.002, 1.]];
    data.tracks[0].samples[1].observation = Some(Observation::Plane(h.map(|r| r.map(|v| v * -3.))));
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    let l = &p.layers[0];
    let b = l.tracking_plane.as_ref().unwrap();
    let expected = tracking::plane::project_quad(h, b.reference_quad).unwrap();
    let delta = b.displacement(&p, l, 0.5).unwrap();
    for i in 0..4 {
        for axis in 0..2 {
            assert!(
                (f64::from(delta[i][axis]) - (expected[i][axis] - b.reference_quad[i][axis]) * 0.5)
                    .abs()
                    < 0.00002
            );
        }
    }
    let mut p = plane_fixture();
    let b = p.layers[0].tracking_plane.as_mut().unwrap();
    b.crop = [20., 30., 160., 120.];
    b.rotation = tracking::plate::Rotation::Clockwise90;
    let l = &p.layers[0];
    // Original delta (10,5) -> clockwise delta (-5,10), then resize 120x160 -> 320x240.
    let d = l
        .tracking_plane
        .as_ref()
        .unwrap()
        .displacement(&p, l, 1.)
        .unwrap();
    for v in d {
        assert!((v[0] + 40. / 3.).abs() < 0.00001);
        assert_eq!(v[1], 15.);
    }
}
#[test]
fn local_clip_reverse_freeze_and_explicit_hold_keep_original_pts() {
    let mut p = plane_fixture();
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 100,
        offset_frame: 10,
    });
    let b = p.layers[0].tracking_plane.as_mut().unwrap();
    b.source_anchor_pts = 0;
    b.speed_numerator = -1;
    let l = &p.layers[0];
    assert_eq!(
        l.tracking_plane
            .as_ref()
            .unwrap()
            .displacement(&p, l, 11.)
            .unwrap(),
        [[10., 5.]; 4]
    );
    p.layers[0].tracking_plane.as_mut().unwrap().speed_numerator = 0;
    let l = &p.layers[0];
    assert_eq!(
        l.tracking_plane
            .as_ref()
            .unwrap()
            .displacement(&p, l, 80.)
            .unwrap(),
        [[20., 10.]; 4]
    );
    p.layers[0].tracking_plane.as_mut().unwrap().speed_numerator = 1;
    p.layers[0].tracking_plane.as_mut().unwrap().interpolation = Interpolation::Hold;
    let l = &p.layers[0];
    assert_eq!(
        l.tracking_plane
            .as_ref()
            .unwrap()
            .displacement(&p, l, 11.)
            .unwrap(),
        [[20., 10.]; 4]
    );
}
#[test]
fn authored_folds_fail_but_disabled_effect_is_quiet() {
    let mut p = plane_fixture();
    p.layers[0].effects[0]
        .params
        .get_mut("tl")
        .unwrap()
        .track
        .value = [320., 240., 0., 0.];
    let l = &p.layers[0];
    let e = &l.effects[0];
    let mut sampled = SampledEffect::new(l.id, e);
    for (i, param) in e.params.values().enumerate() {
        sampled.values[i] = param.sample(0.).unwrap();
    }
    let error = l
        .apply_tracking_plane(&p, 0., &mut sampled)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("phase=sample") && error.contains("object=7") && error.contains("quad"),
        "{error}"
    );
    sampled.enabled = false;
    l.apply_tracking_plane(&p, 100000., &mut sampled).unwrap();
}

#[test]
fn ended_clip_does_not_demand_out_of_range_observations() {
    let mut p = plane_fixture();
    p.layers[0].timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 2,
        offset_frame: 0,
    });
    let l = &p.layers[0];
    let e = &l.effects[0];
    let mut sampled = SampledEffect::new(l.id, e);
    for (i, param) in e.params.values().enumerate() {
        sampled.values[i] = param.sample(20.).unwrap();
    }
    l.apply_tracking_plane(&p, 20., &mut sampled).unwrap();
    let i = sampled.param_ids.iter().position(|id| id == "tl").unwrap();
    assert_eq!(sampled.values[i], [0.; 4]);
    // An in-range hidden source still needs observations for matte/image reads.
    p.layers[0].visible = false;
    p.layers[0].tracking_plane.as_mut().unwrap().max_gap_ticks = 0;
    assert!(p.layers[0]
        .apply_tracking_plane(&p, 0.5, &mut sampled)
        .is_err());
}
