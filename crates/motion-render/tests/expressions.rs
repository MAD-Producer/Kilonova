use motion_core::ProjectExpressions;
use motion_core::{Command, EffectInstance, Engine, ExpressionTarget, Layer, Project, Property, PropertyExpression, EXPRESSION_PROFILE};
use motion_render::Scene;
use motion_render::{effect_plan::PlanBuilder, Renderer};
#[test]
fn expressions_drive_geometry_effects_and_the_shared_export_plan() {
    let mut p = Project::new(64, 64, 30, 90).unwrap();
    p.background = [0.0; 4];
    p.layers.push(Layer::solid(
        1,
        "Card",
        [16.0; 2],
        [16.0, 32.0, 0.0],
        [1.0, 0.0, 0.0, 1.0],
    ));
    let pkg = motion_effects::builtin::package().unwrap();
    let tint = pkg
        .manifest
        .effects
        .iter()
        .find(|e| e.id == "tint")
        .unwrap();
    p.layers[0].effects.push(EffectInstance::new(
        1,
        &pkg.manifest.id,
        &pkg.manifest.version,
        &pkg.hash,
        tint,
        [16.0; 2],
    ));
    p.rebuild_plugin_dependencies();
    let mut engine = Engine::new(p).unwrap();
    for (target, source) in [
        (
            ExpressionTarget::Property {
                object: 1,
                property: Property::Position,
                axis: None,
            },
            "value+[time*16,0,0]",
        ),
        (
            ExpressionTarget::Effect {
                object: 1,
                effect: 1,
                param: "p0003".into(),
            },
            "linear(time,0,1,0,100)",
        ),
    ] {
        engine
            .apply(Command::SetExpression {
                expression: PropertyExpression {
                    target,
                    source: source.into(),
                    enabled: true,
                    seed: 0,
                    profile: EXPRESSION_PROFILE.into(),
                },
                frame: 0,
            })
            .unwrap();
    }
    let mut scene = Scene::new(engine.project());
    let mut renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.capture_target(64, 64).unwrap();
    scene.sample(engine.project(), 0.0, None, &motion_core::ExpressionEvaluator).unwrap();
    let (start, _) = renderer.capture(&scene, &target).unwrap();
    scene.sample(engine.project(), 30.0, None, &motion_core::ExpressionEvaluator).unwrap();
    let (end, _) = renderer.capture(&scene, &target).unwrap();
    let px = |v: &[u8], x: usize| v[(32 * 64 + x) * 4..(32 * 64 + x) * 4 + 4].to_vec();
    assert!(px(&start, 16)[0] > 250 && px(&start, 16)[1] < 3);
    assert_eq!(px(&end, 16), [0, 0, 0, 0]);
    let gray = px(&end, 32);
    assert!(gray[0] > 50 && gray[0].abs_diff(gray[1]) < 2 && gray[1].abs_diff(gray[2]) < 2);
    let mut builder =
        PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let plan = builder.build(&scene, &[0], 64, 64, true).unwrap();
    let mut bytes = vec![0; plan.buffer_bytes(&scene)];
    assert_eq!(plan.write(&scene, &mut bytes).unwrap(), bytes.len());
    let tint_amount = scene.effects[0]
        .param_ids
        .iter()
        .position(|id| id == "p0003")
        .unwrap();
    assert_eq!(scene.effects[0].values[tint_amount][0], 100.0);
    engine
        .apply(Command::Effect {
            object: 1,
            action: motion_core::EffectAction::Duplicate { effect: 1 },
        })
        .unwrap();
    assert_eq!(engine.project().expressions.len(), 3);
    engine
        .apply(Command::Effect {
            object: 1,
            action: motion_core::EffectAction::Remove { effect: 2 },
        })
        .unwrap();
    assert_eq!(engine.project().expressions.len(), 2);
    let frozen = engine.snapshot();
    engine
        .apply(Command::RemoveExpression {
            target: ExpressionTarget::Property {
                object: 1,
                property: Property::Position,
                axis: None,
            },
        })
        .unwrap();
    assert_eq!(
        frozen.evaluated_at(30.0).unwrap().layers[0]
            .transform
            .position
            .value,
        [32.0, 32.0, 0.0]
    );
}

#[test]
fn separated_axis_and_effect_expressions_match_explicit_values_in_pixels_and_frozen_plans() {
    let mut project = Project::new(64,64,30,90).unwrap();
    project.background = [0.;4];
    project.layers.push(Layer::solid(1,"Expression target",[16.;2],[16.,32.,0.],[1.,0.,0.,1.]));
    project.layers[0].transform.position.separate().unwrap();
    let package = motion_effects::builtin::package().unwrap();
    let tint = package.manifest.effects.iter().find(|e|e.id=="tint").unwrap();
    project.layers[0].effects.push(EffectInstance::new(9,&package.manifest.id,&package.manifest.version,&package.hash,tint,[16.;2]));
    project.rebuild_plugin_dependencies();
    let mut engine=Engine::new(project).unwrap();
    for (target,source) in [
        (ExpressionTarget::Property { object:1,property:Property::Position,axis:Some(motion_core::Axis::X) },"value+time*16"),
        (ExpressionTarget::Effect { object:1,effect:9,param:"p0003".into() },"linear(time,0,1,0,100)"),
    ] {
        engine.apply(Command::SetExpression { expression:PropertyExpression { target,source:source.into(),enabled:true,seed:81,profile:EXPRESSION_PROFILE.into() },frame:0 }).unwrap();
    }
    let frozen=engine.snapshot();
    let mut renderer=pollster::block_on(Renderer::headless()).unwrap();
    let target=renderer.capture_target(64,64).unwrap();
    let mut plans=PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    let mut distinct=std::collections::BTreeSet::new();
    for frame in [0.,7.5,15.25,30.,60.] {
        let mut explicit=frozen.clone();
        explicit.expressions.clear();
        explicit.layers[0].transform.position.axes.as_mut().unwrap().x.value=16.+frame as f32/30.*16.;
        explicit.layers[0].effects[0].params.get_mut("p0003").unwrap().track.value[0]=(frame as f32/30.*100.).clamp(0.,100.);
        let mut preview=Scene::new(engine.project());
        preview.sample(engine.project(),frame,None,&motion_core::ExpressionEvaluator).unwrap();
        let mut export=Scene::new(&frozen);
        export.sample(&frozen,frame,None,&motion_core::ExpressionEvaluator).unwrap();
        let mut reference=Scene::new(&explicit);
        reference.sample(&explicit,frame,None,&motion_core::ExpressionEvaluator).unwrap();
        let preview_pixels=renderer.capture(&preview,&target).unwrap().0;
        let export_pixels=renderer.capture(&export,&target).unwrap().0;
        let reference_pixels=renderer.capture(&reference,&target).unwrap().0;
        assert_eq!(preview_pixels,reference_pixels,"preview targets at {frame}");
        assert_eq!(export_pixels,reference_pixels,"frozen export targets at {frame}");
        assert!(export_pixels.chunks_exact(4).any(|p|p[3]>0));
        distinct.insert(export_pixels);
        let mut serialize=|scene:&Scene| {
            let plan=plans.build(scene,&[0],64,64,true).unwrap();
            let mut bytes=vec![0;plan.buffer_bytes(scene)];
            plan.write(scene,&mut bytes).unwrap();bytes
        };
        assert_eq!(serialize(&preview),serialize(&reference));
        assert_eq!(serialize(&export),serialize(&reference));
        assert_eq!(frozen.evaluated_at(frame).unwrap().layers[0].transform.position.sample(frame)[1],32.);
    }
    assert!(distinct.len()>2,"the fixture must animate visible content");
    engine.apply(Command::RemoveExpression { target:ExpressionTarget::Property { object:1,property:Property::Position,axis:Some(motion_core::Axis::X) } }).unwrap();
    assert_eq!(frozen.evaluated_at(30.).unwrap().layers[0].transform.position.sample(30.),[32.,32.,0.]);
}
