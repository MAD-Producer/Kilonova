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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
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
    assert_ledger(&renderer);
    let last = renderer.texture_memory_breakdown();
    assert_eq!(last.mask_outputs, 0);
    assert_eq!(last.effect_input_snapshots, 0);
    assert_eq!(last.total_bytes(), renderer.texture_bytes() + 1028);
}

fn assert_ledger(renderer: &Renderer) {
    let ledger = renderer.texture_resource_ledger();
    assert_eq!(ledger.unknown_capacity_count(), 0);
    assert_eq!(ledger.total_known_bytes(), renderer.texture_memory_breakdown().total_bytes());
    assert_eq!(ledger, renderer.texture_resource_ledger());
    let ids = ledger.resources().map(|r| &r.id).collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), ledger.resources().count());
}

#[test]
fn resource_identity_survives_reuse_but_not_replacement_or_project_switch() {
    use motion_render::TextureCategory;
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let initial = renderer.texture_resource_ledger();
    renderer.upload_image(7, 2, 3, &[255; 24]).unwrap();
    let uploaded = renderer.texture_resource_ledger();
    let id = uploaded.resources().find(|r| r.references[0].category == TextureCategory::StaticImages).unwrap().id.clone();
    // An upload into the same allocation keeps the ID.
    renderer.upload_image(7, 2, 3, &[128; 24]).unwrap();
    assert_eq!(uploaded, renderer.texture_resource_ledger());
    // Replacing dimensions allocates a distinct texture even with the same asset ID.
    renderer.upload_image(7, 3, 3, &[255; 36]).unwrap();
    let replaced = renderer.texture_resource_ledger();
    assert!(replaced.resources().all(|r| r.id != id));
    let before_failure = replaced.clone();
    assert!(renderer.upload_image(8, 0, 0, &[]).is_err());
    assert_eq!(before_failure, renderer.texture_resource_ledger());
    renderer.clear_assets();
    assert_eq!(initial, renderer.texture_resource_ledger());
    // A retained metadata snapshot does not repopulate the renderer or its accounting.
    assert!(uploaded.total_known_bytes() > initial.total_known_bytes());
    assert_ledger(&renderer);
}

#[test]
fn preview_candidates_swap_ownership_and_aliases_count_once() {
    use motion_render::{PreviewFrameTargets, TextureState};
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let mut targets = PreviewFrameTargets::new(&renderer, 32, 32).unwrap();
    let snapshot = |targets: &PreviewFrameTargets| {
        let mut ledger = renderer.texture_resource_ledger();
        targets.collect_texture_resources(&mut ledger);
        ledger
    };
    let initial = snapshot(&targets);
    targets.prepare(&renderer, 16, 16).unwrap();
    let prepared = snapshot(&targets);
    let candidate = prepared.resources().find(|r| r.references[0].state == TextureState::PreviewCandidate).unwrap().id.clone();
    let mut alias = prepared.clone();
    let target = targets.prepared().unwrap();
    target.collect_texture_resources(&mut alias, "presenter/borrowed", TextureState::Retained);
    target.collect_texture_resources(&mut alias, "presenter/borrowed", TextureState::Retained);
    assert_eq!(alias.total_known_bytes(), prepared.total_known_bytes());
    assert_eq!(alias.resources().find(|r| r.id == candidate).unwrap().references.len(), 2);
    let encoder = renderer.device.create_command_encoder(&Default::default());
    renderer.queue.submit(Some(encoder.finish()));
    targets.commit_submitted();
    let committed = snapshot(&targets);
    assert_eq!(prepared.total_known_bytes(), committed.total_known_bytes());
    assert_eq!(committed.resources().find(|r| r.id == candidate).unwrap().references[0].state, TextureState::PreviewFront);
    // Dropping a failed candidate leaves the last front and changes only owned capacity.
    targets.discard_candidate();
    let discarded = snapshot(&targets);
    assert_eq!(discarded.total_known_bytes(), initial.total_known_bytes() - 32*32*4 + 16*16*4);
    assert!(targets.prepare(&renderer, u32::MAX, u32::MAX).is_err());
    assert_eq!(discarded, snapshot(&targets));
    renderer.device.poll(wgpu::PollType::wait_indefinitely()).expect("GPU poll failed");
    renderer.check_health().unwrap();
}
