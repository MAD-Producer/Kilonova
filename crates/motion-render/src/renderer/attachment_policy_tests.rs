//! Counterfactual operations run through the real native encode_pass. Poison
//! passes and overrides are absent from non-test builds, never a product flag.
use super::*;
use crate::effect_gpu::AttachmentTestPolicy;
use crate::effect_plan::EffectAttachmentIntent;
use motion_core::{EffectInstance, ExpressionEvaluator, Layer};
use serde_json::json;
use sha2::{Digest, Sha256};

#[path = "../../../motion-effects/tests/support/scene_packages.rs"]
mod scene_fixtures;

fn discard_package() -> Arc<motion_effects::EffectPackage> {
    static PACKAGE: std::sync::OnceLock<Arc<motion_effects::EffectPackage>> = std::sync::OnceLock::new();
    PACKAGE.get_or_init(|| {
        let directory = tempfile::tempdir().unwrap();
        let mut manifest = motion_effects::builtin::manifest();
        let mut definition = manifest.effects.iter().find(|effect|effect.id=="tint").unwrap().clone();
        manifest.id = "com.motionstudio.tests.attachment-discard".into();
        manifest.name = "Independently authored attachment discard fixture".into();
        definition.id = "cut".into();
        definition.passes = vec![motion_effects::PassDefinition {
            shader: "shaders/cut.wgsl".into(), entry: "main_fx".into(),
        }];
        manifest.effects = vec![definition];
        std::fs::write(directory.path().join("manifest.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
        std::fs::create_dir(directory.path().join("shaders")).unwrap();
        std::fs::write(directory.path().join("shaders/cut.wgsl"),
            "fn main_fx(p:vec2<f32>)->vec4<f32>{if p.x<18.0 {discard;}return sample_input(p);}").unwrap();
        Arc::new(motion_effects::EffectPackage::from_bytes(
            motion_effects::package_directory(directory.path()).unwrap()).unwrap())
    }).clone()
}

fn add_effect(layer: &mut Layer, name: &str) {
    let package = motion_effects::builtin::package().unwrap();
    let definition = package.manifest.effects.iter().find(|effect| effect.id == name).unwrap();
    layer.effects.push(EffectInstance::new(layer.effects.len() as u64 + 1,
        &package.manifest.id, &package.manifest.version, &package.hash, definition, layer.size));
}

fn project(case: &str) -> Project {
    let mut project = Project::new(96, 64, 30, 120).unwrap();
    project.background = [0.; 4];
    let mut layer = Layer::solid(1, "Attachment counterfactual", [36., 24.],
        [48., 32., 0.], [0.9, 0.2, 0.4, 0.6]);
    add_effect(&mut layer, "gaussian_blur");
    add_effect(&mut layer, "tint");
    layer.transform.position.upsert(0, [48.,32.,0.], motion_core::Ease::Linear).unwrap();
    layer.transform.position.upsert(20, [24.,16.,0.], motion_core::Ease::Linear).unwrap();
    project.layers.push(layer);
    match case {
        "plain" => {},
        "discard" => {
            let package = discard_package();
            let layer = &mut project.layers[0];
            layer.effects.push(EffectInstance::new(3,&package.manifest.id,&package.manifest.version,
                &package.hash,&package.manifest.effects[0],layer.size));
        },
        "masked" => {
            use motion_core::{masks::LayerMask,vector::{VectorPath,PathNode},Track};
            project.layers[0].masks.push(LayerMask::new(1,VectorPath {
                id: 1, closed: true, tangents: Default::default(),
                nodes: [[2.,2.],[26.,2.],[26.,19.],[2.,19.]].into_iter().enumerate()
                    .map(|(index,point)|PathNode {id:index as u64+1,
                        geometry:Track::constant([point[0],point[1],0.,0.,0.,0.])}).collect(),
            }));
        },
        "matte" => {
            project.layers.push(Layer::solid(2,"Independent matte",[19.,43.],
                [47.,33.,0.],[1.,1.,1.,0.5]));
            project.layers[0].track_matte = Some(motion_core::compositing::TrackMatte {
                source: 2, mode: motion_core::compositing::MatteMode::Alpha, hide_source: true,
            });
        },
        "blend" => {
            project.layers.push(Layer::solid(2,"Blend backing",[71.,45.],
                [44.,29.,0.],[0.2,0.7,0.3,0.7]));
            project.layers[0].blend.mode = motion_core::compositing::BlendMode::Multiply;
        },
        "adjustment" => {
            let mut adjustment = Layer::solid(2,"Adjustment accumulator",[96.,64.],
                [48.,32.,0.],[0.;4]);
            adjustment.content = motion_core::Content::Adjustment;
            add_effect(&mut adjustment,"tint");
            project.layers.push(adjustment);
        },
        "sprite" | "culled_sprite" => {
            let package = scene_fixtures::scene_package().unwrap();
            let definition = package.manifest.effects.iter().find(|effect| effect.id == "starfield").unwrap();
            let mut layer = Layer::solid(2,"Archived SDK sprite fixture",[96.,64.],
                [48.,32.,0.],[0.;4]);
            let mut effect = EffectInstance::new(1,&package.manifest.id,&package.manifest.version,
                &package.hash,definition,layer.size);
            effect.params.get_mut("extent").unwrap().track.value = [128.,128.,0.,0.];
            effect.params.get_mut("fade").unwrap().track.value = [0.;4];
            layer.effects.push(effect);
            if case == "culled_sprite" { layer.transform.position.value[0] = 100000.; }
            project.layers.push(layer);
        },
        _ => panic!("unknown independently authored fixture"),
    }
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    project
}

#[derive(Clone, Copy, Debug)]
enum Policy { Clear, ProvenLoad, AllLoad, AllDiscard }

fn control(reports: &[EffectAttachmentIntent], policy: Policy) -> AttachmentTestPolicy {
    AttachmentTestPolicy {
        operations: reports.iter().map(|report| wgpu::Operations {
            load: if matches!(policy, Policy::AllLoad)
                || (matches!(policy, Policy::ProvenLoad) && report.full_overwrite_candidate) {
                wgpu::LoadOp::Load
            } else { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) },
            store: if matches!(policy, Policy::AllDiscard) { wgpu::StoreOp::Discard }
                else { wgpu::StoreOp::Store },
        }).collect(),
        encoded: Vec::new(), poison: true,
    }
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_attachment_candidate_and_wrong_operations_are_checked_with_old_pixel_poison() {
    let mut evidence = Vec::new();
    let mut negative_loads = 0;
    let mut negative_discards = 0;
    let mut padded = 0;
    let mut sprites = 0;
    let mut discards = 0;
    for case in ["plain","discard","masked","matte","blend","adjustment","sprite","culled_sprite"] {
        let project = project(case);
        let mut scene = Scene::new(&project).unwrap();
        let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
        let mut registry = scene_fixtures::registry().unwrap();
        registry.insert(discard_package()).unwrap();
        renderer.set_effect_registry(registry);
        for (frame,size) in [(0.,[96,64]),(20.,[48,32]),(7.,[96,64]),(0.,[96,64])] {
            scene.sample(&project,frame,None,&ExpressionEvaluator).unwrap();
            renderer.effect_gpu.state.attachment_test_policy = None;
            let target = renderer.capture_target(size[0],size[1]).unwrap();
            let expected = renderer.capture(&scene,&target).unwrap().0;
            let reports = renderer.effect_attachment_intents().unwrap();
            assert!(!reports.is_empty());
            assert!(reports.iter().any(|report|report.full_overwrite_candidate));
            padded += reports.iter().filter(|report|report.clear_reasons.unused_capacity).count();
            sprites += reports.iter().filter(|report|report.clear_reasons.partial_geometry).count();
            discards += reports.iter().filter(|report|report.clear_reasons.fragment_discard).count();
            let mut outcomes = Vec::new();
            let mut clear_execution = None;
            for policy in [Policy::Clear,Policy::ProvenLoad,Policy::AllLoad,Policy::AllDiscard] {
                let experiment = control(&reports,policy);
                let load_count = experiment.operations.iter().filter(|ops|matches!(ops.load,wgpu::LoadOp::Load)).count();
                renderer.effect_gpu.state.attachment_test_policy = Some(experiment);
                let actual = renderer.capture(&scene,&target).unwrap().0;
                let encoded = &renderer.effect_gpu.state.attachment_test_policy.as_ref().unwrap().encoded;
                // Track mattes snapshot their source before stack-order draws.
                // Require the actual Clear schedule, not numeric pass order;
                // independently require exactly one encode of every fixture pass.
                let mut coverage = encoded.clone();
                coverage.sort_unstable();
                assert_eq!(coverage, (0..reports.len()).collect::<Vec<_>>(), "case={case} frame={frame} policy={policy:?}");
                if let Some(expected_execution) = &clear_execution {
                    assert_eq!(encoded, expected_execution, "policy changed native scheduling");
                } else { clear_execution = Some(encoded.clone()); }
                let difference = actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
                match policy {
                    Policy::Clear | Policy::ProvenLoad => assert_eq!(actual,expected,"case={case} frame={frame} policy={policy:?}"),
                    Policy::AllLoad => { negative_loads += usize::from(difference>0); },
                    Policy::AllDiscard => { negative_discards += usize::from(difference>0); },
                }
                outcomes.push(json!({"policy":format!("{policy:?}"),"loads":load_count,
                    "encodedPasses":encoded,"differentBytes":difference,
                    "rgbaSha256":format!("{:x}",Sha256::digest(&actual))}));
            }
            renderer.check_health().unwrap();
            evidence.push(json!({"case":case,"frame":frame,"size":size,"attachments":reports,
                "policies":outcomes,"adapter":renderer.adapter_info.name,"backend":format!("{:?}",renderer.adapter_info.backend)}));
        }
    }
    assert!(sprites>0,"fixture never encoded partial sprite geometry");
    assert!(padded>0,"fixture never reused capacity outside the viewport");
    assert!(discards>0,"fixture never executed a validated Kill shader");
    assert!(negative_loads>0,"wrong blanket Load did not reveal stale pixels");
    assert!(negative_discards>0,"wrong blanket Discard did not reveal missing consumers");
    println!("ATTACHMENT_POLICY_MATRIX {}",json!({"steps":evidence,"negativeLoads":negative_loads,
        "negativeDiscards":negative_discards,"paddedAttachments":padded,"spriteAttachments":sprites,"discardAttachments":discards,
        "poison":true,"performanceQualified":false}));
}

#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn native_attachment_cost_samples_keep_pixels_and_report_actual_gpu_intervals() {
    // A bounded diagnostic, never a timing threshold or an optimization gate.
    // ABBA order retains every cold/warm sample; it does not establish tile
    // bandwidth, interactive deadlines or universal speedup.
    const SIZE: [u32;2] = [640,360];
    let mut project = Project::new(SIZE[0],SIZE[1],30,120).unwrap();
    project.background = [0.;4];
    let mut layer = Layer::solid(1,"Full attachment cost fixture",[640.,360.],
        [320.,180.,0.],[0.9,0.2,0.4,0.6]);
    for effect in ["gaussian_blur","tint","gaussian_blur","tint"] { add_effect(&mut layer,effect); }
    project.layers.push(layer);
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene = Scene::new(&project).unwrap();
    scene.sample(&project,0.,None,&ExpressionEvaluator).unwrap();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let mut renderer = pollster::block_on(Renderer::new_profiled(&instance,None,TARGET_FORMAT,true)).unwrap();
    let source = renderer.render_target(SIZE[0],SIZE[1]).unwrap();
    let target = renderer.capture_target(SIZE[0],SIZE[1]).unwrap();
    let presenter = crate::Presenter::new_admitted(&renderer,&source.view,renderer.target_format).unwrap();
    let mut timer = crate::GpuTimer::new(&renderer).unwrap();
    let expected = renderer.capture(&scene,&target).unwrap().0;
    let reports = renderer.effect_attachment_intents().unwrap();
    assert!(reports.iter().any(|report|report.full_overwrite_candidate));
    let mut records = Vec::new();
    for (block,policy) in [Policy::Clear,Policy::ProvenLoad,Policy::ProvenLoad,Policy::Clear].into_iter().enumerate() {
        let mut experiment = control(&reports,policy);
        experiment.poison = false;
        let loads = experiment.operations.iter().filter(|ops|matches!(ops.load,wgpu::LoadOp::Load)).count();
        renderer.effect_gpu.state.attachment_test_policy = Some(experiment);
        for sample in 0..11 {
            renderer.effect_gpu.state.attachment_test_policy.as_mut().unwrap().encoded.clear();
            let sequence = records.len() as u64;
            let slot = timer.as_mut().map(|clock|clock.begin(sequence).expect("offline timing slot busy"));
            let started = Instant::now();
            let mut encoder = renderer.allocation_command_encoder(&Default::default()).unwrap();
            let stats = renderer.encode(&scene,&source.view,SIZE[0],SIZE[1],&mut encoder,
                timer.as_ref().zip(slot).map(|(clock,slot)|clock.writes(slot,0))).unwrap();
            presenter.encode(&mut encoder,&target.view,
                timer.as_ref().zip(slot).map(|(clock,slot)|clock.writes(slot,1)),None,project.background);
            if let (Some(clock),Some(slot)) = (&timer,slot) { clock.resolve(slot,&mut encoder); }
            renderer.submit_allocated(encoder.finish(),|ledger| {
                source.collect_texture_resources(ledger,"attachment-cost/source",crate::TextureState::Retained);
                ledger.merge(target.texture_resource_ledger());
            }).unwrap();
            if let (Some(clock),Some(slot)) = (&timer,slot) { clock.map(slot); }
            renderer.device.poll(wgpu::PollType::Wait {
                submission_index: None, timeout: Some(std::time::Duration::from_secs(10)),
            }).unwrap();
            let wall_us = started.elapsed().as_secs_f64()*1_000_000.;
            let gpu = timer.as_mut().map(|clock| {
                let timing = clock.collect().into_iter().flatten().next().expect("actual GPU interval missing");
                assert_eq!(timing.sequence,sequence);
                timing
            });
            let encoded = &renderer.effect_gpu.state.attachment_test_policy.as_ref().unwrap().encoded;
            assert_eq!(encoded,&(0..reports.len()).collect::<Vec<_>>());
            renderer.check_health().unwrap();
            records.push(json!({"block":block,"sample":sample,"warmup":sample<3,
                "policy":format!("{policy:?}"),"loads":loads,"encodedPasses":encoded.len(),
                "drawCalls":stats.draw_calls,"wallUs":wall_us,"compositionGpuUs":gpu.map(|v|v.composition_us),
                "presentationGpuUs":gpu.map(|v|v.presentation_us),"gpuTotalUs":gpu.map(|v|v.total_us),
                "imageReadbackBytes":0,"timestampReadbackBytes":if timer.is_some(){32}else{0}}));
        }
        // Full RGBA checks are outside the measured interval. Rendering directly
        // avoids introducing the separate Presenter's display transfer here.
        let actual = renderer.capture(&scene,&target).unwrap().0;
        assert_eq!(actual,expected,"cost fixture policy={policy:?} changed pixels");
    }
    println!("ATTACHMENT_COST_SAMPLES {}",json!({"adapter":renderer.adapter_info.name,
        "backend":format!("{:?}",renderer.adapter_info.backend),"size":SIZE,"records":records,
        "rgbaSha256":format!("{:x}",Sha256::digest(&expected)),"pixelEquality":true,
        "poison":false,"scope":"serialized offscreen composition and presentation; wall includes completion wait; no physical bandwidth measurement",
        "timestampSupported":timer.is_some(),"performanceQualified":false}));
}
