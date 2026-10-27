#[path = "../../motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;
use motion_core::{
    Composition, CompositionClip, Content, EffectInstance, Layer, LayerTimeline, Project,
    VideoAsset, VideoClip,
};
use motion_render::{composition_plan, effect_plan::PlanBuilder, Renderer, Scene};
use std::collections::HashMap;

fn reference(id: u64, composition: &str, position: [f32; 3], start: i32) -> Layer {
    let mut layer = Layer::solid(id, "authored reference", [32.; 2], position, [1.; 4]);
    let mut clip = CompositionClip::new(composition.into());
    clip.source_start_frame = start;
    layer.content = Content::Composition { clip };
    layer
}
fn fixture() -> Project {
    let mut p = Project::new(96, 32, 30, 60).unwrap();
    p.version = motion_model::CURRENT_PROJECT_VERSION;
    p.background = [0., 0., 0., 1.];
    p.video_assets.push(VideoAsset {
        id: 1,
        path: "assets/authored-video.mp4".into(),
        bytes: 1,
        mime: "video/avc".into(),
        track: 0,
        width: 16,
        height: 16,
        rotation: 0,
        display_width: 16,
        display_height: 16,
        video_start_us: 0,
        video_end_us: 5_000_000,
        duration_us: 5_000_000,
        frame_count: 5,
        variable_frame_rate: false,
        nominal_frame_rate: 1.,
        color_standard: 1,
        color_range: 2,
        audio_asset: None,
    });
    for (id, x, start, range) in [
        (11, 16., 0, (10, 30)),
        (22, 48., 30, (0, 60)),
        (33, 80., 60, (20, 50)),
    ] {
        let mut layer = reference(id, "comp-wrapper", [x, 16., 0.], start);
        layer.timeline = Some(LayerTimeline {
            in_frame: range.0,
            out_frame: range.1,
            offset_frame: 0,
        });
        p.layers.push(layer);
    }
    let mut video = Layer::solid(1, "authored video", [32.; 2], [16., 16., 0.], [1.; 4]);
    video.content = Content::Video {
        video: VideoClip::new(1),
    };
    video.timeline = Some(LayerTimeline::full(150));
    let package = motion_effects::builtin::package().unwrap();
    let definition = package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "brightness_contrast")
        .unwrap();
    video.effects.push(EffectInstance::new(
        1,
        &package.manifest.id,
        &package.manifest.version,
        &package.hash,
        definition,
        [32.; 2],
    ));
    for (id, layers) in [
        (
            "comp-wrapper",
            vec![reference(1, "comp-video", [16., 16., 0.], 0)],
        ),
        ("comp-video", vec![video]),
    ] {
        p.compositions.push(Composition {
            id: id.into(),
            name: "Authored child".into(),
            width: 32,
            height: 32,
            fps: 30,
            frames: 180,
            work_area: motion_core::WorkArea::full(180),
            guides: Vec::new(),
            background: [0.; 4],
            camera: motion_core::Camera::new(32, 32),
            layers,
            expressions: vec![],
        });
    }
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    p
}
fn nested_video(scene: &Scene, reference: u64) -> &Scene {
    &scene
        .nested
        .iter()
        .find(|n| n.layer == reference)
        .unwrap()
        .scene
        .nested[0]
        .scene
}
fn sample(scene: &mut Scene, p: &Project, frame: f64) {
    scene
        .sample(p, frame, None, &motion_core::ExpressionEvaluator)
        .unwrap();
}

#[test]
fn authored_reference_paths_keep_ids_across_clip_boundaries_reentry_and_reopen() {
    let p = fixture();
    let mut reused = Scene::new(&p).unwrap();
    let mut known = HashMap::new();
    let reopened: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    reopened.validate().unwrap();
    let mut builder =
        PlanBuilder::new(scene_fixtures::registry().unwrap()).unwrap();
    for frame in [0., 9.9, 10., 10.1, 19.9, 20., 20.1, 29.9, 30., 50., 0., 20.] {
        sample(&mut reused, &p, frame);
        let mut fresh = Scene::new(&reopened).unwrap();
        sample(&mut fresh, &reopened, frame);
        for node in &reused.nested {
            let child = nested_video(&reused, node.layer);
            let alias = child.layers[0].id;
            assert_eq!(child.source_object(alias), 1);
            assert_eq!(child.effects[0].layer, alias);
            assert!(child.world_matrix(alias).is_some());
            assert_eq!(alias, nested_video(&fresh, node.layer).layers[0].id);
            if let Some(previous) = known.insert(node.layer, alias) {
                assert_eq!(
                    alias, previous,
                    "reference {} changed at frame {frame}",
                    node.layer
                );
            }
        }
        assert_eq!(
            known
                .values()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            known.len()
        );
        let mut a = vec![];
        composition_plan::build(&mut builder, &reused, &p, &[0], &mut a).unwrap();
        let mut b = vec![];
        composition_plan::build(&mut builder, &fresh, &reopened, &[0], &mut b).unwrap();
        assert_eq!(a, b, "fresh/reopened export plan changed at frame {frame}");
    }
    assert_eq!(known.len(), 3);
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn clip_entry_does_not_reupload_a_continuously_visible_decoded_source_or_change_its_pixels() {
    let p = fixture();
    let mut scene = Scene::new(&p).unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(96, 32).unwrap();
    let mut source_pts = HashMap::new();
    let mut expected_uploads = 0;
    let mut stable_alias = None;
    let mut peak = 0;
    let mut sample_us = Vec::new();
    for _ in 0..3 {
        for frame in [9.9, 10., 10.1, 19.9, 20., 20.1, 29.9, 30., 30.1, 9.9] {
            let started = std::time::Instant::now();
            sample(&mut scene, &p, frame);
            sample_us.push(started.elapsed().as_micros());
            renderer.retain_video_instances(&scene);
            source_pts.retain(|reference, _| scene.nested.iter().any(|n| n.layer == *reference));
            let mut expected = HashMap::new();
            for node in &scene.nested {
                let video = &nested_video(&scene, node.layer).layers[0];
                if node.layer == 22 {
                    if let Some(alias) = stable_alias {
                        assert_eq!(video.id, alias);
                    } else {
                        stable_alias = Some(video.id);
                    }
                }
                let source = video.video.as_ref().unwrap();
                let pts = source.source_time_us / 1_000_000 * 1_000_000;
                let color = match pts / 1_000_000 {
                    0 => [255, 0, 0, 255],
                    1 => [0, 255, 0, 255],
                    2 => [0, 0, 255, 255],
                    _ => [255; 4],
                };
                if source_pts.insert(node.layer, pts) != Some(pts) {
                    expected_uploads += 1;
                }
                expected.insert(node.layer, color);
                renderer
                    .upload_video_frame(video.id, 1, pts, 16, 16, &color.repeat(256))
                    .unwrap();
            }
            assert_eq!(
                renderer.video_uploads, expected_uploads,
                "clip entry recreated a resident source at {frame}"
            );
            let pixels = renderer.capture(&scene, &target).unwrap().0;
            for (reference, x) in [(11, 16), (22, 48), (33, 80)] {
                let expected = expected.get(&reference).copied().unwrap_or([0, 0, 0, 255]);
                let actual = &pixels[(16 * 96 + x) * 4..(16 * 96 + x) * 4 + 4];
                assert!(
                    actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                    "frame={frame} reference={reference}: {actual:?} expected {expected:?}"
                );
            }
            peak = peak.max(renderer.texture_bytes());
            assert!(
                peak < 1024 * 1024,
                "small nested fixture exceeded its measured capacity envelope"
            );
        }
    }
    renderer.check_health().unwrap();
    assert_eq!(expected_uploads, 16);
    if let Some(folder) = std::env::var_os("MOTION_SAMPLING_ARTIFACT_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("instance-alias-cost.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "samples":sample_us.len(),"scene_sample_us":sample_us,"video_uploads":renderer.video_uploads,
            "video_upload_bytes":renderer.video_upload_bytes,"peak_renderer_texture_bytes":peak,
            "scope":"Three cycles across nested clip boundaries using explicit decoded 1fps source frames and real GPU captures. Upload count includes required newly-visible and changed-PTS sources; continuously-visible unchanged PTS is reused. Sampling time is CPU Scene evaluation, not total playback time or decoder latency. No media file decoding or UI acceptance is implied."
        })).unwrap()).unwrap();
    }
}

#[test]
fn nested_aliases_remap_parent_matte_effect_image_and_scene_source_together() {
    use motion_core::{
        compositing::{MatteMode, TrackMatte},
        EffectImageInput, EffectImageStage, ParentLink,
    };
    let mut p = fixture();
    let child = p
        .compositions
        .iter_mut()
        .find(|c| c.id == "comp-video")
        .unwrap();
    let mut parent = Layer::solid(7, "parent", [1.; 2], [16., 16., 0.], [0.; 4]);
    parent.content = Content::Null;
    let mut matte = Layer::solid(2, "matte", [32.; 2], [16., 16., 0.], [1.; 4]);
    matte.visible = false;
    let mut input = Layer::solid(3, "map", [32.; 2], [16., 16., 0.], [0.5; 4]);
    input.visible = false;
    child.layers[0].parent = Some(ParentLink {
        object: Some(7),
        bind: glam::Mat4::IDENTITY.to_cols_array_2d(),
    });
    child.layers[0].track_matte = Some(TrackMatte {
        source: 2,
        mode: MatteMode::Alpha,
        hide_source: true,
    });
    let image_package = motion_effects::builtin::package().unwrap();
    let image = image_package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "displacement_map")
        .unwrap();
    let mut effect = EffectInstance::new(
        2,
        &image_package.manifest.id,
        &image_package.manifest.version,
        &image_package.hash,
        image,
        [32.; 2],
    );
    effect.image_input = Some(EffectImageInput::Layer {
        layer: 3,
        stage: EffectImageStage::Source,
    });
    child.layers[0].effects.push(effect);
    let scene_package = scene_fixtures::scene_package().unwrap();
    let scene_definition = scene_package
        .manifest
        .effects
        .iter()
        .find(|d| d.id == "lens_flare")
        .unwrap();
    let mut generator = Layer::solid(4, "generator", [32.; 2], [16., 16., 0.], [1.; 4]);
    let mut effect = EffectInstance::new(
        1,
        &scene_package.manifest.id,
        &scene_package.manifest.version,
        &scene_package.hash,
        scene_definition,
        [32.; 2],
    );
    effect.scene.as_mut().unwrap().source_layer = Some(7);
    generator.effects.push(effect);
    child.layers.extend([matte, input, parent, generator]);
    p.rebuild_plugin_dependencies();
    p.validate().unwrap();
    let mut scene = Scene::new(&p).unwrap();
    for frame in [0., 10., 20., 30., 0.] {
        sample(&mut scene, &p, frame);
        for reference in &scene.nested {
            let child = nested_video(&scene, reference.layer);
            let alias = |source| {
                child
                    .layers
                    .iter()
                    .find(|l| child.source_object(l.id) == source)
                    .unwrap()
                    .id
            };
            let video = child
                .layers
                .iter()
                .find(|l| child.source_object(l.id) == 1)
                .unwrap();
            assert_eq!(video.track_matte.unwrap().source, alias(2));
            let image = child
                .effects
                .iter()
                .find(|e| e.layer == video.id && e.instance == 2)
                .unwrap();
            assert_eq!(
                image.image_input.and_then(EffectImageInput::layer),
                Some(alias(3))
            );
            let generator = child.effects.iter().find(|e| e.layer == alias(4)).unwrap();
            let parent = generator.scene.as_ref().unwrap().source_layer.unwrap();
            assert_eq!(child.source_object(parent), 7);
            assert!(child.world_matrix(parent).is_some());
            assert!(child.world_matrix(video.id).is_some());
        }
    }
}
