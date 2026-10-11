use glam::{Mat4, Vec3};
use motion_core::*;
use motion_model::geometry::rotation_quaternion;
use motion_model::hierarchy;

fn project() -> Project {
    let mut p = Project::new(256, 256, 24, 48).unwrap();
    let mut layer = Layer::solid(1, "layer", [80., 40.], [128., 128., 0.], [1.; 4]);
    layer.three_d = true;
    layer.transform.rotation.value = [30., 40., 50.];
    p.layers.push(layer);
    p
}
fn reference(vector: Vec3, r: [f32; 3]) -> Vec3 {
    // Independently apply each axis to a vector in the editor's coordinates.
    let (x, y, z) = (r[0].to_radians(), r[1].to_radians(), r[2].to_radians());
    let a = Vec3::new(
        vector.x,
        vector.y * x.cos() - vector.z * x.sin(),
        vector.y * x.sin() + vector.z * x.cos(),
    );
    let b = Vec3::new(
        a.x * y.cos() + a.z * y.sin(),
        a.y,
        -a.x * y.sin() + a.z * y.cos(),
    );
    Vec3::new(
        b.x * z.cos() - b.y * z.sin(),
        b.x * z.sin() + b.y * z.cos(),
        b.z,
    )
}
#[test]
fn fixed_axes_match_sequential_rotations_and_legacy_order_remains_distinct() {
    let v = Vec3::new(1., 2., 3.);
    let world = Vec3::new(v.x, -v.y, -v.z);
    for r in [
        [0., 0., 0.],
        [30., 0., 0.],
        [0., 40., 0.],
        [0., 0., 50.],
        [30., 40., 50.],
        [-90., 45., 360.],
    ] {
        let actual = rotation_quaternion(r, RotationOrder::FixedXyz) * world;
        let expected = reference(v, r);
        let expected = Vec3::new(expected.x, -expected.y, -expected.z);
        assert!(
            actual.distance(expected) < 2e-6,
            "{r:?}: {actual:?} vs {expected:?}"
        );
    }
    assert!(
        (rotation_quaternion([30., 40., 50.], RotationOrder::FixedXyz) * world)
            .distance(rotation_quaternion([30., 40., 50.], RotationOrder::LocalXyz) * world)
            > 0.1
    );
}
#[test]
fn switching_order_is_an_explicit_pose_change_with_complete_history_and_format() {
    let mut e = Engine::new(project()).unwrap();
    let original = e.snapshot();
    let mut old = Vec::new();
    let mut states = Vec::new();
    hierarchy::matrices(&original, 0., &mut old, &mut states).unwrap();
    e.apply(Command::SetRotationOrder {
        object: 1,
        order: RotationOrder::FixedXyz,
    })
    .unwrap();
    assert_eq!(e.project().version, 16);
    assert_eq!(
        e.project().layers[0].transform,
        original.layers[0].transform
    );
    let mut fixed = Vec::new();
    hierarchy::matrices(e.project(), 0., &mut fixed, &mut states).unwrap();
    assert_ne!(old, fixed);
    let after = e.snapshot();
    e.undo().unwrap();
    assert_eq!(e.snapshot(), original);
    e.redo().unwrap();
    assert_eq!(e.snapshot(), after);
    assert_eq!(
        serde_json::from_slice::<Project>(&serde_json::to_vec(&after).unwrap()).unwrap(),
        after
    );
}
#[test]
fn anchor_compensation_keeps_geometry_after_order_switch_and_locked_edit_is_atomic() {
    let mut p = project();
    p.version = 16;
    p.layers[0].rotation_order = RotationOrder::FixedXyz;
    let mut e = Engine::new(p).unwrap();
    let mut world = Vec::new();
    let mut states = Vec::new();
    fn model(p: &Project, world: &mut Vec<Mat4>, states: &mut Vec<u8>) -> Mat4 {
        hierarchy::matrices(p, 0., world, states).unwrap();
        let l = &p.layers[0];
        let a = l.transform.anchor;
        world[0]
            * Mat4::from_translation(Vec3::new(
                (0.5 - a[0]) * l.size[0],
                (a[1] - 0.5) * l.size[1],
                0.,
            ))
    }
    let before = model(e.project(), &mut world, &mut states);
    e.apply(Command::Anchor {
        object: 1,
        anchor: [0.2, 0.8],
    })
    .unwrap();
    let after = model(e.project(), &mut world, &mut states);
    assert!(before
        .to_cols_array()
        .iter()
        .zip(after.to_cols_array())
        .all(|(a, b)| (*a - b).abs() < 1e-4));
    let mut locked = e.snapshot();
    locked.layers[0].locked = true;
    let mut e = Engine::new(locked.clone()).unwrap();
    assert!(e
        .apply(Command::SetRotationOrder {
            object: 1,
            order: RotationOrder::LocalXyz
        })
        .is_err());
    assert_eq!(e.snapshot(), locked);
}
#[test]
fn legacy_serialization_is_unchanged_and_fixed_order_requires_its_format() {
    let mut p = project();
    assert!(!serde_json::to_string(&p)
        .unwrap()
        .contains("rotation_order"));
    p.layers[0].rotation_order = RotationOrder::FixedXyz;
    assert!(p.validate().is_err());
    p.version = 16;
    p.validate().unwrap();
}

#[test]
fn reparenting_fixed_rotations_keeps_pose_and_tracks_without_euler_decomposition() {
    let mut p = project();
    p.version = 16;
    p.layers[0].rotation_order = RotationOrder::FixedXyz;
    let mut parent = Layer::solid(2, "parent", [1.; 2], [90., 140., 10.], [0.; 4]);
    parent.content = Content::Null;
    parent.three_d = true;
    parent.rotation_order = RotationOrder::FixedXyz;
    parent.transform.rotation.value = [-15., 35., 60.];
    p.layers.push(parent);
    let mut e = Engine::new(p).unwrap();
    let tracks = e.project().layers[0].transform.clone();
    let mut world = Vec::new();
    let mut states = Vec::new();
    hierarchy::matrices(e.project(), 7., &mut world, &mut states).unwrap();
    let before = world[0];
    for parent in [Some(2), None] {
        e.apply(Command::Parent {
            object: 1,
            parent,
            frame: 7,
        })
        .unwrap();
        hierarchy::matrices(e.project(), 7., &mut world, &mut states).unwrap();
        assert!(before.abs_diff_eq(world[0], 1e-4));
        assert_eq!(tracks, e.project().layers[0].transform);
    }
}

#[test]
fn new_shapes_use_fixed_axes_and_legacy_add_payloads_keep_their_order() {
    let mut e = Engine::new(Project::new(256, 256, 24, 48).unwrap()).unwrap();
    e.apply(Command::AddShape {
        id: 1,
        name: "shape".into(),
        shape: vector::ShapeKind::Rectangle,
        size: [40.; 2],
        position: [128., 128., 0.],
    })
    .unwrap();
    assert_eq!(
        e.project().layers[0].rotation_order,
        RotationOrder::FixedXyz
    );
    assert_eq!(e.project().version, 16);
    let old = Layer::solid(2, "old payload", [40.; 2], [128., 128., 0.], [1.; 4]);
    e.apply(Command::Add { layer: old }).unwrap();
    assert_eq!(
        e.project().layers[1].rotation_order,
        RotationOrder::LocalXyz
    );
    let before = e.snapshot();
    e.apply(Command::Duplicate { object: 1 }).unwrap();
    assert_eq!(
        e.project().layers[2].rotation_order,
        RotationOrder::FixedXyz
    );
    e.undo().unwrap();
    assert_eq!(e.snapshot(), before);
}

#[test]
fn particle_birth_poses_and_cache_follow_rotation_order_changes() {
    let mut p = project();
    p.version = 16;
    let package = motion_effects::builtin::particle_package().unwrap();
    let effect = EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        &package.manifest.effects[0],
        [80., 40.],
    );
    let mut world = Vec::new();
    let mut states = Vec::new();
    let old = particle_history::ParticleHistory::capture(&p, &p.layers[0], &effect, None).unwrap();
    p.layers[0].rotation_order = RotationOrder::FixedXyz;
    let fixed =
        particle_history::ParticleHistory::capture(&p, &p.layers[0], &effect, Some(&old)).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&old, &fixed));
    let pose = fixed.matrix_at(0., &mut world, &mut states).unwrap();
    hierarchy::matrices(&p, 0., &mut world, &mut states).unwrap();
    assert!(pose.abs_diff_eq(world[0], 1e-6));
    let unchanged =
        particle_history::ParticleHistory::capture(&p, &p.layers[0], &effect, Some(&fixed))
            .unwrap();
    assert!(std::sync::Arc::ptr_eq(&fixed, &unchanged));
}
