use super::*;
use bytemuck::Zeroable;

fn fixture() -> PlanBuilder {
    let mut builder = PlanBuilder::new(Registry::default()).unwrap();
    let pass = |output, input| EffectPass {
        instance_data: 0, resource_input: 0, program: 0, input, source: input,
        output, width: 16, height: 16, lut: -1, uniform: EffectUniform::zeroed(),
        sprite: false, sprite_start: 0, sprite_count: 0,
    };
    // The first value is dead, even though it shares semantic and physical
    // storage with the second. The final value is read outside the effect list.
    builder.frame.passes = vec![pass(0,-1), pass(0,-1), pass(1,0)];
    builder.frame.draws = vec![PlannedDraw {
        layer: 1, words: [0.; DRAW_WORDS], pass_start: 0, pass_end: 3,
    }];
    let layout = scratch::layout(&builder.frame, None).unwrap();
    builder.frame.scratch_allocations = layout.allocations;
    builder.frame.scratch_bindings = layout.bindings;
    builder.frame.scratch_outputs = layout.outputs;
    builder
}

#[test]
fn logical_consumers_distinguish_dead_reused_values_double_ports_and_exports() {
    let builder = fixture();
    let reports = builder.attachment_intents().unwrap();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0].allocation, reports[1].allocation);
    assert_ne!(reports[0].logical_value, reports[1].logical_value);
    assert_eq!(reports[0].consumers.sampled_by_passes, 0);
    assert!(!reports[0].store_required);
    assert_eq!(reports[1].consumers.sampled_by_passes, 1, "two ports are one pass consumer");
    assert_eq!(reports[1].consumers.last_sampled_pass, Some(2));
    assert!(reports[1].store_required);
    assert_eq!(reports[2].consumers.sampled_by_passes, 0);
    assert_eq!(reports[2].consumers.snapshot_or_composition, 1);
    assert!(reports[2].store_required, "a terminal composition/snapshot is a real reader");
    assert!(reports.iter().all(|report| report.full_overwrite_candidate));
    assert!(reports.iter().all(|report| report.clears_transparent && report.stores));
}

#[test]
fn capacity_discard_unknown_proof_and_zero_instance_blending_still_require_clear() {
    let mut builder = fixture();
    let allocation = builder.frame.scratch_bindings[0].output as usize;
    builder.frame.scratch_allocations[allocation].width += 1;
    let reports = builder.attachment_intents().unwrap();
    assert!(reports[0].clear_reasons.unused_capacity);
    assert!(!reports[0].full_overwrite_candidate);
    assert!(reports[0].capacity[0] > reports[0].viewport[0]);
    builder.frame.scratch_allocations[allocation].width -= 1;
    builder.programs[0].shader = Arc::new(shader::compile(
        "fn main_fx(p:vec2<f32>)->vec4<f32>{if p.x>0.0{discard;}return vec4(1.0);}", "main_fx").unwrap());
    assert!(builder.attachment_intents().unwrap()[0].clear_reasons.fragment_discard);
    Arc::make_mut(&mut builder.programs[0].shader).may_discard = None;
    assert!(builder.attachment_intents().unwrap()[0].clear_reasons.unknown_shader_coverage);
    Arc::make_mut(&mut builder.programs[0].shader).may_discard = Some(false);
    builder.frame.passes[0].sprite = true;
    let report = builder.attachment_intents().unwrap()[0];
    assert!(report.clear_reasons.partial_geometry && report.clear_reasons.destination_blending);
    assert!(!report.full_overwrite_candidate);
    assert!(report.clears_transparent && report.stores);
}

#[test]
fn malformed_diagnostics_keep_pass_program_and_capacity_causes_without_panicking() {
    let mut builder = fixture();
    builder.frame.passes[0].program = u32::MAX;
    let error = builder.attachment_intents().unwrap_err().to_string();
    assert!(error.contains("pass=0 program=4294967295 phase=shader"), "{error}");
    builder.frame.passes[0].program = 0;
    builder.frame.scratch_bindings[0].output = u32::MAX;
    let error = builder.attachment_intents().unwrap_err().to_string();
    assert!(error.contains("pass=0 allocation=4294967295 phase=capacity"), "{error}");
    builder.frame.scratch_bindings.pop();
    assert!(builder.attachment_intents().unwrap_err().to_string().contains("phase=bindings"));
    let mut builder = fixture();
    let allocation = builder.frame.scratch_bindings[0].output as usize;
    builder.frame.scratch_allocations[allocation].height = 8;
    assert!(builder.attachment_intents().unwrap_err().to_string().contains("viewport 16x16"));
}

#[test]
#[cfg_attr(not(feature="gpu-tests"), ignore="requires actual GPU")]
fn native_reports_preserve_clear_store_transparency_and_random_seek_pixels() {
    use motion_core::{EffectInstance, ExpressionEvaluator, Layer, Project};
    let package = motion_effects::builtin::package().unwrap();
    let mut project = Project::new(32, 32, 30, 60).unwrap();
    project.background = [0.;4];
    let mut layer = Layer::solid(1,"Attachment probe",[12.;2],[16.,16.,0.],[1.,0.2,0.4,0.75]);
    for (index, name) in ["gaussian_blur","tint"].into_iter().enumerate() {
        let definition=package.manifest.effects.iter().find(|item|item.id==name).unwrap();
        layer.effects.push(EffectInstance::new(index as u64+1,&package.manifest.id,
            &package.manifest.version,&package.hash,definition,layer.size));
    }
    layer.transform.position.upsert(0,[16.,16.,0.],motion_core::Ease::Linear).unwrap();
    layer.transform.position.upsert(20,[4.,4.,0.],motion_core::Ease::Linear).unwrap();
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    let mut renderer = pollster::block_on(crate::Renderer::headless()).unwrap();
    let mut first = None;
    let mut evidence = Vec::new();
    for (frame, size) in [(0.,32),(20.,16),(7.,32),(0.,32)] {
        scene.sample(&project,frame,None,&ExpressionEvaluator).unwrap();
        let target = renderer.capture_target(size,size).unwrap();
        let pixels = renderer.capture(&scene,&target).unwrap().0;
        let reports = renderer.effect_attachment_intents().unwrap();
        assert!(!reports.is_empty());
        assert!(reports.iter().all(|report|report.clears_transparent && report.stores));
        assert!(reports.iter().any(|report|report.consumers.snapshot_or_composition>0));
        assert!(reports.iter().all(|report|report.capacity[0]>=report.viewport[0]
            && report.capacity[1]>=report.viewport[1]));
        use sha2::Digest;
        evidence.push(serde_json::json!({"frame":frame,"size":size,"attachments":reports,
            "rgbaSha256":format!("{:x}",sha2::Sha256::digest(&pixels))}));
        if frame==0. {
            assert_eq!(pixels[3],0,"transparent corner retained a previous value");
            assert!(pixels.chunks_exact(4).any(|pixel|pixel[3]>0));
            if let Some(expected)=&first { assert_eq!(&pixels,expected); }
            else { first=Some(pixels); }
        }
    }
    renderer.check_health().unwrap();
    println!("ATTACHMENT_INTENTS {}", serde_json::json!({
        "adapter":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend),
        "steps":evidence,"operations":"clear-transparent/store","performanceQualified":false,
    }));
}
