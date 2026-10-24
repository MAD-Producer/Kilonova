use motion_core::{Camera,Composition,CompositionClip,Content,ExpressionEvaluator,Layer,LayerTimeline,Project};
use motion_render::{Renderer,Scene};

fn render(renderer:&mut Renderer,project:&Project,frame:f64)->Vec<u8>{
    let mut scene=Scene::new(project).unwrap();scene.sample(project,frame,None,&ExpressionEvaluator).unwrap();
    let target=renderer.capture_target(64,64).unwrap();renderer.capture(&scene,&target).unwrap().0
}
#[test]
#[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
fn nested_signed_span_renders_recovered_tail_and_random_seeks_consistently(){
    let mut p=Project::new(64,64,30,24).unwrap();p.version = motion_model::CURRENT_PROJECT_VERSION;p.background=[0.,0.,1.,1.];
    let mut leaf=Layer::solid(1,"Red",[64.;2],[32.,32.,0.],[1.,0.,0.,1.]);
    leaf.timeline=Some(LayerTimeline{in_frame:-6,out_frame:36,offset_frame:-6});
    let mut camera=Camera::new(64,64);camera.created=false;
    p.compositions.push(Composition{id:"comp-child".into(),name:"Child".into(),width:64,height:64,fps:30,work_area: motion_core::WorkArea::full(48), frames:48,
        background:[0.;4],camera,layers:vec![leaf],expressions:vec![]});
    let mut reference=Layer::solid(2,"Reference",[64.;2],[32.,32.,0.],[1.;4]);
    reference.content=Content::Composition{clip:CompositionClip::new("comp-child".into())};
    reference.timeline=Some(LayerTimeline{in_frame:-6,out_frame:48,offset_frame:0});p.layers.push(reference);p.validate().unwrap();
    let mut renderer=pollster::block_on(Renderer::headless()).unwrap();
    let before=render(&mut renderer,&p,12.);assert_eq!(&before[(32*64+32)*4..(32*64+32)*4+4],&[255,0,0,255]);
    p.frames=48;p.validate().unwrap();
    let tail=render(&mut renderer,&p,35.5);assert_eq!(&tail[(32*64+32)*4..(32*64+32)*4+4],&[255,0,0,255]);
    let ended=render(&mut renderer,&p,36.);assert_eq!(&ended[(32*64+32)*4..(32*64+32)*4+4],&[0,0,255,255]);
    assert_eq!(before,render(&mut renderer,&p,12.));
}
