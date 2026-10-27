use base64::{engine::general_purpose::STANDARD, Engine};
use glam::{Mat4, Vec3};
use motion_scene::{
    import, import_frozen, AnimationSample, AssetResolver, CameraKind, ImportContext, ImportLimits,
    ResourceKind, Scene,
};
use serde_json::{json, Value};
use std::io;

const CONTEXT: ImportContext = ImportContext {
    request: 8705,
    source: 12,
};
struct NoAssets;
impl AssetResolver for NoAssets {
    fn read(&mut self, _: &str, _: ResourceKind, _: usize) -> io::Result<Vec<u8>> {
        panic!("camera fixtures are entirely owned inline resources")
    }
}
fn perspective() -> Value {
    json!({"type":"perspective", "perspective":{"yfov":1., "znear":0.2,"zfar":10.}})
}
fn root(camera: Value) -> Value {
    json!({"asset":{"version":"2.0"},"cameras":[camera],
        "nodes":[{"camera":0,"translation":[0,0,3]}],"scenes":[{"nodes":[0]}],"scene":0})
}
fn load(root: &Value) -> Scene {
    import(
        CONTEXT,
        &serde_json::to_vec(root).unwrap(),
        &mut NoAssets,
        &ImportLimits::default(),
    )
    .unwrap()
}
fn ndc(view: Mat4, point: Vec3) -> Vec3 {
    let p = view * point.extend(1.);
    p.truncate() / p.w
}

#[test]
fn finite_perspective_has_independent_zero_to_one_clipping_and_ignores_camera_scale() {
    let mut doc = root(perspective());
    doc["nodes"][0]["scale"] = json!([2, 3, 4]);
    let scene = load(&doc);
    let view = scene.camera_view(0, 0, 100., None, [200, 100]).unwrap();
    assert_eq!(view.kind, CameraKind::Perspective);
    assert_eq!(view.eye, Vec3::new(0., 0., 300.));
    assert_eq!(view.target, Vec3::new(0., 0., 299.));
    assert_eq!(view.viewport, [0., 0., 200., 100.]);
    assert!(ndc(view.view_projection, Vec3::new(0., 0., 280.)).z.abs() < 1e-5);
    assert!((ndc(view.view_projection, Vec3::new(0., 0., -700.)).z - 1.).abs() < 1e-5);
    let p = ndc(view.view_projection, Vec3::new(20., 10., 200.));
    let cot = 1. / 0.5f32.tan();
    assert!((p.x - 0.1 * cot).abs() < 1e-6 && (p.y - 0.1 * cot).abs() < 1e-6);
    assert!(scene.instances(0, 100.).unwrap().is_empty());
}

#[test]
fn infinite_perspective_retains_its_near_plane_and_asymptotic_depth() {
    let mut camera = perspective();
    camera["perspective"]
        .as_object_mut()
        .unwrap()
        .remove("zfar");
    let scene = load(&root(camera));
    let view = scene.camera_view(0, 0, 1., None, [100, 100]).unwrap();
    for distance in [0.2, 1., 10., 1000.] {
        let actual = ndc(view.view_projection, Vec3::new(0., 0., 3. - distance)).z;
        assert!(
            (actual - (1. - 0.2 / distance)).abs() < 2e-6,
            "{distance}: {actual}"
        );
    }
}

#[test]
fn authored_camera_aspects_letterbox_without_cropping_or_stretching() {
    let mut camera = perspective();
    camera["perspective"]["aspectRatio"] = json!(1.);
    let scene = load(&root(camera));
    assert_eq!(
        scene
            .camera_view(0, 0, 1., None, [200, 100])
            .unwrap()
            .viewport,
        [50., 0., 100., 100.]
    );
    let scene = load(&root(json!({"type":"orthographic","orthographic":{
        "xmag":2.,"ymag":1.,"znear":0.,"zfar":10.}})));
    let view = scene.camera_view(0, 0, 1., None, [200, 200]).unwrap();
    assert_eq!(view.viewport, [0., 50., 200., 100.]);
    assert_eq!(view.kind, CameraKind::Orthographic);
    assert!(
        (ndc(view.view_projection, Vec3::new(2., 1., 3.)) - Vec3::new(1., 1., 0.)).length() < 1e-6
    );
    assert!((ndc(view.view_projection, Vec3::new(0., 0., -7.)).z - 1.).abs() < 1e-6);
}

#[test]
fn camera_selection_follows_scene_hierarchy_and_explicit_transform_clip() {
    let mut doc = root(perspective());
    doc["nodes"] = json!([{"translation":[4.,5.,0.],"children":[1]}, {"camera":0}]);
    let values: [f32; 8] = [0., 2., 0., 0., 3., 2., 0., 3.];
    let bytes: Vec<u8> = values.into_iter().flat_map(f32::to_le_bytes).collect();
    doc["buffers"] = json!([{"uri":format!("data:application/octet-stream;base64,{}",STANDARD.encode(&bytes)),"byteLength":bytes.len()}]);
    doc["bufferViews"] =
        json!([{"buffer":0,"byteLength":8},{"buffer":0,"byteOffset":8,"byteLength":24}]);
    doc["accessors"] = json!([{"bufferView":0,"componentType":5126,"count":2,"type":"SCALAR","min":[0],"max":[2]},
        {"bufferView":1,"componentType":5126,"count":2,"type":"VEC3"}]);
    doc["animations"] = json!([{"samplers":[{"input":0,"output":1}],"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}]);
    let scene = load(&doc);
    let sample = scene
        .camera_view(
            0,
            1,
            10.,
            Some(AnimationSample {
                animation: 0,
                seconds: 1.,
            }),
            [100, 100],
        )
        .unwrap();
    assert_eq!(sample.eye, Vec3::new(50., 50., 30.));
    assert_eq!(
        scene.camera_view(0, 1, 10., None, [100, 100]).unwrap().eye,
        Vec3::new(40., 50., 0.)
    );
    assert!(scene
        .camera_view(0, 0, 1., None, [100, 100])
        .unwrap_err()
        .to_string()
        .contains("instantiate"));
    let mut other = doc;
    other["scenes"] = json!([{"nodes":[0]}, {"nodes":[]}]);
    assert!(load(&other)
        .camera_view(1, 1, 1., None, [100, 100])
        .unwrap_err()
        .to_string()
        .contains("absent"));
}

#[test]
fn frozen_camera_scene_revalidates_and_replays_identical_matrices() {
    let doc = root(perspective());
    let source = serde_json::to_vec(&doc).unwrap();
    let scene = load(&doc);
    let frozen = scene.freeze(&source, 1 << 20).unwrap();
    let restored = import_frozen(CONTEXT, &frozen, 1 << 20, &ImportLimits::default()).unwrap();
    assert_eq!(scene.cameras(), restored.cameras());
    assert_eq!(scene.nodes(), restored.nodes());
    assert_eq!(
        scene
            .camera_view(0, 0, 100., None, [800, 600])
            .unwrap()
            .view_projection,
        restored
            .camera_view(0, 0, 100., None, [800, 600])
            .unwrap()
            .view_projection
    );
}

#[test]
fn malformed_camera_descriptors_references_and_budgets_are_explicit_errors() {
    let mut bad = Vec::new();
    for (key, value) in [
        ("znear", 0.),
        ("yfov", 0.),
        ("yfov", std::f32::consts::PI),
        ("zfar", 0.1),
        ("aspectRatio", 0.),
    ] {
        let mut camera = perspective();
        camera["perspective"][key] = json!(value);
        bad.push(camera);
    }
    bad.push(json!({"type":"orthographic","orthographic":{"xmag":1,"ymag":1,"znear":1,"zfar":1}}));
    bad.push(json!({"type":"panorama"}));
    bad.push(
        json!({"type":"perspective","perspective":{"yfov":1,"znear":0.1},
        "orthographic":{"xmag":1,"ymag":1,"znear":0,"zfar":1}}),
    );
    bad.push(json!({"type":"perspective"}));
    for camera in bad {
        assert!(import(
            CONTEXT,
            &serde_json::to_vec(&root(camera)).unwrap(),
            &mut NoAssets,
            &ImportLimits::default()
        )
        .is_err());
    }
    let mut doc = root(perspective());
    doc["nodes"][0]["camera"] = json!(1);
    assert!(import(
        CONTEXT,
        &serde_json::to_vec(&doc).unwrap(),
        &mut NoAssets,
        &ImportLimits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("camera index"));
    let mut limits = ImportLimits::default();
    limits.max_retained_bytes = 1;
    assert!(import(
        CONTEXT,
        &serde_json::to_vec(&root(perspective())).unwrap(),
        &mut NoAssets,
        &limits
    )
    .is_err());
}

#[test]
fn invalid_camera_sampling_and_unrepresentable_hierarchies_do_not_manufacture_a_view() {
    let scene = load(&root(perspective()));
    for viewport in [[0, 100], [100, 0], [u32::MAX, 100]] {
        assert!(scene.camera_view(0, 0, 1., None, viewport).is_err());
    }
    for scale in [0., -1., f32::INFINITY, f32::MIN_POSITIVE] {
        assert!(scene.camera_view(0, 0, scale, None, [100, 100]).is_err());
    }
    let mut doc = root(perspective());
    doc["nodes"][0]["scale"] = json!([0, 1, 1]);
    assert!(load(&doc)
        .camera_view(0, 0, 1., None, [100, 100])
        .unwrap_err()
        .to_string()
        .contains("collapsed"));
    doc["nodes"][0]["scale"] = json!([-1, 1, 1]);
    assert!(load(&doc)
        .camera_view(0, 0, 1., None, [100, 100])
        .unwrap_err()
        .to_string()
        .contains("reflected"));
    let mut doc = root(perspective());
    doc["nodes"] = json!([{"scale":[2,1,1],"children":[1]},
        {"camera":0,"rotation":[0.,0.,0.38268343,0.9238795]}]);
    assert!(load(&doc)
        .camera_view(0, 1, 1., None, [100, 100])
        .unwrap_err()
        .to_string()
        .contains("sheared"));
}
