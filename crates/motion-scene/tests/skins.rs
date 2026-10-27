//! Small independently authored arrays; expected poses are evaluated by hand.
use glam::{Mat4, Vec3};
use motion_scene::{
    import, import_frozen, AnimationSample, AssetResolver, ImportContext, ImportLimits,
    ResourceKind, Scene,
};
use serde_json::{json, Value};
use std::io;

#[derive(Clone)]
struct Fixture {
    doc: Value,
    bytes: Vec<u8>,
}
impl AssetResolver for Fixture {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "skin.bin");
        if self.bytes.len() > limit {
            return Err(io::Error::other("fixture allowance"));
        }
        Ok(self.bytes.clone())
    }
}
impl Fixture {
    fn new() -> Self {
        let mut f = Self {
            doc: json!({"asset":{"version":"2.0"},"buffers":[{"uri":"skin.bin","byteLength":0}],
            "bufferViews":[],"accessors":[],"meshes":[],
            "skins":[{"joints":[2,3],"skeleton":1}],
            "nodes":[{"mesh":0,"skin":0,"translation":[100.,0.,0.]},{"children":[2],"translation":[0.,2.,0.]},{"children":[3],"translation":[1.,0.,0.]},{"translation":[0.,3.,0.]}],
            "scenes":[{"nodes":[0,1]}]}),
            bytes: vec![],
        };
        let p = f.floats("VEC3", &[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
        f.doc["accessors"][p]["min"] = json!([0., 0., 0.]);
        f.doc["accessors"][p]["max"] = json!([1., 1., 0.]);
        let j = f.data("VEC4", 5121, false, 3, &[0, 1, 0, 0].repeat(3));
        let w = f.floats("VEC4", &[0.25, 0.75, 0., 0.].repeat(3));
        f.doc["meshes"] =
            json!([{"primitives":[{"attributes":{"POSITION":p,"JOINTS_0":j,"WEIGHTS_0":w}}]}]);
        f
    }
    fn data(
        &mut self,
        shape: &str,
        component: u32,
        normalized: bool,
        count: usize,
        data: &[u8],
    ) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(data);
        let views = self.doc["bufferViews"].as_array_mut().unwrap();
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()}));
        let accessors = self.doc["accessors"].as_array_mut().unwrap();
        let a = accessors.len();
        accessors.push(json!({"bufferView":view,"type":shape,"componentType":component,"normalized":normalized,"count":count}));
        self.doc["buffers"][0]["byteLength"] = json!(self.bytes.len());
        a
    }
    fn floats(&mut self, shape: &str, data: &[f32]) -> usize {
        let n = match shape {
            "SCALAR" => 1,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => panic!("fixture shape"),
        };
        self.data(
            shape,
            5126,
            false,
            data.len() / n,
            &data
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
    }
    fn load(&self) -> motion_scene::Result<Scene> {
        self.limited(&ImportLimits::default())
    }
    fn limited(&self, limits: &ImportLimits) -> motion_scene::Result<Scene> {
        import(
            ImportContext {
                request: 87001,
                source: 87,
            },
            &serde_json::to_vec(&self.doc).unwrap(),
            &mut self.clone(),
            limits,
        )
    }
    fn attribute(&mut self, name: &str, a: usize) {
        self.doc["meshes"][0]["primitives"][0]["attributes"][name] = json!(a);
    }
    fn reject(&self, text: &str) {
        let e = self.load().unwrap_err();
        assert!(e.to_string().contains(text), "{e}");
    }
}
fn point(matrix: Mat4, expected: [f32; 3]) {
    assert!((matrix.transform_point3(Vec3::ZERO) - Vec3::from_array(expected)).length() < 1e-6);
}

#[test]
fn palettes_use_joint_world_inverse_bind_and_preserve_node_identity() {
    let mut f = Fixture::new();
    let b = f.floats(
        "MAT4",
        &[
            Mat4::from_translation(Vec3::new(-1., -2., 0.)).to_cols_array(),
            Mat4::from_translation(Vec3::new(-1., -5., 0.)).to_cols_array(),
        ]
        .concat(),
    );
    f.doc["skins"][0]["inverseBindMatrices"] = json!(b);
    let scene = f.load().unwrap();
    assert_eq!(scene.skins()[0].joints, [2, 3]);
    assert_eq!(scene.skins()[0].skeleton, Some(1));
    let before = scene.clone();
    let matrices = scene.skin_matrices_at(0, None).unwrap();
    let palette = matrices[0].as_ref().unwrap();
    assert_eq!(palette, &[Mat4::IDENTITY; 2]);
    let instances = scene.mesh_instances_at(0, 10., None).unwrap();
    assert_eq!(
        (instances[0].node, instances[0].mesh, instances[0].skin),
        (0, 0, Some(0))
    );
    point(instances[0].world, [1000., 0., 0.]);
    assert_eq!(scene, before);
    let source = serde_json::to_vec(&f.doc).unwrap();
    let frozen = scene.freeze(&source, 1 << 20).unwrap();
    assert_eq!(
        import_frozen(
            ImportContext {
                request: 87002,
                source: 87
            },
            &frozen,
            1 << 20,
            &ImportLimits::default()
        )
        .unwrap(),
        scene
    );
}

#[test]
fn animated_joint_parent_and_shared_mesh_distinct_skins_sample_without_autoplay() {
    let mut f = Fixture::new();
    let input = f.floats("SCALAR", &[0., 2.]);
    f.doc["accessors"][input]["min"] = json!([0.]);
    f.doc["accessors"][input]["max"] = json!([2.]);
    let output = f.floats("VEC3", &[0., 2., 0., 0., 6., 0.]);
    f.doc["animations"] = json!([{"samplers":[{"input":input,"output":output}],"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}]);
    f.doc["skins"]
        .as_array_mut()
        .unwrap()
        .push(json!({"joints":[3,2]}));
    f.doc["skins"]
        .as_array_mut()
        .unwrap()
        .push(json!({"joints":[2]})); // unused, no palette allocation
    f.doc["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"mesh":0,"skin":1,"scale":[0.,0.,0.]}));
    f.doc["scenes"][0]["nodes"] = json!([0, 1, 4]);
    let scene = f.load().unwrap();
    let before = scene.clone();
    let rest = scene.skin_matrices_at(0, None).unwrap();
    point(rest[0].as_ref().unwrap()[0], [1., 2., 0.]);
    for time in [0., 1., 2., 1.] {
        let palettes = scene
            .skin_matrices_at(
                0,
                Some(AnimationSample {
                    animation: 0,
                    seconds: time,
                }),
            )
            .unwrap();
        point(
            palettes[0].as_ref().unwrap()[0],
            [1., 2. + time as f32 * 2., 0.],
        );
        point(
            palettes[0].as_ref().unwrap()[1],
            [1., 5. + time as f32 * 2., 0.],
        );
        point(
            palettes[1].as_ref().unwrap()[0],
            [1., 5. + time as f32 * 2., 0.],
        );
        assert!(palettes[2].is_none());
    }
    assert_eq!(scene, before);
}

#[test]
fn unsigned_joint_widths_and_all_mixed_quantized_influence_sets_are_retained() {
    let mut f = Fixture::new();
    // Spec integer-unit example:85*257+43690=65535, eight distinct active joints.
    f.doc["nodes"][1]["children"] = json!((2..10).collect::<Vec<_>>());
    f.doc["nodes"][2]
        .as_object_mut()
        .unwrap()
        .remove("children");
    for _ in 4..10 {
        f.doc["nodes"].as_array_mut().unwrap().push(json!({}));
    }
    f.doc["skins"][0]["joints"] = json!((2..10).collect::<Vec<_>>());
    let j = f.data(
        "VEC4",
        5123,
        false,
        3,
        &[4u16, 5, 6, 7]
            .repeat(3)
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let j0 = f.data("VEC4", 5121, false, 3, &[0, 1, 2, 3].repeat(3));
    f.attribute("JOINTS_0", j0);
    let a = f.data("VEC4", 5121, true, 3, &[1, 4, 16, 64].repeat(3));
    f.attribute("WEIGHTS_0", a);
    let b = f.data(
        "VEC4",
        5123,
        true,
        3,
        &[640u16, 2080, 8200, 32770]
            .repeat(3)
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    f.attribute("JOINTS_1", j);
    f.attribute("WEIGHTS_1", b);
    let s = f.load().unwrap();
    let sets = &s.meshes()[0].primitives[0].joint_influences;
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[1].joints[2], [4, 5, 6, 7]);
    assert_eq!(sets[1].weights[0][3], 32770. / 65535.);
    let bad = f.data(
        "VEC4",
        5123,
        true,
        3,
        &[640u16, 2080, 8200, 32769]
            .repeat(3)
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    f.attribute("WEIGHTS_1", bad);
    f.reject("sum to one");
    // Prove the integer recovery used for exact quantized checks, across both domains.
    for n in 0..=65535u32 {
        assert_eq!(((n as f32 / 65535.) as f64 * 65535.).round() as u32, n);
    }
    for n in 0..=255u32 {
        assert_eq!(((n as f32 / 255.) as f64 * 255.).round() as u32, n);
    }
}

#[test]
fn sparse_joint_arrays_and_strided_weights_obey_accessor_layout() {
    let mut f = Fixture::new();
    let j = f.data("VEC4", 5121, false, 3, &[0, 1, 0, 0].repeat(3));
    // Sparse values restore all vertices; the dense source is absent.
    let values = f.doc["accessors"][j]["bufferView"].clone();
    f.doc["accessors"][j]
        .as_object_mut()
        .unwrap()
        .remove("bufferView");
    let indices = f.data("SCALAR", 5121, false, 3, &[0, 1, 2]);
    let index_view = f.doc["accessors"][indices]["bufferView"].clone();
    f.doc["accessors"][j]["sparse"] = json!({"count":3,"indices":{"bufferView":index_view,"componentType":5121},"values":{"bufferView":values}});
    f.attribute("JOINTS_0", j);
    let padded = [0.25f32, 0.75, 0., 0., 99.].repeat(3);
    let w = f.data(
        "VEC4",
        5126,
        false,
        3,
        &padded
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    );
    let view = f.doc["accessors"][w]["bufferView"].as_u64().unwrap() as usize;
    f.doc["bufferViews"][view]["byteStride"] = json!(20);
    f.attribute("WEIGHTS_0", w);
    let s = f.load().unwrap();
    assert_eq!(
        s.meshes()[0].primitives[0].joint_influences[0].weights[2],
        [0.25, 0.75, 0., 0.]
    );
    f.doc["bufferViews"][view]["byteStride"] = json!(18);
    f.reject("alignment or stride");
}

#[test]
fn malformed_influence_sets_weights_indices_and_types_are_explicit_errors() {
    let f = Fixture::new();
    let mut bad = f.clone();
    bad.doc["meshes"][0]["primitives"][0]["attributes"]
        .as_object_mut()
        .unwrap()
        .remove("JOINTS_0");
    bad.reject("paired");
    for name in ["JOINTS_01", "JOINTS_9", "JOINTS_999999999999"] {
        let mut bad = f.clone();
        let a = bad.doc["meshes"][0]["primitives"][0]["attributes"]["JOINTS_0"].clone();
        bad.doc["meshes"][0]["primitives"][0]["attributes"][name] = a;
        assert!(bad.load().is_err());
    }
    for weights in [
        [0., 0., 0., 0.],
        [-0.25, 1.25, 0., 0.],
        [0.25, 0.25, 0., 0.],
    ] {
        let mut bad = f.clone();
        let a = bad.floats("VEC4", &weights.repeat(3));
        bad.attribute("WEIGHTS_0", a);
        bad.reject("weight");
    }
    for joints in [[0, 0, 0, 0], [0, 1, 2, 0]] {
        let mut bad = f.clone();
        let a = bad.data("VEC4", 5121, false, 3, &joints.repeat(3));
        bad.attribute("JOINTS_0", a);
        bad.reject(if joints[1] == 0 {
            "repeated active joint"
        } else {
            "exceeds bound skin"
        });
    }
    let mut bad = f.clone();
    let a = bad.data("VEC4", 5121, true, 3, &[0, 1, 0, 0].repeat(3));
    bad.attribute("JOINTS_0", a);
    bad.reject("non-normalized");
    let mut bad = f.clone();
    let a = bad.data("VEC4", 5120, true, 3, &[64, 63, 0, 0].repeat(3));
    bad.attribute("WEIGHTS_0", a);
    bad.reject("component/normalization");
    let mut bad = f.clone();
    let a = bad.floats("VEC4", &[0.25, 0.75, 0., 0.]);
    bad.attribute("WEIGHTS_0", a);
    bad.reject("count mismatch");
}

#[test]
fn skin_hierarchy_scene_binding_and_inverse_bind_validation_are_bounded() {
    let f = Fixture::new();
    for skin in [
        json!({"joints":[]}),
        json!({"joints":[2,2]}),
        json!({"joints":[2,99]}),
        json!({"joints":[0,3]}),
        json!({"joints":[2,3],"skeleton":3}),
    ] {
        let mut bad = f.clone();
        bad.doc["skins"][0] = skin;
        assert!(bad.load().is_err());
    }
    let mut bad = f.clone();
    bad.doc["scenes"][0]["nodes"] = json!([0]);
    bad.reject("absent from referencing scene");
    let mut bad = f.clone();
    bad.doc["nodes"][0].as_object_mut().unwrap().remove("mesh");
    bad.reject("skin without mesh");
    let mut bad = f.clone();
    let b = bad.floats("MAT4", &Mat4::IDENTITY.to_cols_array());
    bad.doc["skins"][0]["inverseBindMatrices"] = json!(b);
    bad.reject("less than joint count");
    let mut bad = f.clone();
    let mut m = Mat4::IDENTITY.to_cols_array();
    m[3] = 0.1;
    let b = bad.floats("MAT4", &[m, m].concat());
    bad.doc["skins"][0]["inverseBindMatrices"] = json!(b);
    bad.reject("nonaffine");
    let s = f.load().unwrap();
    let limits = ImportLimits {
        max_retained_bytes: s.retained_bytes() - 1,
        ..ImportLimits::default()
    };
    assert!(f
        .limited(&limits)
        .unwrap_err()
        .to_string()
        .contains("retained byte limit"));
    let mut bad = f.clone();
    bad.doc["skins"]
        .as_array_mut()
        .unwrap()
        .extend(vec![json!({"joints":[2,3]}); 2]);
    let limits = ImportLimits {
        max_items: 5,
        ..ImportLimits::default()
    }; // aggregate six references
    assert!(bad
        .limited(&limits)
        .unwrap_err()
        .to_string()
        .contains("joint reference limit"));
}

#[test]
fn morph_precedes_two_joint_blend_with_inverse_transpose_normals() {
    let mut f = Fixture::new();
    let d = f.floats("VEC3", &[0.5, 0., 0.].repeat(3));
    f.doc["accessors"][d]["min"] = json!([0.5, 0., 0.]);
    f.doc["accessors"][d]["max"] = json!([0.5, 0., 0.]);
    f.doc["meshes"][0]["primitives"][0]["targets"] = json!([{"POSITION":d}]);
    let n = f.floats("VEC3", &[0., 0.6, 0.8].repeat(3));
    f.attribute("NORMAL", n);
    let t = f.floats("VEC4", &[1., 0., 0., -1.].repeat(3));
    f.attribute("TANGENT", t);
    let scene = f.load().unwrap();
    let p = &scene.meshes()[0].primitives[0];
    let before = p.clone();
    // .25*diag(2,1,1)+.75*diag(4,3,1)=diag(3.5,2.5,1), translation=(3,0,0).
    let palette = [
        Mat4::from_scale(Vec3::new(2., 1., 1.)),
        Mat4::from_scale_rotation_translation(
            Vec3::new(4., 3., 1.),
            glam::Quat::IDENTITY,
            Vec3::new(4., 0., 0.),
        ),
    ];
    let a = p.skin_attributes(&[1.], &palette).unwrap();
    assert_eq!(
        a.positions,
        [[4.75, 0., 0.], [8.25, 0., 0.], [4.75, 2.5, 0.]]
    );
    let normals = a.normals.unwrap();
    assert!((normals[0][1] - 0.24).abs() < 1e-7);
    assert_eq!(normals[0][2], 0.8);
    assert_eq!(a.tangents.unwrap()[0], [3.5, 0., 0., -1.]);
    assert_eq!(p, &before);
    let combined = scene.mesh_instances_with_skins_at(0, 10., None).unwrap();
    assert_eq!(combined.skins, scene.skin_matrices_at(0, None).unwrap());
    point(combined.meshes[0].world, [1000., 0., 0.]);
}

#[test]
fn effective_skin_overflow_singularity_and_reflection_refuse_instead_of_using_bind_pose() {
    let scene = Fixture::new().load().unwrap();
    let p = &scene.meshes()[0].primitives[0];
    let before = p.clone();
    for matrix in [
        Mat4::from_scale(Vec3::new(-1., 1., 1.)),
        Mat4::from_scale(Vec3::new(0., 1., 1.)),
    ] {
        let error = p.skin_attributes(&[], &[matrix; 2]).unwrap_err();
        assert!(error
            .to_string()
            .contains("singular or reflected effective skin transform"));
    }
    assert!(p
        .skin_attributes(&[], &[Mat4::IDENTITY])
        .unwrap_err()
        .to_string()
        .contains("exceeds palette"));
    let matrix = Mat4::from_scale_rotation_translation(
        Vec3::splat(f32::MAX),
        glam::Quat::IDENTITY,
        Vec3::splat(f32::MAX),
    );
    assert!(p
        .skin_attributes(&[], &[matrix; 2])
        .unwrap_err()
        .to_string()
        .contains("overflow"));
    assert_eq!(p, &before);
}
