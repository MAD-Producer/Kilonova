//! Exercise production chrome, actual model commands and history for both providers.
use super::*;
use crate::{engine::Engine, i18n::Locale, panels::{self, Panels}};
use motion_ui::{dock::{Edge, Node, Panel}, input::{Key, Modifiers, MouseButton}};
use serde_json::Value;

const SIZE: [f32; 2] = [900., 720.];
#[derive(Clone, Copy, Debug)]
enum Provider { Effect, Transform }
impl Provider {
    fn panel(self) -> Panel {
        match self { Self::Effect => Panel::EffectControls, Self::Transform => Panel::Properties }
    }
}
struct Harness {
    folder: tempfile::TempDir,
    engine: Engine,
    panels: Panels,
    atlas: TextAtlas,
    layout: Node,
    provider: Provider,
    floating: bool,
    rect: Rect,
    outside: [f32; 2],
}
fn press(p: [f32; 2]) -> Input {
    Input { mouse:p, pressed:Some((MouseButton::Left,p)), held:Some(MouseButton::Left), ..Default::default() }
}
fn release(p: [f32; 2]) -> Input {
    Input { mouse:p, released:Some((MouseButton::Left,p)), ..Default::default() }
}
fn key(key: Key) -> Input { Input { keys:vec![(key,Default::default())], ..Default::default() } }
impl Harness {
    fn new(provider: Provider, floating: bool, locale: Locale) -> Self {
        let folder=tempfile::tempdir().unwrap();
        let engine=Engine::start(folder.path().join("project"),motion_desktop_media::platform(),8<<30).unwrap();
        engine.edit(serde_json::to_string(&Command::Add { layer:motion_core::Layer::solid(
            7,"Numeric contract",[64.;2],[100.;3],[1.;4]) }).unwrap()).unwrap();
        let catalogue=engine.plugin(r#"{"op":"catalogue"}"#.into()).unwrap();
        let package=catalogue["packages"].as_array().unwrap().iter().find(|p|
            p["manifest"]["effects"].as_array().unwrap().iter().any(|e|e["id"]=="drop_shadow")).unwrap();
        engine.plugin(serde_json::json!({"op":"add","object":7,"effect":"drop_shadow",
            "plugin":package["manifest"]["id"],"version":package["manifest"]["version"],"hash":package["hash"]}).to_string()).unwrap();
        let panel=provider.panel();
        let layout=Node::Split { edge:Edge::Left, split:0.5,
            first:Box::new(Node::dock(panel.dock())),second:Box::new(Node::dock(Panel::Info.dock())) };
        let mut h=Self { folder,engine,panels:Panels { selected:Some(7), focused:Some(panel),locale,..Default::default() },
            atlas:TextAtlas::from_system_language(12.,locale==Locale::Zh).unwrap(),
            layout,provider,floating,rect:Rect::new(0.,0.,0.,0.),outside:[0.;2] };
        h.panels.effect_controls.catalogue=catalogue.into();
        assert!(h.frame(Input::default()).1.is_empty());
        let area=if floating { Rect::new(0.,30.,SIZE[0],SIZE[1]-52.) } else {
            panels::frames(&h.layout,SIZE).into_iter().find(|f|f.panel==panel).unwrap().content };
        h.rect=match provider {
            Provider::Effect => {
                let effect=h.engine.state().unwrap()["project"]["layers"][0]["effects"][0]["id"].as_u64().unwrap();
                h.panels.effect_controls.test_rects[&(7,effect,"effect_opacity".into(),0)]
            },
            Provider::Transform => {
                let styles=PaintList::default().styles();
                panels::properties::inspector_layout(&styles,area,
                    area.min[1]+styles.metric("padding.panel",8.)+styles.metric("row.height",22.)*7.,0,1).number
            }
        };
        h.outside=if floating { [area.max[0]-12.,area.max[1]-12.] } else {
            let info=panels::frames(&h.layout,SIZE).into_iter().find(|f|f.panel==Panel::Info).unwrap().content;
            [info.min[0]+20.,info.min[1]+20.] };
        h
    }
    fn frame(&mut self,input: Input) -> (PaintList,Vec<Action>) {
        let mut paint=PaintList::default();
        let state=self.engine.state().unwrap();
        let actions=if self.floating { panels::floating_chrome(&mut paint,&mut self.atlas,
            self.provider.panel(),SIZE,&mut self.panels,&state,&input) } else {
            panels::chrome(&mut paint,&mut self.atlas,SIZE,&self.layout,&mut self.panels,&state,&input) };
        (paint,actions)
    }
    fn apply(&self,actions: Vec<Action>) {
        for action in actions { match action {
            Action::History(op)=>{self.engine.history(op).unwrap();},
            Action::Edit(command)=>{self.engine.edit(serde_json::to_string(&command).unwrap()).unwrap();},
            other=>panic!("unexpected numeric action: {other:?}"),
        } }
    }
    fn send(&mut self,input: Input) { let (_,actions)=self.frame(input); self.apply(actions); }
    fn click(&mut self) {
        let p=[(self.rect.min[0]+self.rect.max[0])*0.5,(self.rect.min[1]+self.rect.max[1])*0.5];
        self.send(press(p)); self.send(release(p));
        assert!(self.editing(),"{:?} floating={}: real numeric field click",self.provider,self.floating);
    }
    fn editing(&self)->bool { match self.provider {
        Provider::Effect=>self.panels.effect_controls.editing(),
        Provider::Transform=>self.panels.properties.field.is_some(),
    } }
    fn value(&self)->f64 {
        let state=self.engine.state().unwrap();
        match self.provider {
            Provider::Effect=>state["project"]["layers"][0]["effects"][0]["params"]["effect_opacity"]["track"]["value"][0].as_f64().unwrap(),
            Provider::Transform=>state["project"]["layers"][0]["transform"]["opacity"]["value"].as_f64().unwrap()*100.,
        }
    }
    fn project(&self)->Value { self.engine.state().unwrap()["project"].clone() }
    fn type_value(&mut self,text:&str) { self.send(Input { text:text.into(),..Default::default() }); }
}
fn cases(mut check:impl FnMut(&mut Harness)) {
    for locale in [Locale::En,Locale::Zh] { for floating in [false,true] {
        for provider in [Provider::Effect,Provider::Transform] {
            check(&mut Harness::new(provider,floating,locale));
        }
    } }
}

#[test]
fn valid_outside_click_commits_in_both_numeric_hosts_and_restores_history() {
    cases(|h| {
        let before=h.project();
        h.click(); h.type_value("12.3456"); h.send(press(h.outside));
        assert!(!h.editing());
        assert!((h.value()-12.3456).abs()<0.00001,"{:?}: {}",h.provider,h.value());
        let after=h.project();
        h.engine.save().unwrap();
        assert_eq!(serde_json::to_value(motion_core::storage::load(&h.folder.path().join("project")).unwrap()).unwrap(),after);
        assert_eq!(h.engine.history(0).unwrap()["project"],before);
        assert_eq!(h.engine.history(1).unwrap()["project"],after);
    });
}

#[test]
fn invalid_and_ime_outside_clicks_keep_drafts_without_model_changes() {
    cases(|h| {
        let before=h.project();
        h.click(); h.type_value("not-a-number"); h.send(press(h.outside));
        assert!(h.editing(),"invalid draft must remain available for correction");
        assert_eq!(h.project(),before);
        h.send(key(Key::Escape)); assert!(!h.editing());
        h.click(); h.type_value("42");
        for (composing,ime_consumed) in [(true,false),(false,true)] {
            h.send(Input { composing,ime_consumed,..press(h.outside) });
            assert!(h.editing(),"IME owns the commit frame"); assert_eq!(h.project(),before);
        }
        h.send(key(Key::Enter));
        assert!((h.value()-42.).abs()<0.00001);
        assert_eq!(h.engine.history(0).unwrap()["project"],before);
    });
}

#[test]
fn numeric_drafts_and_scrubs_cancel_without_losing_redo_or_crossing_locked_targets() {
    cases(|h| {
        h.click(); h.type_value("42"); h.send(key(Key::Enter));
        let after=h.project(); let before=h.engine.history(0).unwrap()["project"].clone();
        for input in [key(Key::Escape),Input { focus_lost:true,..Default::default() }] {
            h.click(); h.type_value("64"); h.send(input);
            assert!(!h.editing()); assert_eq!(h.project(),before);
            assert_eq!(h.engine.history(1).unwrap()["project"],after);
            assert_eq!(h.engine.history(0).unwrap()["project"],before);
        }
        h.click(); h.type_value("64");
        let mut locked=h.engine.state().unwrap(); locked["project"]["layers"][0]["locked"]=true.into();
        let mut paint=PaintList::default();
        let actions=if h.floating { panels::floating_chrome(&mut paint,&mut h.atlas,h.provider.panel(),SIZE,&mut h.panels,&locked,&key(Key::Enter)) }
            else {panels::chrome(&mut paint,&mut h.atlas,SIZE,&h.layout,&mut h.panels,&locked,&key(Key::Enter))};
        assert!(actions.is_empty(),"locked numeric target must not dispatch {actions:?}");
        assert!(!h.editing()); assert_eq!(h.project(),before);
        assert_eq!(h.engine.history(1).unwrap()["project"],after);
        assert_eq!(h.engine.history(0).unwrap()["project"],before);
        let p=[(h.rect.min[0]+h.rect.max[0])*0.5,(h.rect.min[1]+h.rect.max[1])*0.5];
        h.send(press(p)); h.send(Input { mouse:[p[0]-20.,p[1]],held:Some(MouseButton::Left),..Default::default() });
        assert!((h.value()-90.).abs()<0.00001);
        h.send(key(Key::Escape)); assert_eq!(h.project(),before);
        assert_eq!(h.engine.history(1).unwrap()["project"],after);
    });
}

#[test]
fn numeric_scrubs_share_threshold_modifiers_and_one_history_record() {
    cases(|h| {
        let before=h.project();
        let p=[(h.rect.min[0]+h.rect.max[0])*0.5,(h.rect.min[1]+h.rect.max[1])*0.5];
        let threshold=PaintList::default().styles().metric("motion.drag_threshold",3.);
        for (modifiers,expected) in [
            (Modifiers::default(),95.),
            (Modifiers { shift:true,..Default::default() },99.5),
            (Modifiers { control:true,..Default::default() },50.),
        ] {
            h.send(press(p));
            let (_,actions)=h.frame(Input { mouse:[p[0]-threshold*0.5,p[1]], held:Some(MouseButton::Left),..Default::default() });
            assert!(actions.is_empty()); assert_eq!(h.project(),before);
            for delta in [-5.,-10.] {
                h.send(Input { mouse:[p[0]+delta,p[1]],held:Some(MouseButton::Left),modifiers,..Default::default() });
            }
            h.send(Input { modifiers,..release([p[0]-10.,p[1]]) });
            assert!((h.value()-expected).abs()<0.00001,"{:?}: {}",h.provider,h.value());
            let after=h.project();
            assert_eq!(h.engine.history(0).unwrap()["project"],before);
            assert_eq!(h.engine.history(1).unwrap()["project"],after);
            h.engine.history(0).unwrap();
        }
    });
}

#[test]
fn empty_and_unchanged_numeric_drafts_preserve_redo() {
    cases(|h| {
        h.click(); h.type_value("42"); h.send(key(Key::Enter));
        let after=h.project(); let before=h.engine.history(0).unwrap()["project"].clone();
        for empty in [false,true] {
            h.click();
            if empty { h.send(key(Key::Backspace)); }
            h.send(press(h.outside));
            assert!(!h.editing()); assert_eq!(h.project(),before);
            assert_eq!(h.engine.history(1).unwrap()["project"],after);
            assert_eq!(h.engine.history(0).unwrap()["project"],before);
        }
    });
}

#[test]
fn numeric_close_faults_use_default_file_and_error_stderr_without_committing() {
    const CHILD:&str="MOTION_NUMERIC_CLOSE_OBSERVATION_CHILD";
    if let Some(root)=std::env::var_os(CHILD) {
        let root=std::path::PathBuf::from(root);
        let _logger=crate::logging::init(root.clone()).unwrap();
        let mut h=Harness::new(Provider::Effect,false,Locale::En);
        let before=h.project();
        h.click(); h.type_value("42");
        let healthy=h.engine.state().unwrap();
        let mut malformed=healthy.clone();
        malformed["project"]["layers"][0]["effects"][0]["params"]["effect_opacity"]["min"]="invalid".into();
        for _ in 0..120 {
            let mut actions=vec![];
            assert!(h.panels.effect_controls.numeric_outside(&malformed,&press(h.outside),None,&mut actions));
            assert!(actions.is_empty()); assert!(h.editing()); assert_eq!(h.project(),before);
        }
        // A valid projection recovers without discarding the user's finite draft.
        let mut actions=vec![];
        assert!(!h.panels.effect_controls.numeric_outside(&healthy,&press(h.outside),None,&mut actions));
        h.apply(actions); assert!((h.value()-42.).abs()<0.00001);
        assert_eq!(h.engine.history(0).unwrap()["project"],before);
        crate::logging::flush();
        let file=std::fs::read_to_string(root.join("motion-studio.log")).unwrap();
        for site in ["effects.numeric_close_parameter"] {
            let lines:Vec<_>=file.lines().filter(|l|l.contains(&format!("site={site} "))).collect();
            assert_eq!(lines.len(),2,"{file}");
            assert!(lines[0].contains("WARN")&&lines[0].contains("count=1"));
            assert!(lines[1].contains("ERROR")&&lines[1].contains("count=60"));
            assert!(lines.iter().all(|l|l.contains("object=7 effect=")&&l.contains("parameter=effect_opacity component=0")&&l.contains("reason=")));
        }
        return;
    }
    let temporary=tempfile::tempdir().unwrap();
    let root=std::env::var_os("MOTION_NUMERIC_OBSERVATION_ARTIFACTS").map(std::path::PathBuf::from)
        .unwrap_or_else(||temporary.path().to_owned());
    std::fs::create_dir_all(&root).unwrap();
    let output=std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::numeric::contract_tests::numeric_close_faults_use_default_file_and_error_stderr_without_committing","--nocapture"])
        .env(CHILD,&root).env_remove("MOTION_LOG").output().unwrap();
    std::fs::write(root.join("stdout.log"),&output.stdout).unwrap();
    std::fs::write(root.join("stderr.log"),&output.stderr).unwrap();
    assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    let stderr=String::from_utf8_lossy(&output.stderr);
    let stdout=String::from_utf8_lossy(&output.stdout);
    for site in ["effects.numeric_close_parameter"] {
        assert_eq!(stderr.lines().filter(|l|l.contains(&format!("site={site} "))).count(),1,"{stderr}");
        assert!(stderr.lines().filter(|l|l.contains(&format!("site={site} "))).all(|l|l.contains("ERROR")&&l.contains("count=60")));
        assert!(!stdout.contains(&format!("site={site}")));
    }
    assert!(!stdout.contains(" INFO [")&&!stdout.contains(" ERROR "));
}

#[cfg(target_os="windows")]
#[test]
#[ignore="requires actual native GPU windows; run explicitly"]
fn native_numeric_drafts_commit_and_cancel_in_two_windows_and_languages() {
    use std::{path::PathBuf,sync::Arc,time::Duration};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    let root=PathBuf::from(std::env::var_os("MOTION_NUMERIC_NATIVE_ARTIFACTS").expect("artifact directory"));
    std::fs::create_dir_all(&root).unwrap();
    let mut events=winit::event_loop::EventLoop::builder().with_any_thread(true).build().unwrap();
    #[allow(deprecated)]
    let windows:Vec<_>=(0..2).map(|_| Arc::new(events.create_window(
        winit::window::Window::default_attributes().with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(SIZE[0] as u32,SIZE[1] as u32))).unwrap())).collect();
    cases(|h| {
        crate::native_test_support::receive(&mut events,
            &h.engine.attach_async(windows[0].clone(),SIZE[0] as u32,SIZE[1] as u32).unwrap(),Duration::from_secs(30)).unwrap().unwrap();
        h.engine.float_window(1,windows[1].clone(),SIZE[0] as u32,SIZE[1] as u32).unwrap();
        let name=format!("{:?}-{}-{:?}",h.provider,if h.floating {"floating"}else{"main"},h.panels.locale);
        let mut capture=|h:&mut Harness,suffix:&str| {
            let (paint,actions)=h.frame(Input::default()); assert!(actions.is_empty());
            let config=h.atlas.config();
            let path=root.join(format!("{name}-{suffix}.png"));
            let frame=crate::engine::UiFrame { paint,
                atlas:Some(crate::engine::AtlasUpload { pixels:h.atlas.pixel_data().to_vec(),width:config.width,height:config.height }),
                logical_size:SIZE,composition:None,composition_image:None,window:u64::from(h.floating),node_preview:None };
            crate::native_test_support::capture_ui(&mut events,&h.engine,frame,path.clone()).unwrap();
            assert_eq!(image::open(path).unwrap().into_rgba8().dimensions(),(SIZE[0] as u32,SIZE[1] as u32));
        };
        let before=h.project();
        capture(h,"idle");
        h.click(); h.type_value("NaN"); h.send(press(h.outside));
        assert!(h.editing()); assert_eq!(h.project(),before);
        capture(h,"invalid");
        h.send(key(Key::Escape)); assert!(!h.editing()); assert_eq!(h.project(),before);
        h.click(); h.type_value("12.3456"); h.send(press(h.outside));
        assert!(!h.editing()); assert!((h.value()-12.3456).abs()<0.00001);
        capture(h,"committed");
        let after=h.project();
        assert_eq!(h.engine.history(0).unwrap()["project"],before);
        assert_eq!(h.engine.history(1).unwrap()["project"],after);
        h.engine.close_window(1).unwrap();
    });
}
