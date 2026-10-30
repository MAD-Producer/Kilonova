use motion_model::{Curve, CurveShape, CurveSpace, Ease, Easing, SpatialTangents, Track};

fn scalar() -> Track<f32> {
    let mut track=Track::constant(0.);
    for (frame,value) in [(0,0.),(10,10.),(40,70.),(70,80.)] {
        track.upsert(frame,value,Ease::Linear).unwrap();
    }
    track
}

#[test]
fn read_only_capability_matches_actual_edits_and_keeps_authored_storage_identical() {
    for enabled in [false, true] {
        for mode in ["linear", "automatic", "hold", "single", "velocity", "elastic", "nonfinite"] {
            let mut track = scalar();
            match mode {
                "automatic" => track.set_auto_bezier(10, true).unwrap(),
                "hold" => track.set_ease(10, Ease::Hold).unwrap(),
                "single" => track.keys.retain(|key| key.frame == 10),
                "velocity" => track.set_curve(0, Easing { ease:Ease::Linear, curve:Some(Curve {
                    space:CurveSpace::Velocity, shape:CurveShape::Cubic {
                        control1:[0.2,0.1], control2:[0.8,0.9], start:0., end:1.
                    }
                }) }).unwrap(),
                "elastic" => track.set_curve(0, Easing { ease:Ease::Linear, curve:Some(Curve {
                    space:CurveSpace::Progress, shape:CurveShape::Elastic {oscillations:2.,damping:5.}
                }) }).unwrap(),
                "nonfinite" => track.value = f32::INFINITY,
                _ => {}
            }
            let before = serde_json::to_vec(&track).unwrap();
            for frames in [vec![10], vec![0,10,40], vec![99]] {
                let available = track.validate_auto_bezier_edit(&frames, enabled).is_ok();
                let mut edited = track.clone();
                let accepted = frames.into_iter().try_for_each(|frame| edited.set_auto_bezier(frame, enabled)).is_ok();
                assert_eq!(available, accepted, "mode={mode} enabled={enabled}");
                assert_eq!(serde_json::to_vec(&track).unwrap(), before);
            }
            assert!(track.validate_auto_bezier_edit(&[], enabled).is_err());
        }
    }
}
fn endpoint(track:&Track<f32>,interval:usize,incoming:bool)->f64 {
    let a=&track.keys[interval];let b=&track.keys[interval+1];
    track.effective_easing(interval).unwrap().sample(if incoming {1.} else {0.}).velocity
        *(f64::from(b.value)-f64::from(a.value))/(f64::from(b.frame)-f64::from(a.frame))
}

#[test]
fn automatic_endpoint_has_one_physical_rate_on_unequal_intervals_and_reacts_to_neighbours() {
    let mut track=scalar();let authored=track.keys.clone();
    track.set_auto_bezier(10,true).unwrap();
    let left=endpoint(&track,0,true);let right=endpoint(&track,1,false);
    assert!((left-right).abs()<1e-10);
    assert!(left>1. && left<2.);
    assert_ne!(track.sample(4.5).unwrap(),4.5);
    for (a,b) in authored.iter().zip(&track.keys) {
        assert_eq!((a.frame,a.value,a.ease,a.curve,&a.spatial),(b.frame,b.value,b.ease,b.curve,&b.spatial));
    }
    track.set_at(40,100.).unwrap();
    assert!(track.keys[1].auto_bezier);
    assert!((endpoint(&track,0,true)-endpoint(&track,1,false)).abs()<1e-10);
    assert_ne!(endpoint(&track,0,true),left);
    let encoded=serde_json::to_string(&track).unwrap();
    let reopened:Track<f32>=serde_json::from_str(&encoded).unwrap();
    assert_eq!(track,reopened);
    for i in 0..700 { assert_eq!(track.sample(i as f64/10.).unwrap(),reopened.sample(i as f64/10.).unwrap()); }
}

#[test]
fn scalar_extrema_stop_and_dense_sampling_does_not_create_overshoot() {
    let mut track=Track::constant(0.);
    for (frame,value) in [(0,0.),(9,10.),(70,-20.),(90,30.)] {track.upsert(frame,value,Ease::Linear).unwrap();}
    for frame in [0,9,70,90] {track.set_auto_bezier(frame,true).unwrap();}
    assert_eq!(endpoint(&track,0,true),0.);
    assert_eq!(endpoint(&track,1,false),0.);
    for i in 0..9001 {
        let frame=i as f64/100.;let upper=track.keys.partition_point(|k|f64::from(k.frame)<=frame);
        if upper==0 || upper==track.keys.len() {continue;}
        let a=track.keys[upper-1].value;let b=track.keys[upper].value;let v=track.sample(frame).unwrap();
        assert!(v>=a.min(b)-1e-5 && v<=a.max(b)+1e-5,"{frame}: {v}");
    }
}

#[test]
fn switching_off_freezes_both_incident_intervals_and_preserves_other_manual_handles() {
    let mut track=scalar();
    let base=Easing{ease:Ease::Linear,curve:Some(Curve{space:CurveSpace::Progress,
        shape:CurveShape::Cubic{control1:[0.2,0.1],control2:[0.8,0.95],start:0.,end:1.}})};
    track.set_curve(0,base).unwrap();
    track.set_auto_bezier(10,true).unwrap();
    let CurveShape::Cubic{control1,..}=track.effective_easing(0).unwrap().curve.unwrap().shape else {panic!()};
    assert_eq!(control1,[0.2,0.1]);
    let values:Vec<_>=(0..700).map(|i|track.sample(i as f64/10.).unwrap()).collect();
    track.set_auto_bezier(10,false).unwrap();
    assert!(!track.keys[1].auto_bezier);
    for (i,value) in values.into_iter().enumerate(){assert_eq!(value,track.sample(i as f64/10.).unwrap());}
    let frozen=track.sample(4.5).unwrap();
    track.set_at(40,120.).unwrap();
    assert_eq!(track.sample(4.5).unwrap(),frozen);
}

#[test]
fn vector_automatic_timing_keeps_the_spatial_polyline_and_rejects_manual_spatial_handles() {
    let mut track=Track::constant([0.,0.,0.]);
    for (frame,value) in [(0,[0.,0.,0.]),(10,[10.,0.,0.]),(30,[10.,40.,0.])] {
        track.upsert(frame,value,Ease::Linear).unwrap();
    }
    track.set_auto_bezier(10,true).unwrap();
    for i in 0..300 {
        let t=i as f64/10.;let value=track.sample(t).unwrap();
        if t<10. {assert_eq!(value[1],0.);} else {assert_eq!(value[0],10.);}
        assert_eq!(value[2],0.);
    }
    let before=track.clone();
    assert!(track.set_spatial(10,Some(SpatialTangents{incoming:Some([-1.,0.,0.]),outgoing:Some([0.,1.,0.])})).is_err());
    assert_eq!(track,before);
    assert!(track.separate().is_err());
    assert_eq!(track,before);
    track.set_auto_bezier(10,false).unwrap();
    track.separate().unwrap();
}

#[test]
fn legacy_tracks_serialize_without_policy_and_hold_still_has_a_real_jump() {
    let mut track=scalar();
    assert!(!serde_json::to_string(&track).unwrap().contains("auto_bezier"));
    track.set_ease(0,Ease::Hold).unwrap();track.set_auto_bezier(10,true).unwrap();
    assert_eq!(track.sample(9.999).unwrap(),0.);
    assert_eq!(track.sample(10.).unwrap(),10.);
    assert_eq!(endpoint(&track,1,false),0.);
    for frame in [0,10,40] {track.delete_key(frame).unwrap();}
    assert!(!track.keys[0].auto_bezier);track.validate_local().unwrap();
}

#[test]
fn unsupported_and_malformed_automatic_requests_reject_without_mutating_the_track() {
    let mut track=scalar();
    track.set_curve(0,Easing{ease:Ease::Linear,curve:Some(Curve{space:CurveSpace::Progress,
        shape:CurveShape::Elastic{oscillations:2.,damping:5.}})}).unwrap();
    let before=track.clone();assert!(track.set_auto_bezier(10,true).is_err());assert_eq!(track,before);
    let mut geometry=Track::constant([0.;6]);
    geometry.upsert(0,[0.;6],Ease::Linear).unwrap();geometry.upsert(10,[1.;6],Ease::Linear).unwrap();
    let before=geometry.clone();assert!(geometry.set_auto_bezier(0,true).is_err());assert_eq!(geometry,before);
    let mut extreme=Track::constant(0.);
    for (frame,value) in [(i32::MIN,-1e30),(0,0.),(i32::MAX,1e30)]{extreme.upsert(frame,value,Ease::Linear).unwrap();}
    extreme.set_auto_bezier(0,true).unwrap();
    for time in [-1e9,-0.25,0.25,1e9] {assert!(extreme.sample(time).unwrap().is_finite());}
}

#[test]
fn moved_and_copied_keys_keep_their_policy_but_rederive_timing_from_new_neighbours() {
    let mut track = scalar();
    track.set_auto_bezier(10, true).unwrap();
    let original = endpoint(&track, 0, true);
    track.move_key(10, 20).unwrap();
    assert!(track.keys[1].auto_bezier);
    assert_eq!(track.keys[1].value, 10.);
    assert_ne!(original, endpoint(&track, 0, true));
    assert!((endpoint(&track, 0, true) - endpoint(&track, 1, false)).abs() < 1e-10);
    track.copy_key(20, 50).unwrap();
    assert!(track.keys[3].auto_bezier);
    assert_eq!(track.keys[3].value, 10.);
    // The copied value is a local minimum. Both incident rates become zero.
    assert_eq!(endpoint(&track, 2, true), 0.);
    assert_eq!(endpoint(&track, 3, false), 0.);
    track.validate_local().unwrap();
}
