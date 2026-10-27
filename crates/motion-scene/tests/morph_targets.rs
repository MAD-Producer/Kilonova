//! Independently authored displacement and Hermite references, not exported assets.
use motion_scene::{
    import, AnimationSample, AssetResolver, Cause, ImportContext, ImportLimits, ResourceKind, Scene,
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
        assert_eq!(path, "mesh.bin");
        if self.bytes.len() > limit {
            return Err(io::Error::other("fixture byte allowance"));
        }
        Ok(self.bytes.clone())
    }
}
impl Fixture {
    fn new() -> Self {
        let mut f = Self {
            doc: json!({"asset":{"version":"2.0"},"buffers":[{"uri":"mesh.bin","byteLength":0}],
            "bufferViews":[],"accessors":[],"meshes":[],"nodes":[{"mesh":0},{"mesh":0,"weights":[1.,0.]}],"scenes":[{"nodes":[0,1]}]}),
            bytes: vec![],
        };
        let p = f.vec3(&[[-0.8, -0.8, 0.], [0.8, -0.8, 0.], [0., 0.8, 0.]], true);
        let n = f.vec3(&[[0., 0., 1.]; 3], false);
        let tangent = f.floats("VEC4", &[1., 0., 0., -1., 1., 0., 0., -1., 1., 0., 0., -1.]);
        let a = f.vec3(&[[0.2, 0., 0.]; 3], true);
        let an = f.vec3(&[[1., 0., -1.]; 3], false);
        let at = f.vec3(&[[-1., 1., 0.]; 3], false);
        let b = f.vec3(&[[0., 0., 0.4]; 3], true);
        f.doc["meshes"] = json!([{"weights":[0.25,0.5],"primitives":[{"attributes":{"POSITION":p,"NORMAL":n,"TANGENT":tangent},
            "targets":[{"POSITION":a,"NORMAL":an,"TANGENT":at},{"POSITION":b}]}]}]);
        f
    }
    fn data(
        &mut self,
        shape: &str,
        component: u32,
        normalized: bool,
        count: usize,
        bytes: &[u8],
    ) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        let views = self.doc["bufferViews"].as_array_mut().unwrap();
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":bytes.len()}));
        let accessors = self.doc["accessors"].as_array_mut().unwrap();
        let id = accessors.len();
        accessors.push(json!({"bufferView":view,"componentType":component,"normalized":normalized,"type":shape,"count":count}));
        self.doc["buffers"][0]["byteLength"] = json!(self.bytes.len());
        id
    }
    fn floats(&mut self, shape: &str, values: &[f32]) -> usize {
        let width = match shape {
            "SCALAR" => 1,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => panic!("fixture shape"),
        };
        self.data(
            shape,
            5126,
            false,
            values.len() / width,
            &values
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        )
    }
    fn vec3(&mut self, values: &[[f32; 3]], bounds: bool) -> usize {
        let id = self.floats(
            "VEC3",
            &values.iter().flatten().copied().collect::<Vec<_>>(),
        );
        if bounds {
            self.doc["accessors"][id]["min"] = json!(std::array::from_fn::<_, 3, _>(|c| values
                .iter()
                .map(|v| v[c])
                .fold(f32::INFINITY, f32::min)));
            self.doc["accessors"][id]["max"] = json!(std::array::from_fn::<_, 3, _>(|c| values
                .iter()
                .map(|v| v[c])
                .fold(f32::NEG_INFINITY, f32::max)));
        }
        id
    }
    fn clip(&mut self, mode: &str, times: &[f32], values: &[f32]) {
        let input = self.floats("SCALAR", times);
        self.doc["accessors"][input]["min"] = json!([times[0]]);
        self.doc["accessors"][input]["max"] = json!([times[times.len() - 1]]);
        let output = self.floats("SCALAR", values);
        self.doc["animations"] = json!([{"channels":[{"sampler":0,"target":{"node":0,"path":"weights"}}],
            "samplers":[{"input":input,"output":output,"interpolation":mode}]}]);
    }
    fn load(&mut self) -> motion_scene::Result<Scene> {
        self.limited(&ImportLimits::default())
    }
    fn limited(&mut self, limits: &ImportLimits) -> motion_scene::Result<Scene> {
        let json = serde_json::to_vec(&self.doc).unwrap();
        import(
            ImportContext {
                request: 8704,
                source: 87,
            },
            &json,
            self,
            limits,
        )
    }
}
fn close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 2e-6, "{actual:?} != {expected:?}");
    }
}
fn weights(scene: &Scene, time: f64) -> Vec<f32> {
    scene
        .mesh_instances_at(
            0,
            1.,
            Some(AnimationSample {
                animation: 0,
                seconds: time,
            }),
        )
        .unwrap()[0]
        .weights
        .to_vec()
}

#[test]
fn authored_morph_defaults_node_overrides_and_object_space_displacements_remain_independent() {
    let mut f = Fixture::new();
    f.doc["nodes"][0]["scale"] = json!([2, 3, 4]);
    let scene = f.load().unwrap();
    let instances = scene.mesh_instances_at(0, 10., None).unwrap();
    assert_eq!(instances.len(), 2);
    assert_eq!((instances[0].node, instances[1].node), (0, 1));
    assert_eq!(instances[0].weights.as_ref(), [0.25, 0.5]);
    assert_eq!(instances[1].weights.as_ref(), [1., 0.]);
    assert!(std::ptr::eq(
        instances[0].weights.as_ptr(),
        scene.meshes()[0].weights.as_ptr()
    ));
    assert!(std::ptr::eq(
        instances[1].weights.as_ptr(),
        scene.nodes()[1].weights.as_ref().unwrap().as_ptr()
    ));
    assert!(instances.iter().all(|i| i.has_active_morph()));
    let p = &scene.meshes()[0].primitives[0];
    let a = p.morph_attributes(&instances[0].weights).unwrap();
    close(&a.positions[0], &[-0.75, -0.8, 0.2]);
    close(
        &instances[0]
            .world
            .transform_point3(glam::Vec3::from_array(a.positions[0]))
            .to_array(),
        &[-15., -24., 8.],
    );
    let b = p.morph_attributes(&instances[1].weights).unwrap();
    close(&b.normals.as_ref().unwrap()[0], &[1., 0., 0.]);
    close(&b.tangents.as_ref().unwrap()[0], &[0., 1., 0., -1.]);
    assert_eq!(p.positions[0], [-0.8, -0.8, 0.]);
    assert_eq!(scene.nodes()[0].weights, None);
    let base = p.morph_attributes(&[0., 0.]).unwrap();
    assert_eq!(base.positions, p.positions);
    let negative = p.morph_attributes(&[-2., 3.]).unwrap();
    close(&negative.positions[0], &[-1.2, -0.8, 1.2]);
    f.doc["meshes"][0]
        .as_object_mut()
        .unwrap()
        .remove("weights");
    assert_eq!(
        f.load().unwrap().mesh_instances_at(0, 1., None).unwrap()[0]
            .weights
            .as_ref(),
        [0., 0.]
    );
    assert!(!f.load().unwrap().mesh_instances_at(0, 1., None).unwrap()[0].has_active_morph());
}

#[test]
fn weight_animation_preserves_an_authored_node_matrix() {
    let mut f = Fixture::new();
    let matrix = glam::Mat4::from_translation(glam::vec3(2., -3., 4.)).to_cols_array();
    f.doc["nodes"][0]["matrix"] = json!(matrix);
    f.clip("LINEAR", &[0., 2.], &[0., 0., 1., 2.]);
    let scene = f.load().unwrap();
    let sample = AnimationSample {
        animation: 0,
        seconds: 1.,
    };
    assert_eq!(scene.sample_animation(sample).unwrap()[0], matrix);
    let instances = scene.mesh_instances_at(0, 10., Some(sample)).unwrap();
    assert_eq!(instances[0].weights.as_ref(), [0.5, 1.]);
    assert_eq!(
        instances[0].world.transform_point3(glam::Vec3::ZERO),
        glam::vec3(20., -30., 40.)
    );
    assert_eq!(scene.nodes()[0].local, matrix);
    f.doc["animations"][0]["channels"][0]["target"]["path"] = json!("translation");
    let error = f.load().unwrap_err();
    assert!(error.to_string().contains("matrix target"), "{error}");
}

#[test]
fn morph_weight_step_linear_and_cubic_use_scalar_groups_and_endpoint_clamping() {
    for mode in ["STEP", "LINEAR"] {
        let mut f = Fixture::new();
        f.clip(mode, &[1., 3.], &[0., 2., 2., -2.]);
        let s = f.load().unwrap();
        close(&weights(&s, -1.), &[0., 2.]);
        close(&weights(&s, 4.), &[2., -2.]);
        close(
            &weights(&s, 2.),
            if mode == "STEP" { &[0., 2.] } else { &[1., 0.] },
        );
        assert_eq!(
            s.mesh_instances_at(
                0,
                1.,
                Some(AnimationSample {
                    animation: 0,
                    seconds: 2.
                })
            )
            .unwrap()[1]
                .weights
                .as_ref(),
            [1., 0.]
        );
        // Weight animation must leave authored TRS untouched.
        assert_eq!(
            s.sample_animation(AnimationSample {
                animation: 0,
                seconds: 2.
            })
            .unwrap()[0],
            s.nodes()[0].local
        );
    }
    let mut f = Fixture::new();
    // Per key: two incoming tangents, two values, two outgoing tangents.
    f.clip(
        "CUBICSPLINE",
        &[1., 3.],
        &[0., 0., 0., 2., 2., -1., -1., 2., 2., -2., 0., 0.],
    );
    let s = f.load().unwrap();
    // t=.25: h00=.84375,h10=.140625,h01=.15625,h11=-.046875; dt=2.
    close(&weights(&s, 1.5), &[0.96875, 0.90625]);
    close(&weights(&s, 0.), &[0., 2.]);
    close(&weights(&s, 5.), &[2., -2.]);
    assert!(s
        .mesh_instances_at(
            0,
            1.,
            Some(AnimationSample {
                animation: 0,
                seconds: f64::NAN
            })
        )
        .is_err());
}

#[test]
fn morph_sparse_displacements_and_frozen_reimport_keep_owned_samples() {
    let mut f = Fixture::new();
    let target = f.doc["meshes"][0]["primitives"][0]["targets"][0]["POSITION"]
        .as_u64()
        .unwrap() as usize;
    let indices = f.data("SCALAR", 5121, false, 1, &[1]);
    let values = f.vec3(&[[0.6, 0., 0.]], false);
    let iv = f.doc["accessors"][indices]["bufferView"].clone();
    let vv = f.doc["accessors"][values]["bufferView"].clone();
    f.doc["accessors"][target] = json!({"componentType":5126,"type":"VEC3","count":3,"min":[0,0,0],"max":[0.6,0,0],
        "sparse":{"count":1,"indices":{"bufferView":iv,"componentType":5121},"values":{"bufferView":vv}}});
    let s = f.load().unwrap();
    let p = &s.meshes()[0].primitives[0];
    let out = p.morph_attributes(&[1., 0.]).unwrap();
    close(&out.positions[0], &[-0.8, -0.8, 0.]);
    close(&out.positions[1], &[1.4, -0.8, 0.]);
    let frozen = s
        .freeze(&serde_json::to_vec(&f.doc).unwrap(), 4 << 20)
        .unwrap();
    let reopened = motion_scene::import_frozen(
        ImportContext {
            request: 2,
            source: 87,
        },
        &frozen,
        4 << 20,
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(reopened.meshes(), s.meshes());
    assert_eq!(reopened.nodes(), s.nodes());
}

#[test]
fn morph_weight_normalized_integer_outputs_decode_without_probability_normalization() {
    for (component, data) in [
        (5120, vec![127u8, 128, 0, 127]),
        (5121, vec![255, 0, 0, 255]),
        (
            5122,
            [32767i16, -32768, 0, 32767]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
        (
            5123,
            [65535u16, 0, 0, 65535]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
    ] {
        let mut f = Fixture::new();
        f.clip("LINEAR", &[0., 2.], &[0., 0., 0., 0.]);
        let id = f.data("SCALAR", component, true, 4, &data);
        f.doc["animations"][0]["samplers"][0]["output"] = json!(id);
        let s = f.load().unwrap();
        close(
            &weights(&s, 1.),
            &[
                0.5,
                if component == 5120 || component == 5122 {
                    0.
                } else {
                    0.5
                },
            ],
        );
    }
}

#[test]
fn malformed_morph_shapes_weights_bounds_attributes_and_budgets_fail_explicitly() {
    let base = Fixture::new();
    for kind in 0..10 {
        let mut f = base.clone();
        match kind {
            0 => f.doc["meshes"][0]["weights"] = json!([]),
            1 => f.doc["nodes"][1]["weights"] = json!([1]),
            2 => f.doc["nodes"][1]
                .as_object_mut()
                .unwrap()
                .remove("mesh")
                .map(|_| ())
                .unwrap(),
            3 => {
                let p = f.doc["meshes"][0]["primitives"][0].clone();
                f.doc["meshes"][0]["primitives"]
                    .as_array_mut()
                    .unwrap()
                    .push(p);
                f.doc["meshes"][0]["primitives"][1]["targets"] = json!([]);
            }
            4 => f.doc["meshes"][0]["primitives"][0]["targets"][0] = json!({}),
            5 => f.doc["meshes"][0]["primitives"][0]["targets"][0]["POSITION"] = json!(99999),
            6 => f.doc["accessors"][3]["count"] = json!(2),
            7 => f.doc["accessors"][3]["max"] = json!([1, 0, 0]),
            8 => {
                f.doc["meshes"][0]["primitives"][0]["attributes"]
                    .as_object_mut()
                    .unwrap()
                    .remove("NORMAL");
            }
            _ => f.doc["meshes"][0]["primitives"][0]["targets"][0]["NORMAL"] = json!(3),
        }
        assert!(f.load().is_err(), "case {kind}");
    }
    let mut f = base.clone();
    f.doc["meshes"][0]["primitives"][0]["targets"][0]["TEXCOORD_0"] = json!(3);
    assert!(matches!(f.load().unwrap_err().cause, Cause::Unsupported(_)));
    let mut f = base.clone();
    let cost = f.load().unwrap().retained_bytes();
    assert!(f
        .limited(&ImportLimits {
            max_retained_bytes: cost - 1,
            ..Default::default()
        })
        .is_err());
    let mut f = base.clone();
    f.clip("LINEAR", &[0., 2.], &[0., 1., 2.]);
    assert!(f.load().is_err());
    let mut f = base;
    f.clip("LINEAR", &[0., 2.], &[0., 1., 2., 3.]);
    f.doc["meshes"][0]["primitives"][0]["targets"] = json!([]);
    f.doc["meshes"][0]
        .as_object_mut()
        .unwrap()
        .remove("weights");
    f.doc["nodes"][1].as_object_mut().unwrap().remove("weights");
    assert!(f.load().is_err());
}

#[test]
fn morph_sampling_refuses_overflow_and_keeps_authored_source_unchanged() {
    let s = Fixture::new().load().unwrap();
    let p = &s.meshes()[0].primitives[0];
    assert!(p.morph_attributes(&[f32::INFINITY, 0.]).is_err());
    assert!(p.morph_attributes(&[]).is_err());
    let mut p = p.clone();
    p.morph_targets[0].positions.as_mut().unwrap()[0][0] = f32::MAX;
    assert!(p.morph_attributes(&[f32::MAX, 0.]).is_err());
    assert_eq!(s.meshes()[0].primitives[0].positions[0], [-0.8, -0.8, 0.]);
}
