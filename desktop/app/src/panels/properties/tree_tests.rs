use super::*;
use motion_core::{Layer, Project};

fn anchor_state(engine: &motion_core::Engine, frame: f64) -> Value {
    let p=engine.project();
    serde_json::json!({"frame":frame,"project":p,"timeline_layers":p.timeline_layers(frame).unwrap(),
        "sampledLayers":p.layers.iter().map(|l|{let f=l.local_frame(frame);serde_json::json!({
            "id":l.id,"anchor":l.transform.anchor.sample(f),"position":l.transform.position.sample(f),
            "rotation":l.transform.rotation.sample(f),"scale":l.transform.scale.sample(f),"opacity":l.transform.opacity.sample(f)})}).collect::<Vec<_>>()})
}
#[test]
fn anchor_stopwatch_and_animated_numeric_cancel_agree_in_both_property_hosts() {
    for locale in [Locale::Zh,Locale::En] { for host in [Panel::Properties,Panel::Timeline] {
        let mut p=Project::new(640,360,30,120).unwrap();
        p.layers.push(Layer::solid(7,"Anchor",[100.,200.],[320.,180.,0.],[1.;4]));
        let mut e=motion_core::Engine::new(p).unwrap();
        let mut panels=Panels::default();panels.locale=locale;panels.focused=Some(host);panels.select_only(Some(7));panels.timeline.expanded.insert(7);
        let area=Rect::new(0.,0.,720.,520.);let mut atlas=TextAtlas::from_system(12.).unwrap();
        let mut paint=PaintList::default();let styles=paint.styles();let row=styles.metric("row.height",22.);
        // Inspector Masks precedes Transform; this empty layer has no timeline mask rows.
        let y=if host==Panel::Timeline {64.+row*2.} else {styles.metric("padding.panel",8.)+row*3.};
        let layout=if host==Panel::Timeline {super::super::timeline::property_layout(&styles,area,y,row,0,2)}else{inspector_layout(&styles,area,y,0,2)};
        let point=[layout.watch.min[0]+layout.watch.width()*0.5,y+row*0.5];let mut actions=vec![];
        let state=anchor_state(&e,30.);
        if host==Panel::Timeline {super::super::timeline::render(&mut paint,&mut atlas,area,&mut panels,&state,&pointer(point,true,true),&mut actions);}
        else {render(&mut paint,&mut atlas,area,&mut panels,&state,&pointer(point,true,true),&mut actions);}
        let commands=actions.into_iter().flat_map(|a| match a {Action::Edit(c)=>vec![c],Action::EditBatch(c)=>c,_=>vec![]}).collect::<Vec<_>>();
        assert!(commands.iter().any(|c|matches!(c,Command::Animate{object:7,property:Property::Anchor,enabled:true,..})),"host={host:?}");
        e.apply_batch(commands).unwrap();
        e.apply(Command::SetVector{object:7,property:Property::Anchor,frame:60,value:[0.2,0.8,0.]}).unwrap();
        let before=e.snapshot();let state=anchor_state(&e,45.);
        let mut control=AnchorControls::default();let field=layout.number;
        let point=[field.min[0]+field.width()*0.5,field.min[1]+field.height()*0.5];let mut actions=vec![];
        control.draw(&mut paint,&mut atlas,area,y,host,&state["project"]["layers"][0],&state,&pointer(point,true,false),false,&mut actions);
        control.process(&styles,&state,&pointer([point[0]+20.,point[1]],false,false),&mut actions);
        assert!(actions.iter().any(|a|matches!(a,Action::Edit(Command::AnchorAt{object:7,frame:45,..}))));
        apply(&mut e,std::mem::take(&mut actions));
        let state=anchor_state(&e,45.);
        control.process(&styles,&state,&Input{focus_lost:true,..Default::default()},&mut actions);
        apply(&mut e,actions);assert_eq!(e.project(),&before);assert!(!control.editing());
    }}
}

#[test]
fn invalid_anchor_edit_reaches_reply_file_stderr_without_mutating_state() {
    const CHILD:&str="MOTION_ANCHOR_ERROR_CHILD";
    const CAUSE:&str="anchor requires two components; Z must be zero";
    if let Some(root)=std::env::var_os(CHILD) {
        let root=std::path::PathBuf::from(root);let _logger=crate::logging::init(root.clone()).unwrap();
        let engine=crate::engine::Engine::start(root.join("project"),motion_desktop_media::platform(),8<<30).unwrap();
        engine.edit(serde_json::to_string(&Command::Add{layer:Layer::solid(7,"Anchor",[100.;2],[0.;3],[1.;4])}).unwrap()).unwrap();
        let before=engine.state().unwrap();
        let failure=engine.edit(serde_json::to_string(&Command::SetVector{object:7,property:Property::Anchor,frame:0,value:[0.,0.,1.]}).unwrap()).unwrap_err();
        assert!(failure.contains(CAUSE));
        let after=engine.state().unwrap();assert_eq!(before["project"],after["project"]);assert_eq!(before["revision"],after["revision"]);
        assert!(!engine.failed());
        let mut animated=before.clone();
        animated["project"]["layers"][0]["transform"]["anchor"]=serde_json::json!({"value":[0.5,0.5],"keys":[{"frame":0,"value":[0.5,0.5],"ease":"linear"}]});
        animated["sampledLayers"]=serde_json::json!([]);
        let mut healthy=animated.clone();healthy["sampledLayers"]=serde_json::json!([{"id":7,"anchor":[0.5,0.5]}]);
        for state in [&animated,&healthy,&animated] {
            for _ in 0..80 { AnchorControls::values_at(state,&state["project"]["layers"][0]); }
        }
        let mut bad_size=healthy.clone();bad_size["project"]["layers"][0]["size"][0]=0.into();
        for state in [&bad_size,&healthy,&bad_size] { for _ in 0..80 {
            animation_data::display_scale(state,7,Property::Anchor,0);
        }}
        return;
    }
    let dir=tempfile::tempdir().unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","panels::properties::tree_tests::invalid_anchor_edit_reaches_reply_file_stderr_without_mutating_state"])
        .env(CHILD,dir.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{} {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    let file=std::fs::read_to_string(dir.path().join("motion-studio.log")).unwrap();
    assert!(file.contains("logging initialized level=INFO") && file.contains("start #"));
    for cause in ["animated layer anchor sample missing","anchor source size unavailable"] {
        // Panel observations deliberately accumulate per process/call-site:
        // a healthy neighbouring row cannot reset another malformed row.
        // The first warning is file-only; the single 60th Error also reaches
        // stderr. Interleaving healthy rows must not restart this episode.
        assert_eq!(file.matches(cause).count(),2);
        assert_eq!(String::from_utf8_lossy(&output.stderr).matches(cause).count(),1);
        assert!(!String::from_utf8_lossy(&output.stdout).contains(cause));
    }
    for site in ["properties.anchor_sample","animation.anchor_display_scale"] {
        assert_eq!(file.matches(&format!("site={site} count=1 ")).count(),1);
        assert_eq!(file.matches(&format!("site={site} count=60 ")).count(),1);
    }
    assert!(file.contains(CAUSE));assert!(String::from_utf8_lossy(&output.stderr).contains(CAUSE));assert!(!String::from_utf8_lossy(&output.stdout).contains(CAUSE));
}

fn pointer(point: [f32; 2], pressed: bool, released: bool) -> Input {
    Input {
        mouse: point,
        pressed: pressed.then_some((MouseButton::Left, point)),
        released: released.then_some((MouseButton::Left, point)),
        held: (!released).then_some(MouseButton::Left),
        ..Default::default()
    }
}
fn apply(engine: &mut motion_core::Engine, actions: Vec<Action>) {
    for action in actions {
        match action {
            Action::Edit(command) => engine.apply(command).unwrap(),
            Action::History(2) => engine.begin_gesture().unwrap(),
            Action::History(3) => engine.end_gesture(true).unwrap(),
            Action::History(4) => engine.end_gesture(false).unwrap(),
            _ => panic!("unexpected anchor action"),
        }
    }
}
#[test]
fn anchor_rows_keep_one_gesture_when_other_layers_render_and_cancel_when_hidden() {
    for host in [Panel::Properties, Panel::Timeline] {
        let mut project = Project::new(640, 360, 30, 120).unwrap();
        project
            .layers
            .push(Layer::solid(7, "A", [100.; 2], [0.; 3], [1.; 4]));
        project
            .layers
            .push(Layer::solid(8, "B", [100.; 2], [0.; 3], [1.; 4]));
        let mut engine = motion_core::Engine::new(project).unwrap();
        let before = engine.project().clone();
        let mut controls = AnchorControls::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 400., 600.);
        let mut paint = PaintList::default();
        let styles = paint.styles();
        let rect = if host == Panel::Timeline {
            super::super::timeline::property_layout(&styles, area, 100., 22., 0, 2).number
        } else {
            inspector_layout(&styles, area, 100., 0, 2).number
        };
        let point = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        let mut actions = vec![];
        let layer = serde_json::to_value(&engine.project().layers[0]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            100.,
            host,
            &layer,
            &serde_json::json!({"project":engine.project()}),
            &pointer(point, true, false),
            false,
            &mut actions,
        );
        assert!(actions.is_empty());
        let other = serde_json::to_value(&engine.project().layers[1]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            150.,
            host,
            &other,
            &serde_json::json!({"project":engine.project()}),
            &Input::default(),
            false,
            &mut actions,
        );
        assert!(controls.editing() && actions.is_empty());
        for (dx, released) in [(20., false), (30., true)] {
            let state = serde_json::json!({"project":engine.project()});
            controls.process(
                &styles,
                &state,
                &pointer([point[0] + dx, point[1]], false, released),
                &mut actions,
            );
            apply(&mut engine, std::mem::take(&mut actions));
        }
        assert!(!controls.editing());
        assert!((engine.project().layers[0].transform.anchor.value[0] - 0.65).abs() < 0.001);
        assert_eq!(
            engine.project().layers[0].transform.anchor.value[1],
            before.layers[0].transform.anchor.value[1]
        );
        assert_eq!(engine.project().layers[1], before.layers[1]);
        engine.undo().unwrap();
        assert_eq!(engine.project(), &before);
        let layer = serde_json::to_value(&engine.project().layers[0]).unwrap();
        controls.draw(
            &mut paint,
            &mut atlas,
            area,
            100.,
            host,
            &layer,
            &serde_json::json!({"project":engine.project()}),
            &pointer(point, true, false),
            false,
            &mut actions,
        );
        controls.process(
            &styles,
            &serde_json::json!({"project":engine.project()}),
            &pointer([point[0] + 20., point[1]], false, false),
            &mut actions,
        );
        apply(&mut engine, std::mem::take(&mut actions));
        // No draw call: a folded or clipped row must still receive cancellation.
        controls.process(
            &styles,
            &serde_json::json!({"project":engine.project()}),
            &Input {
                focus_lost: true,
                ..Default::default()
            },
            &mut actions,
        );
        apply(&mut engine, actions);
        assert!(!controls.editing());
        assert_eq!(engine.project(), &before);
        engine.begin_gesture().unwrap();
        engine.end_gesture(false).unwrap();
    }
}

#[test]
fn embedded_effect_dropdown_blocks_group_folding_in_main_and_floating_properties() {
    let folder = tempfile::tempdir().unwrap();
    let engine = crate::engine::Engine::start(
        folder.path().join("project"),
        motion_desktop_media::platform(),
        8 << 30,
    )
    .unwrap();
    engine
        .edit(
            serde_json::to_string(&Command::Add {
                layer: Layer::solid(7, "Tree", [100.; 2], [0.; 3], [1.; 4]),
            })
            .unwrap(),
        )
        .unwrap();
    let catalogue = engine.plugin("{\"op\":\"catalogue\"}".into()).unwrap();
    let package = catalogue["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| {
            package["manifest"]["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|effect| effect["id"] == "drop_shadow")
        })
        .unwrap();
    engine.plugin(serde_json::json!({"op":"add","object":7,"plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"],"effect":"drop_shadow"}).to_string()).unwrap();
    let before = engine.state().unwrap();
    let effect = before["project"]["layers"][0]["effects"][0]["id"]
        .as_u64()
        .unwrap();
    for floating in [false, true] {
        let mut panels = Panels::default();
        panels.selected = Some(7);
        panels.properties.transform_collapsed = true;
        panels.properties.effects_collapsed = false;
        panels.effect_controls.catalogue = catalogue.clone().into();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let layout = Node::dock(Panel::Properties.dock());
        let mut draw = |panels: &mut Panels, input: &Input| {
            let state = engine.state().unwrap();
            if floating {
                super::super::floating_chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    Panel::Properties,
                    [360., 560.],
                    panels,
                    &state,
                    input,
                )
            } else {
                super::super::chrome(
                    &mut PaintList::default(),
                    &mut atlas,
                    [360., 660.],
                    &layout,
                    panels,
                    &state,
                    input,
                )
            }
        };
        assert!(draw(&mut panels, &Input::default()).is_empty());
        let menu = panels.properties.effects.test_rects[&(7, effect, "mode".into(), -4)];
        assert!(draw(
            &mut panels,
            &pointer(
                [menu.min[0] + 2., menu.min[1] + menu.height() * 0.5],
                true,
                false
            )
        )
        .is_empty());
        assert!(panels.properties.effects.editing());
        let top = if floating {
            30.
        } else {
            frames(&layout, [360., 660.])[0].content.min[1]
        };
        assert!(draw(
            &mut panels,
            &pointer([60., top + 8. + 22. + 11.], true, false)
        )
        .is_empty());
        assert!(!panels.properties.effects.editing());
        assert!(panels.properties.transform_collapsed);
        assert_eq!(engine.state().unwrap()["project"], before["project"]);
    }
}
