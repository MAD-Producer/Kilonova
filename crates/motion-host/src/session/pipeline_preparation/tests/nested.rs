use super::*;
use motion_core::{Composition, CompositionClip, Content, FrameRate, ClipRange};
use motion_core::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch, TimeRemap, TimeRemapKey};

fn nested_fixture() -> Project {
    let (mut project, _) = fixture();
    let mut leaf = project.layers.remove(1);
    leaf.timeline = Some(LayerTimeline { in_frame: 20, out_frame: 24, offset_frame: 0 });
    project.layers.clear();
    project.compositions.push(Composition {
        id: "comp-future-leaf".into(), name: "future leaf".into(), width:16, height:16,
        fps: FrameRate::integer(60).unwrap(), frames:120,
        work_area:motion_core::WorkArea::full(120), guides:vec![], background:[0.;4],
        camera:motion_core::Camera::new(16,16), layers:vec![leaf], expressions:vec![],
    });
    let mut reference = Layer::solid(7, "nested reference", [16.;2], [8.,8.,0.], [1.;4]);
    let mut clip = CompositionClip::new("comp-future-leaf".into());
    clip.source_start_frame=4;
    reference.content=Content::Composition {clip};
    project.layers.push(reference);
    project.rebuild_plugin_dependencies();
    project
}

fn check_pixels(project: &Project, frame: f64, revision: u64) {
    let mut renderer=pollster::block_on(Renderer::headless()).unwrap();
    project.validate().unwrap_or_else(|error| panic!("revision {revision}: {error}"));
    let mut scene=Scene::new(project).unwrap();
    scene.sample(project,0.,None,&ExpressionEvaluator).unwrap();
    assert!(scene.nested.is_empty() || scene.nested[0].scene.layers.is_empty());
    let mut state=State::default();
    let mut key=key(); key.context.project_revision=revision;
    finish(&mut state,&mut renderer,project,&scene,key);
    assert_eq!(state.status()["lookaheadErrors"],serde_json::json!([]));
    let builds=renderer.effect_pipeline_builds();
    let target=renderer.capture_target(16,16).unwrap();
    let before=renderer.capture(&scene,&target).unwrap().0;
    scene.sample(project,frame,None,&ExpressionEvaluator).unwrap();
    let after=renderer.capture(&scene,&target).unwrap().0;
    assert_ne!(before,after,"future nested effect must be visible at {frame}");
    assert_eq!(renderer.effect_pipeline_builds(),builds,"nested effect compiled on foreground at {frame}");
    let mut cold=pollster::block_on(Renderer::headless()).unwrap();
    let target=cold.capture_target(16,16).unwrap();
    let before_cold=cold.effect_pipeline_builds();
    assert_eq!(cold.capture(&scene,&target).unwrap().0,after);
    assert!(cold.effect_pipeline_builds()>before_cold,"negative control must compile the future effect");
}

#[test]
#[cfg_attr(not(feature="gpu-tests"),ignore="requires an actual GPU")]
fn nested_worker_prepares_fps_start_stretch_reverse_hold_and_elastic_overshoot() {
    let mut project=nested_fixture();
    check_pixels(&project,9.,101);
    let mut deep=project.clone();
    let mut grandchild=deep.compositions[0].clone();
    grandchild.id="comp-grandchild".into();
    let mut inner=deep.compositions[0].layers[0].clone();
    inner.effects.clear();
    let mut clip=CompositionClip::new(grandchild.id.clone());
    clip.source_start_frame=5;
    inner.content=Content::Composition {clip};
    inner.time_mapping=Some(LayerTimeMapping {clock:LayerClock::new(
        TimeStretch::new(2,1).unwrap(),TimeDirection::Forward,20.,16.).unwrap(),remap:None});
    deep.compositions[0].layers=vec![inner];
    deep.compositions.push(grandchild);
    deep.rebuild_plugin_dependencies();
    check_pixels(&deep,9.,106);
    project.layers[0].time_mapping=Some(LayerTimeMapping {
        clock:LayerClock::new(TimeStretch::new(1,2).unwrap(),TimeDirection::Forward,0.,0.).unwrap(),remap:None,
    });
    check_pixels(&project,4.5,102);
    project.layers[0].time_mapping.as_mut().unwrap().clock=LayerClock::new(
        TimeStretch::default(),TimeDirection::Reverse,0.,20.).unwrap();
    check_pixels(&project,11.,103);
    project.layers[0].time_mapping=Some(LayerTimeMapping {clock:LayerClock::default(),remap:Some(TimeRemap::new(
        [ (0.,0.),(10.,9.),(20.,0.) ].into_iter().map(|(frame,source_frame)|TimeRemapKey {
            frame,source_frame,easing:motion_core::Easing {ease:motion_core::Ease::Hold,curve:None},
        }).collect()).unwrap())});
    check_pixels(&project,15.,104);
    project.compositions[0].fps=project.fps;
    project.compositions[0].layers[0].timeline=Some(LayerTimeline {in_frame:1,out_frame:2,offset_frame:0});
    project.compositions[0].layers[0].subframe_range=Some(ClipRange {in_frame:1.1,out_frame:1.2});
    let Content::Composition {clip}=&mut project.layers[0].content else { panic!("fixture reference"); };
    clip.source_start_frame=0;
    project.layers[0].time_mapping=Some(LayerTimeMapping {clock:LayerClock::default(),remap:Some(TimeRemap::new(vec![
        TimeRemapKey {frame:0.,source_frame:0.,easing:motion_core::Easing {ease:motion_core::Ease::Linear,
            curve:Some(motion_core::Curve {space:motion_core::CurveSpace::Progress,
                shape:motion_core::CurveShape::Elastic {oscillations:2.,damping:0.5}})}},
        TimeRemapKey {frame:60.,source_frame:1.,easing:Default::default()},
    ]).unwrap())});
    // Independent oscillator formula locates the first 1.15 source sample.
    let (mut lo,mut hi)=(0.,0.25);
    let w=4.*std::f64::consts::PI;
    for _ in 0..64 {
        let mid=(lo+hi)/2.;
        let progress=(1.-(-0.5_f64*mid).exp()*((w*mid).cos()+0.5/w*(w*mid).sin()))/(1.-(-0.5_f64).exp());
        if progress<1.15 {lo=mid;} else {hi=mid;}
    }
    check_pixels(&project,60.*(lo+hi)/2.,105);
}

#[test]
#[cfg_attr(not(feature="gpu-tests"),ignore="requires an actual GPU")]
fn nested_event_budget_reports_degradation_but_current_pixels_and_cancellation_remain_valid() {
    let mut project=nested_fixture();
    let leaf=project.compositions[0].layers[0].clone();
    project.compositions[0].layers=(0..20).map(|index| {
        let mut layer=leaf.clone(); layer.id=100+index;
        layer.effects[0].id=100+index; layer
    }).collect();
    let reference=project.layers[0].clone();
    project.layers=(0..32).map(|index| {let mut layer=reference.clone();layer.id=1000+index;layer}).collect();
    project.rebuild_plugin_dependencies();
    project.validate().unwrap();
    let mut scene=Scene::new(&project).unwrap();
    scene.sample(&project,0.,None,&ExpressionEvaluator).unwrap();
    let mut renderer=pollster::block_on(Renderer::headless()).unwrap();
    let mut state=State::default();
    finish(&mut state,&mut renderer,&project,&scene,key());
    let status=state.status();
    assert_eq!(status["limited"],true);
    assert!(status["lookaheadErrors"].to_string().contains("event budget exceeded"),"{status}");
    let target=renderer.capture_target(16,16).unwrap();
    let current=renderer.capture(&scene,&target).unwrap().0;
    let mut cold=pollster::block_on(Renderer::headless()).unwrap();
    let target=cold.capture_target(16,16).unwrap();
    assert_eq!(cold.capture(&scene,&target).unwrap().0,current);
    let ticket=state.ticket.as_ref().unwrap().clone();
    ticket.cancel();
    assert!(matches!(lookahead::frames(&project,0.,&ticket),Err(lookahead::Failure::Cancelled)));
}
