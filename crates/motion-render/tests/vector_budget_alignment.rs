use motion_core::{Content,EffectInstance,Layer,Project,vector::{ShapeKind,VectorContent,VectorSource,groups::VectorGroup}};
use motion_render::{Renderer,Scene,TextureCategory,ResourceBudgetScope,ResourceRejection,
    effect_plan::PlanBuilder,resource_policy::MemoryPolicy,PlaneCompositor,selection_geometry};

fn sample(p:&Project)->Scene {
    let mut s=Scene::new(p);s.sample(p,30.,None,&motion_core::ExpressionEvaluator).unwrap();s
}
fn vector_project(grouped:bool,second_size:f32)->Project {
    let mut p=Project::new(64,64,30,120).unwrap();p.background=[0.;4];
    for (id,size) in [(1,20.),(2,second_size)] {
        let mut vector=VectorContent::shape(ShapeKind::Rectangle);
        if grouped {
            vector=VectorGroup::wrap(vector,[size;2]);
            let VectorSource::Group {group}=&mut vector.source else{unreachable!()};
            group.transform.opacity.value=50.;
        }
        let mut l=Layer::solid(id,"vector",[size;2],[32.,32.,0.],[1.;4]);l.content=Content::Vector {vector};p.layers.push(l);
    }
    p
}
fn working_bytes(r:&Renderer)->u64 {
    // Measure the exact owned textures included in the vector working-set guard.
    // The input white pixel is already resident; LUT/fallback/effect scratch are
    // separate policies. No estimated width/height or pool slot count is used.
    r.texture_resource_ledger().resources().filter(|r|r.references.iter().any(|a|
        matches!(a.category,TextureCategory::VectorSources|TextureCategory::VectorRasterScratch|TextureCategory::VectorGroupScratch|TextureCategory::VideoPlanes|TextureCategory::VideoRgba)
            || a.owner=="images/Static(0)")).map(|r|r.logical_bytes.unwrap()).sum()
}
fn policy(bytes:u64)->MemoryPolicy {MemoryPolicy {source_texture_budget_bytes:bytes,..Default::default()}}

#[test]
fn exact_actual_texture_budget_succeeds_and_one_byte_less_refuses_cached_and_uncached_vectors() {
    for grouped in [false,true] {
        for second_size in [20.,24.] {
            let p=vector_project(grouped,second_size);let s=sample(&p);
            let mut r=pollster::block_on(Renderer::headless()).unwrap();let target=r.capture_target(64,64).unwrap();
            let expected=r.capture(&s,&target).unwrap().0;
            let measured=working_bytes(&r);assert!(measured>4);
            let ledger=r.texture_resource_ledger();
            let sources:Vec<_>=ledger.resources().filter(|r|r.references.iter().any(|a|a.category==TextureCategory::VectorSources)).collect();
            assert_eq!(sources.len(),2);
            let scratch:Vec<_>=ledger.resources().filter(|r|r.references.iter().any(|a|matches!(a.category,TextureCategory::VectorRasterScratch|TextureCategory::VectorGroupScratch))).collect();
            assert!(scratch.iter().any(|r|r.samples==4));assert!(scratch.iter().any(|r|r.samples==1));
            if !grouped {
                assert_eq!(scratch.len(),if second_size==20.{2}else{4});
                assert_eq!(scratch.iter().map(|r|r.logical_bytes.unwrap()).sum::<u64>(),r.texture_memory_breakdown().vector_raster_scratch);
                for t in scratch.iter().filter(|r|r.samples==4) {
                    let resolved=scratch.iter().find(|r|r.samples==1&&r.width==t.width&&r.height==t.height).unwrap();
                    assert_eq!(t.logical_bytes.unwrap()+resolved.logical_bytes.unwrap(),u64::from(t.width)*u64::from(t.height)*20);
                }
            }
            r.set_memory_policy(policy(measured)).unwrap();
            assert_eq!(r.capture(&s,&target).unwrap().0,expected);
            assert_eq!(r.texture_resource_ledger(),ledger);
            r.set_memory_policy(policy(measured-1)).unwrap();
            let error=r.draw(&s,&target.view,64,64).unwrap_err();
            assert_eq!(error.resource_rejection(),Some(&ResourceRejection::Budget {scope:ResourceBudgetScope::VectorWorkingSet,required_bytes:measured,budget_bytes:measured-1}));
            assert_eq!(r.texture_resource_ledger(),ledger,"cached refusal must not replace textures");
            assert_eq!(r.read_target(&target).unwrap(),expected,"refusal must preserve the presented image");
            let mut fresh=pollster::block_on(Renderer::headless()).unwrap();let fresh_target=fresh.capture_target(64,64).unwrap();
            let before=fresh.texture_resource_ledger();fresh.set_memory_policy(policy(measured-1)).unwrap();
            let error=fresh.draw(&s,&fresh_target.view,64,64).unwrap_err();
            assert_eq!(error.resource_rejection(),Some(&ResourceRejection::Budget {scope:ResourceBudgetScope::VectorWorkingSet,required_bytes:measured,budget_bytes:measured-1}));
            assert_eq!(fresh.texture_resource_ledger(),before,"preflight must precede all vector allocation");
            fresh.set_memory_policy(policy(measured)).unwrap();assert_eq!(fresh.capture(&s,&fresh_target).unwrap().0,expected);
        }
    }
}

fn generator_project()->Project {
    let package=motion_effects::builtin::scene_package().unwrap();
    let def=package.manifest.effects.iter().find(|d|d.id=="starfield").unwrap();
    let mut p=Project::new(128,128,30,120).unwrap();p.background=[0.;4];p.camera.created=true;
    let mut l=Layer::solid(1,"generated",[8.;2],[88.,70.,30.],[0.;4]);l.three_d=true;
    l.transform.rotation.value=[17.,23.,31.];
    let mut effect=EffectInstance::new(1,&package.manifest.id,&package.manifest.version,&package.hash,def,[128.;2]);
    for (id,value) in [("extent",[0.;4]),("fade",[0.;4]),("size",[64.,0.,0.,0.]),("end_size",[64.,0.,0.,0.])] {
        effect.params.get_mut(id).unwrap().track.value=value;
    }
    l.effects.push(effect);p.layers.push(l);p.rebuild_plugin_dependencies();p
}

#[test]
fn cached_vector_working_set_includes_retained_yuv_planes() {
    let p=vector_project(false,20.);let s=sample(&p);
    let mut r=pollster::block_on(Renderer::headless()).unwrap();
    r.upload_video_yuv(99,1,0,&motion_render::Yuv420Frame {width:8,height:8,rotation:0,standard:1,range:2,phase:[0,0],
        chroma_layout:motion_render::ChromaLayout::Uv,y:vec![128;64],uv:vec![128;32]}).unwrap();
    let target=r.capture_target(64,64).unwrap();r.capture(&s,&target).unwrap();
    let ledger=r.texture_resource_ledger();
    assert!(ledger.resources().any(|r|r.references.iter().any(|a|a.category==TextureCategory::VideoPlanes)));
    let measured=working_bytes(&r);r.set_memory_policy(policy(measured-1)).unwrap();
    let error=r.draw(&s,&target.view,64,64).unwrap_err();
    assert_eq!(error.resource_rejection(),Some(&ResourceRejection::Budget {scope:ResourceBudgetScope::VectorWorkingSet,
        required_bytes:measured,budget_bytes:measured-1}));
    assert_eq!(r.texture_resource_ledger(),ledger);
}
#[test]
fn spatial_generator_pixels_pick_with_the_rendered_uv_and_leave_logical_transforms_intact() {
    let p=generator_project();let s=sample(&p);let registry=motion_effects::Registry::new_with_builtins().unwrap();
    assert!(s.layers[0].three_d&&s.layers[0].screen_overlay);
    let model=s.layers[0].model;let projection=s.layers[0].view_projection;let node=s.world_matrix(1).unwrap();
    assert_ne!(model,glam::Mat4::IDENTITY);
    let outline=selection_geometry::polygon(&s.layers[0],&s,&registry).unwrap();
    assert_eq!(outline,vec![[0.,0.],[128.,0.],[128.,128.],[0.,128.]]);
    let mut picker=s.clone();picker.layers[0]=selection_geometry::picking_layer(&s.layers[0],&s,&registry);
    let mut legacy=s.clone();legacy.layers[0].screen_overlay=false;
    let mut r=pollster::block_on(Renderer::headless()).unwrap();let target=r.capture_target(128,128).unwrap();
    let pixels=r.capture(&s,&target).unwrap().0;
    let (i,_)=pixels.chunks_exact(4).enumerate().find(|(i,p)|p[3]>32&&legacy.hit_candidates([(*i%128) as f32+0.5,(*i/128) as f32+0.5]).unwrap().is_empty()).expect("generated pixels must extend beyond the old transformed 8px plane");
    let point=[(i%128) as f32+0.5,(i/128) as f32+0.5];
    for scene in [&s,&picker] {
        let hits=scene.hit_candidates(point).unwrap();assert_eq!(hits.len(),1);assert!(hits[0].three_d);
        assert!((hits[0].uv[0]-point[0]/128.).abs()<1e-6);assert!((hits[0].uv[1]-point[1]/128.).abs()<1e-6);
    }
    assert_eq!(s.layers[0].model,model);assert_eq!(s.layers[0].view_projection,projection);assert_eq!(s.world_matrix(1).unwrap(),node);
    assert_eq!(r.capture(&s,&target).unwrap().0,pixels,"identical frames remain deterministic");
}
#[test]
fn generator_is_a_compositing_boundary_and_disabled_generator_returns_to_camera_geometry() {
    let mut p=generator_project();
    let mut front=Layer::solid(2,"front",[128.;2],[64.,64.,-100.],[1.;4]);front.three_d=true;
    let mut back=front.clone();back.id=3;back.name="back".into();back.transform.position.value[2]=100.;
    p.layers.insert(0,front);p.layers.push(back);
    let s=sample(&p);assert_eq!(s.layers.iter().map(|l|l.id).collect::<Vec<_>>(),vec![2,1,3]);
    let mut builder=PlanBuilder::new(motion_effects::Registry::new_with_builtins().unwrap()).unwrap();
    builder.build(&s,&[0],128,128,true).unwrap();
    let mut compositor=PlaneCompositor::new();compositor.prepare(&s).unwrap();
    let generated=s.layers.iter().position(|l|l.id==1).unwrap();
    let mvp=glam::Mat4::from_cols_array(builder.frame.draws[generated].words[..16].try_into().unwrap());
    for batch in compositor.batches.iter().filter(|b|b.layer==generated) {
        for vertex in &compositor.vertices[batch.vertices.start as usize..batch.vertices.end as usize] {
            let clip=mvp*glam::Vec3::from_array(vertex.position).extend(1.);
            assert!((((clip.x/clip.w+1.)*0.5)-vertex.uv[0]).abs()<1e-6);
            assert!((((1.-clip.y/clip.w)*0.5)-vertex.uv[1]).abs()<1e-6);
        }
    }
    p.layers[1].effects[0].enabled=false;
    let disabled=sample(&p);assert!(!disabled.layers.iter().find(|l|l.id==1).unwrap().screen_overlay);
    assert_ne!(disabled.layers.iter().map(|l|l.id).collect::<Vec<_>>(),vec![2,1,3],"disabled generator must rejoin the 3D run");
}

#[test]
fn zero_effect_opacity_is_the_original_plane_and_nonzero_generator_uses_screen_coordinates() {
    // A valid SDK package declares the optional wrapper opacity parameter.
    // Builtin scene packages currently omit it; use a temporary source package
    // rather than manufacturing a mismatched registry definition or hash.
    let original=motion_effects::builtin::scene_package().unwrap();
    let root=tempfile::tempdir().unwrap();
    for (name,bytes) in &original.files {
        let path=root.path().join(name);std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(path,bytes).unwrap();
    }
    let mut manifest:serde_json::Value=serde_json::from_slice(&original.files["manifest.json"]).unwrap();
    manifest["id"]="com.motionstudio.tests.overlay".into();manifest["version"]="1.0.0".into();
    let effect=manifest["effects"].as_array_mut().unwrap().iter_mut().find(|d|d["id"]=="starfield").unwrap();
    effect["params"].as_array_mut().unwrap().push(serde_json::json!({"id":"effect_opacity","name":"Mix opacity","kind":"float",
        "default":[100.,0.,0.,0.],"min":0.,"max":100.,"step":1.,"units":"%","animatable":true,"options":[]}));
    std::fs::write(root.path().join("manifest.json"),serde_json::to_vec(&manifest).unwrap()).unwrap();
    let package=std::sync::Arc::new(motion_effects::EffectPackage::from_bytes(motion_effects::package_directory(root.path()).unwrap()).unwrap());
    let def=package.manifest.effects.iter().find(|d|d.id=="starfield").unwrap();
    let mut p=generator_project();
    p.layers[0].effects=vec![EffectInstance::new(1,&package.manifest.id,&package.manifest.version,&package.hash,def,[8.;2])];
    p.rebuild_plugin_dependencies();
    let mut registry=motion_effects::Registry::new_with_builtins().unwrap();registry.insert(package.clone()).unwrap();
    let mut builder=PlanBuilder::new(registry.clone()).unwrap();
    p.layers[0].effects[0].params.get_mut("effect_opacity").unwrap().track.value=[0.;4];
    let zero=sample(&p);assert!(!zero.layers[0].screen_overlay);
    builder.build(&zero,&[0],128,128,false).unwrap();assert!(builder.frame.sprites.is_empty());
    let original_outline=selection_geometry::polygon(&zero.layers[0],&zero,&registry).unwrap();
    assert_ne!(original_outline,vec![[0.,0.],[128.,0.],[128.,128.],[0.,128.]]);
    p.layers[0].effects[0].params.get_mut("effect_opacity").unwrap().track.value=[50.,0.,0.,0.];
    let partial=sample(&p);assert!(partial.layers[0].screen_overlay);
    builder.build(&partial,&[0],128,128,false).unwrap();
    assert_eq!(selection_geometry::polygon(&partial.layers[0],&partial,&registry).unwrap(),vec![[0.,0.],[128.,0.],[128.,128.],[0.,128.]]);
}
