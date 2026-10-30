use motion_model::{
    masks::LayerMask,
    vector::{PathNode, ShapeKind, TangentMode, VectorContent, VectorPath, VectorSource},
    Ease, Keyframe, Track,
};

fn path() -> VectorPath {
    VectorPath {
        id: 7,
        closed: true,
        tangents: TangentMode::Automatic,
        nodes: [[0., 0.], [90., 0.], [90., 60.], [0., 60.]]
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let mut geometry = Track::constant([p[0], p[1], 2., 3., 4., 5.]);
                geometry.keys = [0, 10]
                    .into_iter()
                    .map(|frame| Keyframe {
                        frame,
                        value: [p[0] + frame as f32 * (i + 1) as f32, p[1], 2., 3., 4., 5.],
                        ease: Ease::Linear,
                        curve: None,
                        spatial: None,
                    })
                    .collect();
                PathNode {
                    id: i as u64 + 1,
                    geometry,
                }
            })
            .collect(),
    }
}

#[test]
fn shape_and_mask_sample_the_same_constraint_at_fractional_frames_without_mutation() {
    let path = path();
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    vector.source = VectorSource::Paths {
        paths: vec![path.clone()],
    };
    let mask = LayerMask::new(1, path.clone());
    vector.validate().unwrap();
    mask.validate().unwrap();
    for frame in [0., 0.5, 1.25, 5., 9.99, 10.] {
        let sampled = path.sample(frame).unwrap();
        assert_eq!(vector.sample(frame, [128.; 2]).unwrap().paths[0], sampled);
        assert_eq!(mask.sample(frame).unwrap().unwrap().nodes, sampled.nodes);
        for (node, output) in path.nodes.iter().zip(&sampled.nodes) {
            let original = node.geometry.sample(frame).unwrap();
            assert_eq!(output[..2], original[..2]);
            assert_ne!(output[2..], original[2..]);
        }
    }
    assert_eq!(path, mask.path);
}

#[test]
fn manual_mode_restores_authored_handles_and_serialization_retains_automatic_mode() {
    let mut path = path();
    let saved = serde_json::to_value(&path).unwrap();
    assert_eq!(saved["tangents"], "automatic");
    assert_eq!(serde_json::from_value::<VectorPath>(saved).unwrap(), path);
    path.tangents = TangentMode::Manual;
    for frame in [0., 2.5, 10.] {
        assert_eq!(
            path.sample(frame).unwrap().nodes,
            path.nodes
                .iter()
                .map(|n| n.geometry.sample(frame).unwrap())
                .collect::<Vec<_>>()
        );
    }
    let manual = serde_json::to_value(&path).unwrap();
    assert!(manual.get("tangents").is_none());
    assert_eq!(serde_json::from_value::<VectorPath>(manual).unwrap(), path);
}

#[test]
fn seam_rotation_preserves_corresponding_tracks_and_sampling() {
    let original = path();
    let mut changed = original.clone();
    changed.set_first_vertex(3).unwrap();
    for frame in [0., 1.25, 4.5, 10.] {
        let mut expected = original.sample(frame).unwrap().nodes;
        expected.rotate_left(2);
        assert_eq!(changed.sample(frame).unwrap().nodes, expected);
    }
    for node in changed.nodes {
        assert_eq!(
            node,
            original
                .nodes
                .iter()
                .find(|n| n.id == node.id)
                .unwrap()
                .clone()
        );
    }
}

#[test]
fn invalid_sampling_retains_path_frame_and_original_cause() {
    use std::error::Error;
    let mut path = path();
    path.nodes[1].geometry.keys[0].value[0] = f32::NAN;
    let error = path.sample(0.).unwrap_err();
    assert!(error.to_string().contains("path=7 frame=0"));
    assert!(error.source().is_some());
}

#[test]
fn sampled_coordinates_use_the_existing_sample_bound_not_the_authored_track_bound() {
    let mut nodes = [[100000., 0., 0., 0., 0., 0.], [100010., 0., 0., 0., 0., 0.]];
    motion_model::vector::auto_tangents::apply(&mut nodes, false).unwrap();
    assert_eq!(nodes[0][4], 10. / 3.);
}

#[test]
fn animated_open_and_closed_paths_keep_correspondence_through_key_boundaries() {
    for closed in [false, true] {
        let mut path = path();
        path.closed = closed;
        let original = path.clone();
        let mut previous = path.sample(-0.01).unwrap().nodes;
        for tick in 0..=1001 {
            let frame = f64::from(tick) * 0.01;
            let sampled = path.sample(frame).unwrap();
            for ((node, g), before) in path.nodes.iter().zip(&sampled.nodes).zip(&previous) {
                assert_eq!(g[..2], node.geometry.sample(frame).unwrap()[..2]);
                assert!(g.iter().zip(before).all(|(a,b)|(a-b).abs()<0.25),
                    "unexpected geometric jump: closed={closed} frame={frame}");
            }
            previous = sampled.nodes;
        }
        assert_eq!(path, original);
    }
    // Cost observation, deliberately without a wall-clock assertion in CI.
    let mut large = path();
    large.closed = false;
    large.nodes = (0..2048).map(|i| PathNode {
        id:i+1, geometry:Track::constant([i as f32, (i as f32 * 0.1).sin(),0.,0.,0.,0.])
    }).collect();
    let start = std::time::Instant::now();
    for frame in 0..100 {
        assert_eq!(large.sample(f64::from(frame)).unwrap().nodes.len(),2048);
    }
    eprintln!("automatic tangent sampling: nodes=2048 samples=100 elapsed_us={}",start.elapsed().as_micros());
}
