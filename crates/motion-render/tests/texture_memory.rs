use motion_core::{
    vector::{ShapeKind, VectorContent},
    Content, EffectInstance, Engine, Layer, Project,
};
use motion_render::{ChromaLayout, Renderer, Scene, Yuv420Frame};

fn render(renderer: &mut Renderer, project: &Project) {
    let mut scene = Scene::new(project);
    scene
        .sample(project, 0., None, &motion_core::ExpressionEvaluator)
        .unwrap();
    renderer.retain_video_instances(&scene);
    let target = renderer.capture_target(64, 64).unwrap();
    renderer.capture(&scene, &target).unwrap();
}
fn effect(id: &str) -> EffectInstance {
    let p = motion_effects::builtin::package().unwrap();
    let d = p.manifest.effects.iter().find(|d| d.id == id).unwrap();
    EffectInstance::new(1, &p.manifest.id, &p.manifest.version, &p.hash, d, [20.; 2])
}

#[test]
fn owned_images_planes_and_fallbacks_are_disjoint_and_external_targets_are_excluded() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let before = renderer.texture_memory_breakdown();
    assert_eq!(before.system_textures, 4 + 4 + 1024);
    assert_eq!(before.total_bytes(), 1032);
    let _external_target = renderer.render_target(64, 64).unwrap();
    assert_eq!(before, renderer.texture_memory_breakdown());
    renderer.upload_image(7, 2, 3, &[255; 24]).unwrap();
    renderer
        .upload_video_frame(8, 1, 0, 2, 2, &[255; 16])
        .unwrap();
    let yuv = Yuv420Frame {
        width: 4,
        height: 2,
        rotation: 0,
        standard: 1,
        range: 2,
        phase: [0, 0],
        chroma_layout: ChromaLayout::Uv,
        y: vec![128; 8],
        uv: vec![128; 4],
    };
    renderer.upload_video_yuv(9, 1, 0, &yuv).unwrap();
    let memory = renderer.texture_memory_breakdown();
    assert_eq!(memory.static_images, 24);
    assert_eq!(memory.video_rgba, 16 + 32);
    assert_eq!(memory.video_planes, 8 + 4);
    assert_eq!(memory.total_bytes(), 1116);
    assert_eq!(memory.total_bytes(), renderer.texture_bytes() + 1028);
    renderer.clear_assets();
    assert_eq!(renderer.texture_memory_breakdown(), before);
    assert_eq!(renderer.texture_bytes(), 4);
}

#[test]
fn grouped_vectors_share_scratch_while_sources_and_curve_luts_keep_their_owners() {
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut vector = motion_core::vector::groups::VectorGroup::wrap(
        VectorContent::shape(ShapeKind::Rectangle),
        [20.; 2],
    );
    let motion_core::vector::VectorSource::Group { group } = &mut vector.source else {
        unreachable!()
    };
    group.transform.opacity.value = 50.;
    let mut layer = Layer::solid(1, "vector", [20.; 2], [32., 32., 0.], [1.; 4]);
    layer.content = Content::Vector { vector };
    layer.effects.push(effect("curves"));
    project.layers.push(layer.clone());
    project.rebuild_plugin_dependencies();
    render(&mut renderer, &project);
    let first = renderer.texture_memory_breakdown();
    assert!(first.vector_sources > 0 && first.vector_group_scratch > 0);
    assert_eq!(first.curve_luts, 1024);
    assert!(first.effect_scratch > 0);
    assert_eq!(first.total_bytes(), renderer.texture_bytes() + 1028);
    layer.id = 2;
    layer.effects.clear();
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    render(&mut renderer, &project);
    let second = renderer.texture_memory_breakdown();
    assert_eq!(second.vector_sources, first.vector_sources * 2);
    assert_eq!(second.vector_group_scratch, first.vector_group_scratch);
    assert_eq!(second.curve_luts, 1024);
    render(&mut renderer, &project);
    assert_eq!(second, renderer.texture_memory_breakdown());
}

#[test]
fn nested_composition_outputs_are_not_reported_as_video_rgba() {
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    project
        .layers
        .push(Layer::solid(1, "solid", [20.; 2], [32., 32., 0.], [1.; 4]));
    let mut engine = Engine::new(project).unwrap();
    engine
        .apply_batch(
            motion_core::parse_commands(
                &serde_json::json!({"op":"composition",
        "composition":motion_core::MAIN_COMPOSITION,
        "action":{"kind":"precompose","objects":[1],"name":"child"}})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    render(&mut renderer, engine.project());
    let memory = renderer.texture_memory_breakdown();
    assert_eq!(memory.nested_compositions, 64 * 64 * 4);
    assert_eq!(memory.video_rgba, 0);
    assert_eq!(memory.video_planes, 0);
    assert_eq!(memory.total_bytes(), renderer.texture_bytes() + 1028);
}

#[test]
fn masks_and_shared_effect_input_snapshots_have_distinct_ownership() {
    use motion_core::{
        masks::LayerMask,
        vector::{PathNode, VectorPath},
        EffectImageInput, EffectImageStage, Track,
    };
    let mut project = Project::new(64, 64, 30, 60).unwrap();
    let mut target = Layer::solid(1, "target", [20.; 2], [32., 32., 0.], [1.; 4]);
    target.masks.push(LayerMask::new(
        1,
        VectorPath {
            id: 1,
            closed: true,
            nodes: [[5., 5.], [15., 5.], [15., 15.], [5., 15.]]
                .into_iter()
                .enumerate()
                .map(|(i, p)| PathNode {
                    id: i as u64 + 1,
                    geometry: Track::constant([p[0], p[1], 0., 0., 0., 0.]),
                })
                .collect(),
        },
    ));
    let mut displacement = effect("displacement_map");
    displacement.image_input = Some(EffectImageInput::Layer {
        layer: 2,
        stage: EffectImageStage::Effects,
    });
    target.effects.push(displacement);
    let mut source = Layer::solid(2, "source", [20.; 2], [32., 32., 0.], [0.3, 0.5, 0.8, 1.]);
    source.effects.push(effect("tint"));
    project.layers = vec![target.clone(), source];
    project.rebuild_plugin_dependencies();
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    render(&mut renderer, &project);
    let first = renderer.texture_memory_breakdown();
    assert!(first.mask_outputs > 0 && first.mask_scratch > 0);
    assert!(first.effect_input_snapshots > 0);
    assert_eq!(first.total_bytes(), renderer.texture_bytes() + 1028);
    target.id = 3;
    target.masks.clear();
    project.layers.push(target);
    project.rebuild_plugin_dependencies();
    render(&mut renderer, &project);
    assert_eq!(
        first.effect_input_snapshots,
        renderer.texture_memory_breakdown().effect_input_snapshots
    );
    project.layers[0].masks.clear();
    for index in [0, 2] {
        project.layers[index].effects[0].image_input = Some(EffectImageInput::Layer {
            layer: 2,
            stage: EffectImageStage::Source,
        });
    }
    render(&mut renderer, &project);
    let last = renderer.texture_memory_breakdown();
    assert_eq!(last.mask_outputs, 0);
    assert_eq!(last.effect_input_snapshots, 0);
    assert_eq!(last.total_bytes(), renderer.texture_bytes() + 1028);
}
