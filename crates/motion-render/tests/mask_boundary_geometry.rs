use motion_core::{
    Layer, Project, Track,
    masks::LayerMask,
    vector::{PathNode, VectorPath},
};
use motion_render::{
    Renderer, Scene,
    mask_plan::{BoundaryGeometry, feather_support},
};

fn nodes(points: &[[f32; 2]]) -> Vec<[f32; 6]> {
    points
        .iter()
        .map(|p| [p[0], p[1], 0., 0., 0., 0.])
        .collect()
}
fn rectangle() -> Vec<[f32; 6]> {
    nodes(&[[24., 20.], [72., 20.], [72., 76.], [24., 76.]])
}
fn geometry(nodes: Vec<[f32; 6]>, expansion: f32) -> BoundaryGeometry {
    BoundaryGeometry::new(nodes, [0., 0., 96., 96.], [96., 96.], expansion, 1.).unwrap()
}
fn intervals(actual: Vec<[f32; 2]>, expected: Vec<[f32; 2]>) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} != {expected:?}");
    for (actual, expected) in actual.iter().flatten().zip(expected.iter().flatten()) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }
}

#[test]
fn sections_follow_union_and_erosion_instead_of_a_bounding_rectangle() {
    for (expansion, expected_x, expected_y) in [
        (0., vec![[24., 72.]], vec![[20., 76.]]),
        (8., vec![[16., 80.]], vec![[12., 84.]]),
        (-8., vec![[32., 64.]], vec![[28., 68.]]),
        (-40., vec![], vec![]),
    ] {
        let shape = geometry(rectangle(), expansion);
        intervals(shape.section(0, 48.).unwrap(), expected_x);
        intervals(shape.section(1, 48.).unwrap(), expected_y);
    }
    let concave = geometry(
        nodes(&[
            [16., 16.],
            [80., 16.],
            [80., 36.],
            [36., 36.],
            [36., 60.],
            [80., 60.],
            [80., 80.],
            [16., 80.],
        ]),
        0.,
    );
    intervals(concave.section(0, 48.).unwrap(), vec![[16., 36.]]);
    intervals(
        concave.section(1, 48.).unwrap(),
        vec![[16., 36.], [60., 80.]],
    );
    assert!(concave.section(0, 90.).unwrap().is_empty());
    let offset =
        BoundaryGeometry::new(rectangle(), [-14., -9., 124., 114.], [124., 114.], -8., 2.).unwrap();
    let interval = offset.section(0, 48.).unwrap();
    assert_eq!(interval.len(), 1);
    assert!((interval[0][0] - 32.).abs() < 0.0001 && (interval[0][1] - 64.).abs() < 0.0001);
}

#[test]
fn invalid_inputs_are_errors_and_empty_geometry_is_a_normal_miss() {
    for (shape, size, rect, expansion, density) in [
        (rectangle(), [0., 96.], [0., 0., 96., 96.], 0., 1.),
        (rectangle(), [96., 96.], [0., 0., f32::NAN, 96.], 0., 1.),
        (
            rectangle(),
            [96., 96.],
            [0., 0., 96., 96.],
            f32::INFINITY,
            1.,
        ),
        (rectangle(), [96., 96.], [0., 0., 96., 96.], 32001., 1.),
        (rectangle(), [96., 96.], [0., 0., 96., 96.], 0., 0.),
        (vec![[f32::NAN; 6]], [96., 96.], [0., 0., 96., 96.], 0., 1.),
        (vec![[0.; 6]; 2049], [96., 96.], [0., 0., 96., 96.], 0., 1.),
    ] {
        assert!(BoundaryGeometry::new(shape, rect, size, expansion, density).is_err());
    }
    let empty = geometry(vec![], 0.);
    assert!(empty.section(0, 48.).unwrap().is_empty());
    assert!(empty.section(2, 48.).is_err());
    assert!(empty.section(0, f32::NAN).is_err());
    for feather in [-1., f32::NAN, f32::INFINITY] {
        assert!(feather_support(feather).is_err());
    }
    for (feather, expected) in [
        (0., 0.),
        (1., 0.),
        (2., 1.),
        (8., 6.),
        (32., 24.),
        (32000., 24000.),
    ] {
        assert_eq!(feather_support(feather).unwrap(), expected);
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn actual_gpu_cross_sections_match_expansion_and_inversion_for_convex_and_concave_masks() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(96, 96).unwrap();
    for points in [
        rectangle(),
        nodes(&[
            [16., 16.],
            [80., 16.],
            [80., 36.],
            [36., 36.],
            [36., 60.],
            [80., 60.],
            [80., 80.],
            [16., 80.],
        ]),
    ] {
        for expansion in [0., 8., -8., -40.] {
            let shape = geometry(points.clone(), expansion);
            for inverted in [false, true] {
                let mut project = Project::new(96, 96, 30, 60).unwrap();
                project.background = [0.; 4];
                let mut layer = Layer::solid(1, "Mask", [96., 96.], [48., 48., 0.], [1.; 4]);
                let mut mask = LayerMask::new(
                    1,
                    VectorPath {
                        id: 1,
                        closed: true,
                        nodes: points
                            .iter()
                            .enumerate()
                            .map(|(i, p)| PathNode {
                                id: i as u64 + 1,
                                geometry: Track::constant(*p),
                            })
                            .collect(),
                    },
                );
                mask.expansion.value = expansion;
                mask.inverted = inverted;
                layer.masks.push(mask);
                project.layers.push(layer);
                let mut scene = Scene::new(&project).unwrap();
                scene
                    .sample(&project, 0., None, &motion_core::ExpressionEvaluator)
                    .unwrap();
                let pixels = renderer.capture(&scene, &target).unwrap().0;
                for axis in [0, 1] {
                    for cross in [27usize, 47, 68] {
                        let sections = shape.section(axis, cross as f32 + 0.5).unwrap();
                        for position in 1..95 {
                            let source = position as f32 + 0.5;
                            // Raster edge tie-breaking may differ by a subpixel.
                            // Compare away from edges; do not excuse whole pixels.
                            if sections
                                .iter()
                                .flatten()
                                .any(|edge| (source - edge).abs() < 0.05)
                            {
                                continue;
                            }
                            let inside =
                                sections.iter().any(|[lo, hi]| *lo < source && source < *hi);
                            let (x, y) = if axis == 0 {
                                (position, cross)
                            } else {
                                (cross, position)
                            };
                            let actual = pixels[(y * 96 + x) * 4 + 3];
                            assert_eq!(
                                actual,
                                if inside != inverted { 255 } else { 0 },
                                "expansion={expansion} inverted={inverted} axis={axis} pixel={x},{y} sections={sections:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
