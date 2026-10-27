//! Authored glTF fixtures; expected poses are independent geometric calculations.
use glam::{Mat4, Vec3};
use motion_scene::{
    import, AnimationSample, AssetResolver, Cause, ImportContext, ImportLimits, ResourceKind,
};
use serde_json::{json, Value};
use std::io;

struct Fixture {
    root: Value,
    bytes: Vec<u8>,
}
impl AssetResolver for Fixture {
    fn read(&mut self, path: &str, _: ResourceKind, limit: usize) -> io::Result<Vec<u8>> {
        assert_eq!(path, "scene.bin");
        if self.bytes.len() > limit {
            return Err(io::Error::other("fixture byte budget"));
        }
        Ok(self.bytes.clone())
    }
}
impl Fixture {
    fn new() -> Self {
        let mut f = Self {
            root: json!({"asset":{"version":"2.0"},
            "buffers":[{"uri":"scene.bin","byteLength":0}], "bufferViews":[], "accessors":[],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "nodes":[{"mesh":0,"translation":[4,5,6],"scale":[1,2,3]}],
            "scenes":[{"nodes":[0]}],"scene":0}),
            bytes: vec![],
        };
        let position = f.floats("VEC3", &[0., 0., 0., 1., 0., 0., 0., 1., 0.]);
        f.root["accessors"][position]["min"] = json!([0, 0, 0]);
        f.root["accessors"][position]["max"] = json!([1, 1, 0]);
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
        let views = self.root["bufferViews"].as_array_mut().unwrap();
        let view = views.len();
        views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":data.len()}));
        let accessors = self.root["accessors"].as_array_mut().unwrap();
        let index = accessors.len();
        accessors.push(json!({"bufferView":view,"componentType":component,"normalized":normalized,"type":shape,"count":count}));
        self.root["buffers"][0]["byteLength"] = json!(self.bytes.len());
        index
    }
    fn floats(&mut self, shape: &str, data: &[f32]) -> usize {
        let n = match shape {
            "SCALAR" => 1,
            "VEC3" => 3,
            "VEC4" => 4,
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
    fn clip(&mut self, path: &str, mode: &str, times: &[f32], values: &[f32]) {
        let input = self.floats("SCALAR", times);
        self.root["accessors"][input]["min"] = json!([times[0]]);
        self.root["accessors"][input]["max"] = json!([times[times.len() - 1]]);
        let output = self.floats(if path == "rotation" { "VEC4" } else { "VEC3" }, values);
        self.root["animations"] = json!([{"name":"Authored clip","channels":[{"sampler":0,"target":{"node":0,"path":path}}],
            "samplers":[{"input":input,"output":output,"interpolation":mode}]}]);
    }
    fn load(&mut self) -> motion_scene::Result<motion_scene::Scene> {
        self.limited(&ImportLimits::default())
    }
    fn limited(&mut self, limits: &ImportLimits) -> motion_scene::Result<motion_scene::Scene> {
        let json = serde_json::to_vec(&self.root).unwrap();
        import(
            ImportContext {
                request: 1,
                source: 87,
            },
            &json,
            self,
            limits,
        )
    }
    fn sampler(&mut self) -> &mut Value {
        &mut self.root["animations"][0]["samplers"][0]
    }
    fn output(&self) -> usize {
        self.root["animations"][0]["samplers"][0]["output"]
            .as_u64()
            .unwrap() as usize
    }
    fn input(&self) -> usize {
        self.root["animations"][0]["samplers"][0]["input"]
            .as_u64()
            .unwrap() as usize
    }
}
fn linear() -> Fixture {
    let mut f = Fixture::new();
    f.clip(
        "translation",
        "LINEAR",
        &[1., 3.],
        &[0., 0., 0., 4., 0., 0.],
    );
    f
}
fn pose(scene: &motion_scene::Scene, seconds: f64) -> Mat4 {
    Mat4::from_cols_array(
        &scene
            .sample_animation(AnimationSample {
                animation: 0,
                seconds,
            })
            .unwrap()[0],
    )
}
fn near(actual: Vec3, expected: Vec3) {
    assert!(
        (actual - expected).abs().max_element() < 0.0001,
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn explicit_linear_clip_clamps_endpoints_and_preserves_unanimated_components() {
    let mut f = linear();
    let scene = f.load().unwrap();
    assert_eq!(scene.animations()[0].name(), Some("Authored clip"));
    assert_eq!(scene.animations()[0].duration_seconds(), 3.);
    assert_eq!(scene.animations()[0].channel_count(), 1);
    near(
        pose(&scene, -99.).transform_point3(Vec3::Y),
        Vec3::new(0., 2., 0.),
    );
    near(
        pose(&scene, 2.).transform_point3(Vec3::Z),
        Vec3::new(2., 0., 3.),
    );
    near(
        pose(&scene, 99.).transform_point3(Vec3::ZERO),
        Vec3::new(4., 0., 0.),
    );
    let static_world = scene.instances(0, 100.).unwrap()[0].1;
    near(
        static_world.transform_point3(Vec3::ZERO),
        Vec3::new(400., 500., 600.),
    );
    let animated_world = scene
        .instances_at(
            0,
            100.,
            Some(AnimationSample {
                animation: 0,
                seconds: 2.,
            }),
        )
        .unwrap()[0]
        .1;
    near(
        animated_world.transform_point3(Vec3::ZERO),
        Vec3::new(200., 0., 0.),
    );
    assert_eq!(
        scene.nodes()[0].local,
        Mat4::from_scale_rotation_translation(
            Vec3::new(1., 2., 3.),
            glam::Quat::IDENTITY,
            Vec3::new(4., 5., 6.)
        )
        .to_cols_array()
    );
}
#[test]
fn step_changes_at_the_exact_key_and_single_key_linear_is_constant() {
    let mut f = linear();
    f.sampler()["interpolation"] = json!("STEP");
    let scene = f.load().unwrap();
    near(pose(&scene, 2.999).transform_point3(Vec3::ZERO), Vec3::ZERO);
    near(
        pose(&scene, 3.).transform_point3(Vec3::ZERO),
        Vec3::new(4., 0., 0.),
    );
    f.clip("translation", "LINEAR", &[2.], &[7., 8., 9.]);
    let scene = f.load().unwrap();
    for t in [-1., 2., 100.] {
        near(
            pose(&scene, t).transform_point3(Vec3::ZERO),
            Vec3::new(7., 8., 9.),
        );
    }
}
#[test]
fn rotations_use_shortest_arc_slerp_and_handle_opposite_quaternion_signs() {
    let mut f = Fixture::new();
    f.root["nodes"][0] = json!({"mesh":0});
    let a = 85f32.to_radians();
    f.clip(
        "rotation",
        "LINEAR",
        &[0., 2.],
        &[0., 0., a.sin(), a.cos(), 0., 0., -a.sin(), a.cos()],
    );
    let scene = f.load().unwrap();
    near(pose(&scene, 1.).transform_vector3(Vec3::X), -Vec3::X);
    f.clip(
        "rotation",
        "LINEAR",
        &[0., 2.],
        &[0., 0., 0., 1., 0., 0., 0., -1.],
    );
    let scene = f.load().unwrap();
    near(pose(&scene, 1.).transform_vector3(Vec3::X), Vec3::X);
}
#[test]
fn cubic_tangents_use_seconds_and_rotation_output_is_normalized() {
    let mut f = Fixture::new();
    f.clip(
        "translation",
        "CUBICSPLINE",
        &[1., 3.],
        &[
            0., 0., 0., 0., 0., 0., 4., 0., 0., 0., 0., 0., 4., 0., 0., 0., 0., 0.,
        ],
    );
    let scene = f.load().unwrap();
    near(
        pose(&scene, 2.).transform_point3(Vec3::ZERO),
        Vec3::new(3., 0., 0.),
    );
    near(
        pose(&scene, 99.).transform_point3(Vec3::ZERO),
        Vec3::new(4., 0., 0.),
    );
    f.root["nodes"][0] = json!({"mesh":0});
    f.clip(
        "rotation",
        "CUBICSPLINE",
        &[0., 2.],
        &[
            0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0.,
            0., 0.,
        ],
    );
    let scene = f.load().unwrap();
    near(pose(&scene, 1.).transform_vector3(Vec3::X), Vec3::Y);
}
#[test]
fn cubic_zero_quaternion_and_vector_overflow_return_errors_without_mutation() {
    let mut f = Fixture::new();
    f.clip(
        "rotation",
        "CUBICSPLINE",
        &[0., 2.],
        &[
            0., 0., 0., 0., 0., 0., 0., 1., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., -1., 0.,
            0., 0., 0.,
        ],
    );
    let scene = f.load().unwrap();
    assert!(scene
        .sample_animation(AnimationSample {
            animation: 0,
            seconds: 1.
        })
        .is_err());
    assert!(scene
        .sample_animation(AnimationSample {
            animation: 0,
            seconds: 0.
        })
        .is_ok());
    f.clip(
        "translation",
        "CUBICSPLINE",
        &[0., 100.],
        &[
            0.,
            0.,
            0.,
            0.,
            0.,
            0.,
            f32::MAX,
            0.,
            0.,
            0.,
            0.,
            0.,
            4.,
            0.,
            0.,
            0.,
            0.,
            0.,
        ],
    );
    let scene = f.load().unwrap();
    assert!(scene
        .sample_animation(AnimationSample {
            animation: 0,
            seconds: 50.
        })
        .is_err());
}
#[test]
fn normalized_integer_quaternions_are_decoded_including_signed_minimum() {
    for (component, bytes) in [
        (5120, vec![0, 0, 128, 0, 0, 0, 127, 0]),
        (5121, vec![0, 0, 255, 0, 0, 0, 255, 0]),
        (
            5122,
            [0i16, 0, i16::MIN, 0, 0, 0, i16::MAX, 0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
        (
            5123,
            [0u16, 0, u16::MAX, 0, 0, 0, u16::MAX, 0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
    ] {
        let mut f = linear();
        f.root["nodes"][0] = json!({"mesh":0});
        let output = f.data("VEC4", component, true, 2, &bytes);
        f.root["animations"][0]["channels"][0]["target"]["path"] = json!("rotation");
        f.sampler()["output"] = json!(output);
        let scene = f.load().unwrap();
        near(pose(&scene, 2.).transform_vector3(Vec3::X), -Vec3::X);
    }
}
#[test]
fn sampling_validates_clip_and_time_and_clips_remain_independent() {
    let mut f = linear();
    let mut second = f.root["animations"][0].clone();
    second["name"] = json!("Second");
    f.root["animations"].as_array_mut().unwrap().push(second);
    let scene = f.load().unwrap();
    for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(scene
            .sample_animation(AnimationSample {
                animation: 0,
                seconds: t
            })
            .is_err());
    }
    assert!(scene
        .sample_animation(AnimationSample {
            animation: 2,
            seconds: 0.
        })
        .is_err());
    assert_eq!(
        scene
            .sample_animation(AnimationSample {
                animation: 0,
                seconds: 2.
            })
            .unwrap(),
        scene
            .sample_animation(AnimationSample {
                animation: 1,
                seconds: 2.
            })
            .unwrap()
    );
}
#[test]
fn hierarchy_inherits_animated_parents_and_does_not_decompose_matrix_nodes() {
    let mut f = linear();
    f.root["nodes"] =
        json!([{"children":[1]}, {"mesh":0,"matrix":[1,0,0,0,0,1,0,0,0,0,1,0,0,5,0,1]}]);
    let scene = f.load().unwrap();
    let locals = scene
        .sample_animation(AnimationSample {
            animation: 0,
            seconds: 2.,
        })
        .unwrap();
    assert_eq!(locals[1], scene.nodes()[1].local);
    let world = scene
        .instances_at(
            0,
            10.,
            Some(AnimationSample {
                animation: 0,
                seconds: 2.,
            }),
        )
        .unwrap()[0]
        .1;
    near(world.transform_point3(Vec3::ZERO), Vec3::new(20., 50., 0.));
}
#[test]
fn invalid_animation_structure_and_unsupported_targets_remain_typed_failures() {
    let mut f = linear();
    f.root["animations"] = json!([{}]);
    assert!(matches!(f.load().unwrap_err().cause, Cause::Json(_)));
    let mut f = linear();
    f.root["animations"][0]["channels"][0]["target"]["path"] = json!("visibility");
    assert!(matches!(f.load().unwrap_err().cause, Cause::Unsupported(_)));
    let mut f = linear();
    f.root["animations"][0]["channels"][0]["target"]["path"] = json!("weights");
    let error = f.load().unwrap_err();
    assert!(matches!(error.cause, Cause::Invalid(_)));
    assert!(error
        .to_string()
        .contains("requires a mesh with morph targets"));
    let mut f = linear();
    f.root["animations"][0]["channels"][0]["target"]
        .as_object_mut()
        .unwrap()
        .remove("node");
    assert!(matches!(f.load().unwrap_err().cause, Cause::Unsupported(_)));
}
#[test]
fn duplicate_targets_matrix_targets_and_invalid_references_are_rejected() {
    for kind in 0..5 {
        let mut f = linear();
        match kind {
            0 => {
                let c = f.root["animations"][0]["channels"][0].clone();
                f.root["animations"][0]["channels"]
                    .as_array_mut()
                    .unwrap()
                    .push(c);
            }
            1 => f.root["nodes"][0] = json!({"mesh":0,"matrix":Mat4::IDENTITY.to_cols_array()}),
            2 => f.root["animations"][0]["channels"][0]["target"]["node"] = json!(9),
            3 => f.root["animations"][0]["channels"][0]["sampler"] = json!(9),
            _ => f.sampler()["output"] = json!(99),
        }
        assert!(
            matches!(f.load().unwrap_err().cause, Cause::Invalid(_)),
            "kind {kind}"
        );
    }
}
#[test]
fn invalid_key_order_bounds_counts_components_and_payloads_are_rejected() {
    for kind in 0..11 {
        let mut f = linear();
        let input = f.input();
        let output = f.output();
        match kind {
            0 => f.clip("translation", "LINEAR", &[-1., 3.], &[0.; 6]),
            1 => f.clip("translation", "LINEAR", &[1., 1.], &[0.; 6]),
            2 => f.clip("translation", "LINEAR", &[3., 1.], &[0.; 6]),
            3 => f.root["accessors"][input]
                .as_object_mut()
                .unwrap()
                .remove("min")
                .map(|_| ())
                .unwrap(),
            4 => f.root["accessors"][input]["max"] = json!([10.]),
            5 => f.root["accessors"][output]["count"] = json!(1),
            6 => f.root["accessors"][input]["normalized"] = json!(true),
            7 => f.sampler()["interpolation"] = json!("UNKNOWN"),
            8 => f.clip("translation", "CUBICSPLINE", &[1.], &[0.; 9]),
            9 => {
                let offset = f.root["bufferViews"][output]["byteOffset"]
                    .as_u64()
                    .unwrap() as usize;
                f.bytes[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
            }
            _ => f.clip("rotation", "LINEAR", &[0., 1.], &[0.; 8]),
        }
        assert!(f.load().is_err(), "kind {kind}");
    }
}
#[test]
fn animation_views_reject_vertex_targets_stride_and_wrong_shape() {
    for kind in 0..3 {
        let mut f = linear();
        let output = f.output();
        match kind {
            0 => f.root["bufferViews"][output]["target"] = json!(34962),
            1 => f.root["bufferViews"][output]["byteStride"] = json!(12),
            _ => f.root["accessors"][output]["type"] = json!("VEC4"),
        }
        assert!(f.load().is_err(), "kind {kind}");
    }
}
#[test]
fn key_descriptor_and_retained_budgets_cover_decoded_animation_payloads() {
    let mut f = linear();
    assert!(f
        .limited(&ImportLimits {
            max_animation_keys: 1,
            ..Default::default()
        })
        .is_err());
    assert!(f
        .limited(&ImportLimits {
            max_items: 1,
            ..Default::default()
        })
        .is_err());
    let scene = f.load().unwrap();
    let retained = scene.retained_bytes();
    assert!(f
        .limited(&ImportLimits {
            max_retained_bytes: retained - 1,
            ..Default::default()
        })
        .is_err());
    f.root.as_object_mut().unwrap().remove("animations");
    let rest = f.load().unwrap();
    assert_eq!(retained - rest.retained_bytes(), 2 * 4 + 2 * 3 * 4);
}

#[test]
fn several_channels_share_a_node_without_leaking_between_clips_or_samples() {
    let mut f = linear();
    let scale = f.floats("VEC3", &[1., 1., 1., 3., 3., 3.]);
    let input = f.input();
    f.root["animations"][0]["samplers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"input":input,"output":scale}));
    f.root["animations"][0]["channels"]
        .as_array_mut()
        .unwrap()
        .push(json!({"sampler":1,"target":{"node":0,"path":"scale"}}));
    let mut second = f.root["animations"][0].clone();
    second["channels"] = json!([{"sampler":1,"target":{"node":0,"path":"scale"}}]);
    f.root["animations"].as_array_mut().unwrap().push(second);
    let scene = f.load().unwrap();
    near(
        pose(&scene, 2.).transform_point3(Vec3::X),
        Vec3::new(4., 0., 0.),
    );
    let second = scene
        .sample_animation(AnimationSample {
            animation: 1,
            seconds: 2.,
        })
        .unwrap();
    near(
        Mat4::from_cols_array(&second[0]).transform_point3(Vec3::X),
        Vec3::new(6., 5., 6.),
    );
    near(pose(&scene, 1.).transform_point3(Vec3::X), Vec3::X);
    // A shared sampler is charged for every owned channel copy, across clips.
    assert!(f
        .limited(&ImportLimits {
            max_animation_keys: 5,
            ..Default::default()
        })
        .is_err());
}

#[test]
fn sparse_animation_accessors_and_frozen_bundles_keep_owned_clip_data() {
    let mut f = linear();
    let indices = f.data("SCALAR", 5121, false, 1, &[1]);
    let sparse_values = f.floats("VEC3", &[4., 0., 0.]);
    let output = f.output();
    let value_view = f.root["accessors"][sparse_values]["bufferView"].clone();
    let index_view = f.root["accessors"][indices]["bufferView"].clone();
    f.root["accessors"][output]
        .as_object_mut()
        .unwrap()
        .remove("bufferView");
    f.root["accessors"][output]["sparse"] = json!({"count":1,"indices":{"bufferView":index_view,"componentType":5121},"values":{"bufferView":value_view}});
    let scene = f.load().unwrap();
    near(
        pose(&scene, 2.).transform_point3(Vec3::ZERO),
        Vec3::new(2., 0., 0.),
    );
    let source = serde_json::to_vec(&f.root).unwrap();
    let bundle = scene.freeze(&source, 1 << 20).unwrap();
    f.bytes.fill(0);
    let restored = motion_scene::import_frozen(
        ImportContext {
            request: 2,
            source: 87,
        },
        &bundle,
        1 << 20,
        &ImportLimits::default(),
    )
    .unwrap();
    assert_eq!(restored, scene);
    assert_eq!(
        restored
            .sample_animation(AnimationSample {
                animation: 0,
                seconds: 2.
            })
            .unwrap(),
        scene
            .sample_animation(AnimationSample {
                animation: 0,
                seconds: 2.
            })
            .unwrap()
    );
}
