use super::*;
use motion_model::{
    layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch},
    Composition, CompositionClip, Layer, LayerTimeline, VideoAsset, VideoClip, WorkArea,
};

fn fixture() -> Project {
    let mut p = Project::new(16, 16, 10, 20).unwrap();
    let asset: VideoAsset = serde_json::from_value(serde_json::json!({
        "id":1,"path":"assets/test.mp4","bytes":4,"mime":"video/avc","track":0,
        "width":2,"height":2,"rotation":0,"display_width":2,"display_height":2,
        "video_start_us":0,"video_end_us":1000000,"duration_us":1000000,
        "frame_count":10,"variable_frame_rate":false,"nominal_frame_rate":10.,
        "color_standard":1,"color_range":2,"audio_asset":null
    }))
    .unwrap();
    p.video_assets.push(asset);
    let mut l = Layer::solid(7, "upcoming", [16.; 2], [8., 8., 0.], [1.; 4]);
    l.content = Content::Video {
        video: VideoClip::new(1),
    };
    l.timeline = Some(LayerTimeline {
        in_frame: 5,
        out_frame: 10,
        offset_frame: 5,
    });
    p.layers.push(l);
    p
}
fn plan(p: &Project, frame: f64) -> Vec<UpcomingVideo<'_>> {
    p.validate().unwrap();
    VideoLookahead::default()
        .plan(p, &p.composition_id, frame, 4, &[], 4)
        .unwrap()
}
fn nest(p: &mut Project) {
    let mut video = p.layers.remove(0);
    video.timeline = Some(LayerTimeline {
        in_frame: 2,
        out_frame: 20,
        offset_frame: 2,
    });
    p.compositions.push(Composition {
        id: "comp-warm-child".into(),
        name: "comp-warm-child".into(),
        width: 16,
        height: 16,
        fps: motion_model::FrameRate::integer(20).unwrap(),
        frames: 20,
        work_area: WorkArea::full(20),
        guides: vec![],
        background: p.background,
        camera: p.camera.clone(),
        layers: vec![video],
        expressions: vec![],
    });
    for (id, offset) in [(17, 5), (18, 6)] {
        let mut l = Layer::solid(id, "reference", [16.; 2], [8., 8., 0.], [1.; 4]);
        l.content = Content::Composition {
            clip: CompositionClip::new("comp-warm-child".into()),
        };
        l.timeline = Some(LayerTimeline {
            in_frame: offset,
            out_frame: 15,
            offset_frame: offset,
        });
        p.layers.push(l);
    }
}

#[test]
fn upcoming_root_uses_clip_offset_reverse_endpoint_and_nearest_instant() {
    let mut p = fixture();
    assert!(plan(&p, 0.).is_empty());
    assert_eq!(
        plan(&p, 1.),
        vec![UpcomingVideo {
            object: 7,
            asset: &p.video_assets[0],
            source_time_us: 0,
            blend: false,
            output_steps: 4
        }]
    );
    assert!(plan(&p, 9.).is_empty(), "clip out is exclusive");
    p.layers[0].time_mapping = Some(LayerTimeMapping {
        clock: LayerClock::new(TimeStretch::default(), TimeDirection::Reverse, 0., 10.).unwrap(),
        remap: None,
    });
    assert_eq!(
        plan(&p, 4.)[0].source_time_us,
        999_999,
        "reverse starts at the last source sample"
    );
    let r = VideoLookahead::default()
        .plan(&p, &p.composition_id, 4., 4, &[7], 4)
        .unwrap();
    assert!(r.is_empty(), "mandatory instance is not admitted again");
}

#[test]
fn inactive_nested_references_keep_the_actual_render_alias_and_child_fps() {
    let mut p = fixture();
    nest(&mut p);
    let before = serde_json::to_value(&p).unwrap();
    let result = plan(&p, 4.);
    assert_eq!(result.len(), 2);
    assert_ne!(result[0].object, result[1].object);
    assert_eq!((result[0].output_steps, result[0].source_time_us), (2, 0));
    assert_eq!((result[1].output_steps, result[1].source_time_us), (3, 0));
    for candidate in &result {
        let mut actual = crate::Scene::new(&p).unwrap();
        actual
            .sample(
                &p,
                4. + candidate.output_steps as f64,
                None,
                &motion_core::ExpressionEvaluator,
            )
            .unwrap();
        let videos = actual.video_decode_layers();
        let l = videos
            .iter()
            .find(|l| l.id == candidate.object)
            .expect("warm alias must equal actual sampling");
        assert_eq!(
            l.video.as_ref().unwrap().source_time_us,
            candidate.source_time_us
        );
    }
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        before,
        "planning must not alter the document"
    );
}

#[test]
fn capacity_is_applied_after_required_instances_and_nearest_time_wins() {
    let mut p = fixture();
    for (id, start) in [(8, 4), (9, 3), (10, 2), (11, 5)] {
        let mut l = p.layers[0].clone();
        l.id = id;
        l.timeline = Some(LayerTimeline {
            in_frame: start,
            out_frame: 10,
            offset_frame: start,
        });
        p.layers.push(l);
    }
    let result = VideoLookahead::default()
        .plan(&p, &p.composition_id, 1., 4, &[10], 2)
        .unwrap();
    assert_eq!(result.iter().map(|c| c.object).collect::<Vec<_>>(), [9, 8]);
    assert!(VideoLookahead::default()
        .plan(&p, &p.composition_id, 1., 4, &[], 0)
        .unwrap()
        .is_empty());
}

#[test]
fn invisible_and_transparent_sources_stay_idle_unless_a_matte_needs_them() {
    let mut p = fixture();
    p.layers[0].visible = false;
    assert!(plan(&p, 4.).is_empty());
    p.layers[0].visible = true;
    p.layers[0].transform.opacity = motion_model::Track::constant(0.);
    assert!(plan(&p, 4.).is_empty());
    let mut consumer = Layer::solid(8, "matte consumer", [16.; 2], [8., 8., 0.], [1.; 4]);
    consumer.track_matte = Some(motion_model::compositing::TrackMatte {
        source: 7,
        mode: motion_model::compositing::MatteMode::Alpha,
        hide_source: true,
    });
    p.layers.push(consumer);
    assert_eq!(plan(&p, 4.)[0].object, 7);
}

#[test]
fn topology_replacement_and_invalid_input_preserve_typed_causes_and_recover() {
    let mut p = fixture();
    nest(&mut p);
    let mut planner = VideoLookahead::default();
    let key = |items: Vec<UpcomingVideo<'_>>| {
        items
            .into_iter()
            .map(|c| (c.object, c.asset.id, c.source_time_us, c.output_steps))
            .collect::<Vec<_>>()
    };
    let original = key(planner.plan(&p, &p.composition_id, 4., 4, &[], 4).unwrap());
    for frame in [f64::NAN, f64::INFINITY, -1., 20.] {
        assert!(planner
            .plan(&p, &p.composition_id, frame, 4, &[], 4)
            .is_err());
    }
    assert!(planner.plan(&p, &p.composition_id, 4., 5, &[], 4).is_err());
    assert!(planner.plan(&p, &p.composition_id, 4., 4, &[], 5).is_err());
    p.video_assets.clear();
    let error = planner
        .plan(&p, &p.composition_id, 4., 4, &[], 4)
        .unwrap_err();
    match error {
        Error::LayerTimeObject {
            object: 7,
            phase: "video_warmup_asset",
            source,
        } => assert!(matches!(*source, Error::Missing(1))),
        other => panic!("lost source identity: {other:?}"),
    }
    p.video_assets = fixture().video_assets;
    assert_eq!(
        key(planner.plan(&p, &p.composition_id, 4., 4, &[], 4).unwrap()),
        original
    );
    p.layers.swap(0, 1);
    let changed = planner.plan(&p, &p.composition_id, 4., 4, &[], 4).unwrap();
    assert_eq!(
        changed,
        plan(&p, 4.),
        "reused registry must match a fresh topology"
    );
    p.compositions[0].layers[0].content = Content::Composition {
        clip: CompositionClip::new("comp-warm-child".into()),
    };
    assert!(
        planner.plan(&p, &p.composition_id, 4., 4, &[], 4).is_err(),
        "cycle must not recurse forever"
    );
}
