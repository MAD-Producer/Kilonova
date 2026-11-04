use motion_core::{Camera, Composition, CompositionClip, Content, FrameRate, Layer, Project, WorkArea};
use motion_render::{composition_plan, effect_plan::PlanBuilder, Scene};

#[path = "rational_frame_clock/gpu.rs"]
mod gpu;

fn clock_project(rate: FrameRate) -> Project {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|effect| effect.id == "wave_warp").unwrap();
    let mut project = Project::new_with_frame_rate(64, 64, rate, 240).unwrap();
    let mut layer = Layer::solid(1, "Local shader clock", [32.;2], [32.,32.,0.], [1.,0.25,0.1,1.]);
    layer.timeline = Some(motion_core::LayerTimeline { in_frame:10,out_frame:120,offset_frame:10 });
    layer.effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
        &package.manifest.version, &package.hash, definition, layer.size));
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

#[test]
fn retained_preview_geometry_distinguishes_rate_denominators_and_refreshes_subframe_clocks() {
    let mut cached = PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut previous_rate = None;
    let mut builds = 0;
    let mut hits = 0;
    let mut project = clock_project(FrameRate::new(30000,1001).unwrap());
    let mut scene = Scene::new(&project).unwrap();
    for ratio in [[30000,1001],[60000,2002],[30000,1003],[24,1],[24000,1001],[60000,1001],[30000,1001]] {
        let rate = FrameRate::new(ratio[0], ratio[1]).unwrap();
        project.fps = rate;
        project.validate().unwrap();
        // Reuse the same project and scene so only the clock identity changes.
        let mut original = None;
        for (index, parent_frame) in [10.125,10.25,55.5,10.125].into_iter().enumerate() {
            scene.sample(&project, parent_frame, None, &motion_core::ExpressionEvaluator).unwrap();
            let plan = cached.build_preview(&scene, &[0], 64, 64).unwrap();
            assert!(plan.diagnostics.is_empty());
            let local_frame = parent_frame - 10.;
            for pass in &plan.passes {
                assert_eq!(pass.uniform.clock[0].to_bits(),
                    ((local_frame * f64::from(ratio[1]) / f64::from(ratio[0])) as f32).to_bits());
                assert_eq!(pass.uniform.clock[1], local_frame as f32);
            }
            let mut actual = vec![0;plan.buffer_bytes(&scene)];
            plan.write(&scene,&mut actual).unwrap();
            let mut fresh = PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
            let reference = fresh.build_preview(&scene,&[0],64,64).unwrap();
            let mut expected = vec![0;reference.buffer_bytes(&scene)];
            reference.write(&scene,&mut expected).unwrap();
            assert_eq!(actual,expected,"rate {rate}, frame {parent_frame}");
            if index == 0 {
                original = Some(actual);
                if previous_rate != Some(rate) { builds += 1; } else { hits += 1; }
            } else {
                if index == 3 { assert_eq!(Some(&actual), original.as_ref()); }
                else { assert_ne!(Some(&actual), original.as_ref()); }
                hits += 1;
            }
            assert_eq!(cached.preview_plan_builds,builds,"rate {rate}, frame {parent_frame}");
            assert_eq!(cached.preview_cache_hits,hits,"rate {rate}, frame {parent_frame}");
        }
        previous_rate = Some(rate);
    }
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn frame(bytes: &[u8], node: usize) -> f64 {
    let offset = composition_plan::HEADER + node * composition_plan::NODE + 56;
    f64::from_bits(u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap()))
}

#[test]
fn current_bundle_keeps_both_rate_components_and_unrounded_nested_frames() {
    let mut project = Project::new_with_frame_rate(32, 24, FrameRate::new(24000, 1001).unwrap(), 240).unwrap();
    project.compositions.push(Composition {
        id: "comp-child".into(), name: "Child".into(), width: 32, height: 24,
        fps: FrameRate::new(60000, 1001).unwrap(), frames: 600,
        work_area: WorkArea::full(600), guides: vec![], background: [0.; 4],
        camera: Camera::new(32, 24), expressions: vec![],
        layers: vec![Layer::solid(7, "Card", [8.; 2], [16., 12., 0.], [1.; 4])],
    });
    let mut reference = Layer::solid(1, "Nested", [32., 24.], [16., 12., 0.], [1.; 4]);
    let mut clip = CompositionClip::new("comp-child".into());
    clip.source_start_frame = 2;
    reference.content = Content::Composition { clip };
    project.layers.push(reference);
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    let mut builder = PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut previous = Vec::new();
    for (parent_frame, child_frame) in [(0.125, 2.3125), (0.25, 2.625), (0.125, 2.3125)] {
        scene.sample(&project, parent_frame, None, &motion_core::ExpressionEvaluator).unwrap();
        let mut bytes = Vec::new();
        composition_plan::build(&mut builder, &scene, &project, &[0], &mut bytes).unwrap();
        assert_eq!(word(&bytes, 0), composition_plan::MAGIC);
        assert_eq!(word(&bytes, 4), 2);
        assert_eq!(word(&bytes, 8), 2);
        assert_eq!(word(&bytes, 16), 1);
        let child = composition_plan::HEADER;
        let parent = child + composition_plan::NODE;
        assert_eq!((word(&bytes, child + 64), word(&bytes, child + 68)), (60000, 1001));
        assert_eq!((word(&bytes, parent + 64), word(&bytes, parent + 68)), (24000, 1001));
        assert_eq!(frame(&bytes, 0), child_frame);
        assert_eq!(frame(&bytes, 1), parent_frame);
        if parent_frame == 0.25 { assert_ne!(bytes, previous); }
        previous = bytes;
    }
}

#[test]
fn native_and_packed_shader_clocks_keep_rational_local_subframes_and_seek_recovery() {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|effect| effect.id == "wave_warp").unwrap();
    for [numerator, denominator] in [[24000,1001],[30000,1001],[60000,1001]] {
        let rate = FrameRate::new(numerator, denominator).unwrap();
        let mut project = Project::new_with_frame_rate(64, 64, rate, 240).unwrap();
        let mut layer = Layer::solid(1, "Local shader clock", [64.;2], [32.,32.,0.], [1.;4]);
        layer.timeline = Some(motion_core::LayerTimeline { in_frame:10,out_frame:120,offset_frame:10 });
        layer.effects.push(motion_core::EffectInstance::new(1, &package.manifest.id,
            &package.manifest.version, &package.hash, definition, [64.;2]));
        project.layers.push(layer);
        project.rebuild_plugin_dependencies();
        project.validate().unwrap();
        let mut scene = Scene::new(&project).unwrap();
        let mut builder = PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
        let mut first = None;
        for parent_frame in [10.125,10.25,55.5,10.125] {
            scene.sample(&project, parent_frame, None, &motion_core::ExpressionEvaluator).unwrap();
            let plan = builder.build(&scene, &[0], 64, 64, true).unwrap();
            assert!(!plan.passes.is_empty());
            let local_frame = parent_frame - 10.;
            let seconds = (local_frame * f64::from(denominator) / f64::from(numerator)) as f32;
            let mut bytes = vec![0; plan.buffer_bytes(&scene)];
            assert_eq!(plan.write(&scene, &mut bytes).unwrap(), bytes.len());
            let pass_table = word(&bytes, 20) as usize;
            for (index, pass) in plan.passes.iter().enumerate() {
                assert_eq!(pass.uniform.clock[0].to_bits(), seconds.to_bits());
                assert_eq!(pass.uniform.clock[1], local_frame as f32);
                let uniform = word(&bytes, pass_table + index * 40 + 24) as usize;
                // Four vec4 fields precede clock in the actual shader uniform.
                for component in 0..4 {
                    assert_eq!(word(&bytes, uniform + 64 + component * 4),
                        pass.uniform.clock[component].to_bits());
                }
            }
            if parent_frame == 10.125 {
                if let Some(first) = &first { assert_eq!(&bytes, first); }
                else { first = Some(bytes); }
            } else {
                assert_ne!(first.as_ref().unwrap(), &bytes);
            }
        }
    }
}
