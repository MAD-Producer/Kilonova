use motion_core::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    vector::{ShapeKind, VectorContent, VectorSource},
    Content, Ease, Layer, LayerTimeline, Project,
};
use motion_host::{Platform, Session, SurfaceTarget, VideoDecoder, VideoQuery};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

struct NoMedia;
impl Platform for NoMedia {
    fn name(&self) -> &'static str {
        "layer-time-test"
    }
    fn probe_video(
        &self,
        _: &Path,
        _: Option<u32>,
        _: Option<u32>,
        _: &dyn Fn() -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_media::VideoProbe> {
        panic!("unexpected video probe")
    }
    fn open_decoder(
        &self,
        _: &Path,
        _: motion_core::VideoAsset,
        _: Vec<u64>,
    ) -> motion_media::Result<Box<dyn VideoDecoder>> {
        panic!("unexpected video decoder")
    }
    fn decode_audio(
        &self,
        _: &Path,
        _: &Path,
        _: Option<u32>,
        _: u64,
        _: &mut dyn FnMut(Option<f64>) -> motion_media::Result<()>,
    ) -> motion_media::Result<motion_core::AudioAsset> {
        panic!("unexpected audio decode")
    }
    fn media_capabilities(&self, _: Option<&VideoQuery>) -> motion_media::Result<Value> {
        Ok(json!({}))
    }
    fn attach_surface(
        &self,
        _: Arc<dyn wgpu::DisplayAndWindowHandle + Send + Sync>,
        _: u32,
        _: u32,
        _: motion_render::AllocationBudget,
    ) -> motion_host::Result<SurfaceTarget> {
        panic!("unexpected GPU work")
    }
}

fn project(clock: Option<LayerClock>) -> Project {
    let mut p = Project::new(64, 64, 30, 120).unwrap();
    let mut layer = Layer::solid(7, "shape", [24., 16.], [32., 32., 0.], [1.; 4]);
    layer.timeline = Some(LayerTimeline {
        in_frame: 10,
        out_frame: 110,
        offset_frame: 10,
    });
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    let VectorSource::Shape { parameters, .. } = &mut vector.source else {
        panic!("shape fixture")
    };
    let angle = parameters.get_mut("angle").unwrap();
    angle.upsert(0, 0., Ease::Linear).unwrap();
    angle.upsert(40, 80., Ease::Linear).unwrap();
    layer
        .transform
        .opacity
        .upsert(0, 0.2, Ease::Linear)
        .unwrap();
    layer
        .transform
        .opacity
        .upsert(1, 0.22, Ease::Linear)
        .unwrap();
    layer
        .transform
        .opacity
        .upsert(40, 1., Ease::Linear)
        .unwrap();
    layer.content = Content::Vector { vector };
    layer.time_mapping = clock.map(|clock| LayerTimeMapping { clock, remap: None });
    p.layers.push(layer);
    p
}

#[test]
fn host_shape_parameters_and_geometry_share_the_property_clock() {
    for (clock, cases, key_times) in [
        (None, [(10., 0.), (30., 40.), (50., 80.)], [10., 11., 50.]),
        (
            Some(
                LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    TimeDirection::Forward,
                    0.,
                    0.,
                )
                .unwrap(),
            ),
            [(10., 0.), (40., 40.), (70., 80.)],
            [10., 11.5, 70.],
        ),
        (
            Some(
                LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    TimeDirection::Reverse,
                    0.,
                    40.,
                )
                .unwrap(),
            ),
            [(10., 80.), (40., 40.), (70., 0.)],
            [70., 68.5, 10.],
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let p = project(clock);
        let original = p.clone();
        let mut session = Session::new(p, directory.path().into(), Arc::new(NoMedia)).unwrap();
        for (composition_time, expected_angle) in cases {
            session.frame = composition_time;
            session.sample().unwrap();
            let state = session.snapshot().unwrap();
            let shown = &state["vector_layers"][0];
            assert_eq!(
                shown["parameters"]["angle"],
                json!(expected_angle),
                "clock={clock:?} C={composition_time}"
            );
            let mut expected_vector = VectorContent::shape(ShapeKind::Rectangle);
            let VectorSource::Shape { parameters, .. } = &mut expected_vector.source else {
                panic!("shape fixture")
            };
            parameters.get_mut("angle").unwrap().value = expected_angle as f32;
            let expected = expected_vector.sample(0., [24., 16.]).unwrap();
            for (actual, expected) in shown["paths"]
                .as_array()
                .unwrap()
                .iter()
                .zip(&expected.paths)
            {
                let nodes: Vec<_> = actual["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n["geometry"].clone())
                    .collect();
                assert_eq!(
                    nodes,
                    expected.nodes.iter().map(|n| json!(n)).collect::<Vec<_>>()
                );
            }
            assert_eq!(
                shown["paths"].as_array().unwrap().len(),
                expected.paths.len()
            );
            assert_eq!(state["project"], serde_json::to_value(&original).unwrap());
            for (key, expected_time) in state["timeline_layers"][0]["properties"]["opacity"]["keys"]
                .as_array()
                .unwrap()
                .iter()
                .zip(key_times)
            {
                assert_eq!(key["frame"].as_f64().unwrap(), expected_time);
            }
        }
        assert_eq!(
            session.engine.project(),
            &original,
            "reads cannot reauthor keys"
        );
    }
}

#[test]
fn host_command_snapshot_save_and_undo_preserve_local_keys_and_fractional_projection() {
    let directory = tempfile::tempdir().unwrap();
    let original = project(None);
    let mut session =
        Session::new(original.clone(), directory.path().into(), Arc::new(NoMedia)).unwrap();
    session.frame = 40.;
    session.sample().unwrap();
    let before = session.snapshot().unwrap();
    let mapped = session.apply_commands(r#"{"op":"layer_time","object":7,"action":{"kind":"stretch","stretch":{"numerator":3,"denominator":2},"anchor_frame":10,"resize_clip":false}}"#).unwrap();
    assert_eq!(mapped["vector_layers"][0]["parameters"]["angle"], 40.);
    assert_eq!(
        mapped["timeline_layers"][0]["properties"]["opacity"]["keys"][1]["frame"],
        11.5
    );
    let current = session.engine.project().clone();
    assert_eq!(current.layers[0].content, original.layers[0].content);
    assert_eq!(current.layers[0].transform, original.layers[0].transform);
    assert_eq!(current.layers[0].timeline, original.layers[0].timeline);
    motion_core::storage::save(directory.path(), &current).unwrap();
    let reopened = motion_core::storage::load(directory.path()).unwrap();
    assert_eq!(reopened, current);
    let mut other = Session::new(reopened, directory.path().into(), Arc::new(NoMedia)).unwrap();
    other.frame = 40.;
    other.sample().unwrap();
    let reopened = other.snapshot().unwrap();
    for field in [
        "project",
        "timeline_layers",
        "vector_layers",
        "sampledLayers",
    ] {
        assert_eq!(mapped[field], reopened[field], "saved field={field}");
    }
    let failed = session
        .apply_commands(r#"{"op":"layer_time","object":999,"action":{"kind":"reverse"}}"#)
        .unwrap_err();
    assert!(failed.to_string().contains("999"), "{failed}");
    assert_eq!(session.snapshot().unwrap(), mapped_with_no_results(&mapped));
    session.engine.undo().unwrap();
    session.sample().unwrap();
    let restored = session.snapshot().unwrap();
    for field in [
        "project",
        "timeline_layers",
        "vector_layers",
        "sampledLayers",
    ] {
        assert_eq!(restored[field], before[field], "undo field={field}");
    }
    session.engine.redo().unwrap();
    session.sample().unwrap();
    let redone = session.snapshot().unwrap();
    for field in [
        "project",
        "timeline_layers",
        "vector_layers",
        "sampledLayers",
    ] {
        assert_eq!(redone[field], mapped[field], "redo field={field}");
    }
}

fn mapped_with_no_results(mapped: &Value) -> Value {
    let mut expected = mapped.clone();
    expected.as_object_mut().unwrap().remove("edit_result");
    expected.as_object_mut().unwrap().remove("edit_results");
    expected
}
