//! Camera-specific editor geometry must match the renderer's camera fork,
//! without temporarily swapping the shared observer or changing history.
use super::*;
use motion_core::{Content, Layer, Property, Track};
use motion_core::vector::{PathNode, TangentMode, VectorContent, VectorPath, VectorSource};
use serde_json::{json, Value};

fn path(offset: f32) -> VectorPath {
    VectorPath {
        id: 1,
        closed: true,
        tangents: TangentMode::Manual,
        nodes: [[-3., -3.], [3., -3.], [3., 3.], [-3., 3.]]
            .into_iter().enumerate().map(|(i, [x,y])| PathNode {
                id: i as u64 + 1,
                geometry: Track::constant([x+offset, y+offset, 0., 0., 0., 0.]),
            }).collect(),
    }
}

fn fixture() -> Project {
    let mut p = Project::new(64, 64, 30, 60).unwrap();
    for (id, z) in [(1, -80.), (2, 80.)] {
        let mut layer = Layer::camera(id, "Independent input camera", 64, 64);
        if let Content::Camera { camera } = &mut layer.content {
            camera.position.value = [32., 32., z];
        }
        p.layers.push(layer);
    }
    for (id, z) in [(7, -5.), (8, 5.)] {
        let mut layer = Layer::solid(id, "Spatial selection target", [12.; 2], [32.,32.,z], [1.;4]);
        layer.three_d = true;
        if id == 7 {
            layer.masks.push(motion_core::masks::LayerMask {
                id: 1, name: "View-local mask".into(), path: path(6.),
                mode: Default::default(), enabled: true, inverted: false,
                opacity: Track::constant(100.), feather: Track::constant([0.;2]),
                feather_linked: true, expansion: Track::constant(0.),
            });
        }
        p.layers.push(layer);
    }
    let mut vector = Layer::solid(9, "Spatial path", [12.;2], [50.,48.,0.], [1.;4]);
    vector.three_d = true;
    let mut content = VectorContent::shape(motion_core::vector::ShapeKind::Rectangle);
    content.source = VectorSource::Paths { paths: vec![path(0.)] };
    vector.content = Content::Vector { vector: content };
    p.layers.push(vector);
    p.validate().unwrap();
    p
}

fn entry<'a>(state: &'a Value, field: &str, id: u64) -> &'a Value {
    state[field].as_array().unwrap().iter().find(|v| v["id"] == id).unwrap()
}
fn matrix(value: &Value) -> glam::Mat4 {
    let values = value.as_array().unwrap();
    glam::Mat4::from_cols_array(&std::array::from_fn(|i| values[i].as_f64().unwrap() as f32))
}

#[test]
fn explicit_view_geometry_and_picks_preserve_the_complete_shared_state() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Session::new(fixture(), root.path().into(), Arc::new(super::super::last_error_observation_tests::Stub)).unwrap();
    let before = s.snapshot().unwrap();
    assert_eq!(before, s.snapshot_for_view(ViewCamera::Composition).unwrap());
    let a = s.snapshot_for_view(ViewCamera::Camera(1)).unwrap();
    let b = s.snapshot_for_view(ViewCamera::Camera(2)).unwrap();
    for field in ["projectedLayers", "source_transforms", "mask_layers", "vector_layers"] {
        assert_ne!(a[field], b[field], "view-specific geometry stayed global: {field}");
    }
    for id in [7, 8, 9] {
        for (state, camera) in [(&a, ViewCamera::Camera(1)), (&b, ViewCamera::Camera(2))] {
            let scene = s.scene.fork_view(camera).unwrap();
            let model = scene.layers.iter().find(|l| l.id == id).unwrap();
            let projected = entry(state, "projectedLayers", id);
            let expected = motion_render::selection_geometry::polygon(model, &scene, &s.effects.registry).unwrap();
            assert_eq!(projected["corners"], json!(expected));
            // The original source plane and output bounds are different
            // contracts; both must use this view's evaluated projection.
            let world = scene.world_matrix(id).unwrap();
            let transform = entry(state, "source_transforms", id);
            assert_eq!(matrix(&transform["mvp"]), model.view_projection * world);
        }
    }
    let first = s.hit_candidates_for_view(ViewCamera::Camera(1), [32.,32.]).unwrap();
    let second = s.hit_candidates_for_view(ViewCamera::Camera(2), [32.,32.]).unwrap();
    assert_eq!(first["candidates"][0]["id"], 7);
    assert_eq!(second["candidates"][0]["id"], 8);
    let mut observer = motion_core::Observer::new(64,64);
    observer.view = motion_core::ObservationView::Side;
    let side = s.snapshot_for_view(ViewCamera::Observer(&observer)).unwrap();
    assert_eq!(side["observing"], true);
    assert_eq!(side["observationView"], "side");
    assert_ne!(side["source_transforms"], a["source_transforms"]);
    assert_eq!(s.snapshot().unwrap(), before, "read-only view query changed global state");

    for camera in [ViewCamera::Camera(999), ViewCamera::Camera(7)] {
        assert!(s.snapshot_for_view(camera).is_err());
        assert!(s.hit_candidates_for_view(camera, [32.,32.]).is_err());
    }
    assert!(s.hit_candidates_for_view(ViewCamera::Camera(1), [f64::NAN,0.]).is_err());
    assert_eq!(s.snapshot().unwrap(), before, "failed view query changed global state");

    // Editing remains one shared project transaction. Every view must expose
    // the edit, and undo must recover each view's original source geometry.
    s.engine.apply(motion_core::Command::SetVector {
        object: 7, property: Property::Position, frame: 0, value: [36.,32.,-5.],
    }).unwrap();
    s.sample().unwrap();
    for (original, camera) in [(&a, ViewCamera::Camera(1)), (&b, ViewCamera::Camera(2))] {
        let changed = s.snapshot_for_view(camera).unwrap();
        assert_ne!(entry(&changed,"source_transforms",7), entry(original,"source_transforms",7));
        assert_eq!(entry(&changed,"source_transforms",8), entry(original,"source_transforms",8));
        assert!(changed["canUndo"].as_bool().unwrap());
    }
    s.engine.undo().unwrap();
    s.sample().unwrap();
    for (original, camera) in [(&a, ViewCamera::Camera(1)), (&b, ViewCamera::Camera(2))] {
        let restored = s.snapshot_for_view(camera).unwrap();
        for field in ["source_transforms", "projectedLayers", "mask_layers", "vector_layers"] {
            assert_eq!(restored[field], original[field], "undo did not restore {field}");
        }
    }
}
