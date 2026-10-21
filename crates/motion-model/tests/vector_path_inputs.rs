use motion_model::{
    vector::{path_ops, SampledDashes, SampledPath, SampledTrimPaths, TrimMode},
    Error,
};

fn line() -> SampledPath {
    SampledPath {
        closed: false,
        nodes: vec![[0.; 6], [100., 0., 0., 0., 0., 0.]],
    }
}

fn trim(start: f32, end: f32, offset: f32) -> SampledTrimPaths {
    SampledTrimPaths {
        start,
        end,
        offset,
        mode: TrimMode::Simultaneously,
    }
}

fn bits(paths: &[SampledPath]) -> Vec<(bool, Vec<[u32; 6]>)> {
    paths
        .iter()
        .map(|path| {
            (
                path.closed,
                path.nodes
                    .iter()
                    .map(|node| node.map(f32::to_bits))
                    .collect(),
            )
        })
        .collect()
}

fn rejection(error: Error, operation: &str, node: usize, component: usize) {
    let Error::Invalid(message) = error else {
        panic!("path failure lost model error identity: {error:?}");
    };
    assert!(
        message.contains(&format!("operation={operation}")),
        "{message}"
    );
    assert!(message.contains("contour=1"), "{message}");
    assert!(message.contains(&format!("node={node}")), "{message}");
    assert!(
        message.contains(&format!("component={component}")),
        "{message}"
    );
}

#[test]
fn nonfinite_anchors_and_tangents_are_rejected_even_on_passthrough_and_empty_ranges() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for node in 0..2 {
            for component in 0..6 {
                let mut paths = vec![line(), line()];
                paths[1].nodes[node][component] = value;
                let before = bits(&paths);
                for settings in [
                    trim(0., 100., 0.),
                    trim(30., 30., 0.),
                    trim(20., 70., 0.),
                    trim(0., 50., 270.),
                ] {
                    rejection(
                        path_ops::trim(&paths, &settings).unwrap_err(),
                        "trim",
                        node,
                        component,
                    );
                    rejection(
                        path_ops::trim_by_path(&paths, &settings).unwrap_err(),
                        "trim",
                        node,
                        component,
                    );
                }
                for pattern in [vec![10., 0.], vec![10., 10.]] {
                    rejection(
                        path_ops::dash(
                            &paths,
                            &SampledDashes {
                                pattern,
                                offset: 0.,
                            },
                        )
                        .unwrap_err(),
                        "dash",
                        node,
                        component,
                    );
                }
                assert_eq!(
                    bits(&paths),
                    before,
                    "rejection modified caller-owned geometry"
                );
            }
        }
    }
}

#[test]
fn empty_and_degenerate_finite_contours_remain_normal_results() {
    let paths = vec![
        SampledPath {
            closed: false,
            nodes: vec![],
        },
        SampledPath {
            closed: true,
            nodes: vec![[3., 4., 0., 0., 0., 0.]],
        },
        line(),
    ];
    let before = bits(&paths);
    assert_eq!(path_ops::trim(&paths, &trim(0., 100., 0.)).unwrap(), paths);
    assert_eq!(
        path_ops::trim_by_path(&paths, &trim(30., 30., 0.)).unwrap(),
        vec![vec![]; 3]
    );
    assert_eq!(
        path_ops::trim(&paths, &trim(0., 50., 0.)).unwrap()[0].nodes[1][0],
        50.
    );
    assert_eq!(
        path_ops::dash(
            &paths,
            &SampledDashes {
                pattern: vec![10., 0.],
                offset: 0.
            }
        )
        .unwrap(),
        paths
    );
    let dashed = path_ops::dash(
        &paths,
        &SampledDashes {
            pattern: vec![10., 10.],
            offset: 0.,
        },
    )
    .unwrap();
    assert_eq!(dashed.len(), 5);
    assert_eq!(bits(&paths), before);
}
