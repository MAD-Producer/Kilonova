use glam::{Mat4, Vec3};
use motion_core::{ExpressionEvaluator, Layer, ParentLink, Project, RotationOrder};
use motion_render::{Renderer, Scene};

fn sequential(v: Vec3) -> Vec3 {
    let [x, y, z] = [
        30_f32.to_radians(),
        40_f32.to_radians(),
        50_f32.to_radians(),
    ];
    let v = Vec3::new(
        v.x,
        v.y * x.cos() - v.z * x.sin(),
        v.y * x.sin() + v.z * x.cos(),
    );
    let v = Vec3::new(
        v.x * y.cos() + v.z * y.sin(),
        v.y,
        -v.x * y.sin() + v.z * y.cos(),
    );
    Vec3::new(
        v.x * z.cos() - v.y * z.sin(),
        v.x * z.sin() + v.y * z.cos(),
        v.z,
    )
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn combined_rotation_pixels_match_independent_fixed_axis_basis() {
    let mut p = Project::new(256, 256, 24, 48).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0., 0., 0., 1.];
    let mut l = Layer::solid(1, "plane", [160., 80.], [128., 128., 0.], [1.; 4]);
    l.three_d = true;
    l.rotation_order = RotationOrder::FixedXyz;
    l.transform.rotation.value = [30., 40., 50.];
    p.layers.push(l);
    let convert = |v: Vec3| Vec3::new(v.x, -v.y, -v.z);
    let basis = Mat4::from_cols(
        convert(sequential(convert(Vec3::X))).extend(0.),
        convert(sequential(convert(Vec3::Y))).extend(0.),
        convert(sequential(convert(Vec3::Z))).extend(0.),
        glam::Vec4::W,
    );
    let mut reference = p.clone();
    reference.layers[0].transform.rotation.value = [0.; 3];
    reference.layers[0].parent = Some(ParentLink {
        object: None,
        bind: basis.to_cols_array_2d(),
    });
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(256, 256).unwrap();
    let mut capture = |p: &Project| {
        let mut scene = Scene::new(p).unwrap();
        scene.sample(p, 7., None, &ExpressionEvaluator).unwrap();
        assert_eq!(scene.hit_candidates([128., 128.]).unwrap()[0].id, 1);
        renderer.capture(&scene, &target).unwrap().0
    };
    let actual = capture(&p);
    let expected = capture(&reference);
    let error: u64 = actual
        .iter()
        .zip(&expected)
        .map(|(a, b)| u64::from(a.abs_diff(*b)))
        .sum();
    assert!(
        error as f64 / (actual.len() as f64) < 0.1,
        "fixed axis image error {error}"
    );
    p.layers[0].rotation_order = RotationOrder::LocalXyz;
    let legacy = capture(&p);
    let changed = actual
        .chunks_exact(4)
        .zip(legacy.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > 1000,
        "synthetic angles must expose the old order: {changed}"
    );
}
