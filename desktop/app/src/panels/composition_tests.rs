use super::*;

#[test]
fn hidden_handles_leave_pen_creation_available_and_do_not_toggle_during_its_active_draft() {
    let folder=tempfile::tempdir().unwrap();let engine=crate::engine::Engine::start(folder.path().join("project"),motion_desktop_media::platform(),8<<30).unwrap();
    let state=engine.state().unwrap();let mut atlas=TextAtlas::from_system(18.).unwrap();
    for locale in [Locale::En,Locale::Zh] {
        let mut panels=Panels::default();panels.locale=locale;panels.focused=Some(Panel::Composition);let mut out=vec![];
        super::super::tools::choose(&mut panels,super::super::tools::Active::Pen,&state,&mut out);assert!(out.is_empty());
        panels.viewer_guides.visibility.handles=false;panels.viewer_guides.visibility.rulers=true;
        let area=Rect::new(0.,0.,900.,600.);render(&mut PaintList::default(),&mut atlas,area,&mut panels,&state,&Input::default(),&mut out);
        let view=styled_viewer(area,&PaintList::default().styles());let map=panels.composition.navigation.mapping(view,[1920.,1080.]).unwrap();let point=map.screen([600.,400.]);
        render(&mut PaintList::default(),&mut atlas,area,&mut panels,&state,&Input {mouse:point,pressed:Some((MouseButton::Left,point)),held:Some(MouseButton::Left),..Default::default()},&mut out);
        assert!(panels.path_controls.editing());assert_eq!(out.len(),1);
        let Some(Action::Seek(frame))=out.pop() else {panic!("new draft must keep the existing view-only seek action")};
        assert_eq!(frame,state["frame"].as_f64().unwrap());engine.seek(frame).unwrap();assert!(out.is_empty());
        let key=Input {keys:vec![(Key::H,motion_ui::input::Modifiers {control:true,shift:true,alt:false})],..Default::default()};
        super::super::keyboard::route(&key,&mut panels,&state,false,None,&mut out);assert!(!panels.viewer_guides.visibility.handles);assert!(out.is_empty());
        // The native window bridge cancels panel interactions before forwarding
        // FocusLost; the leaf composition renderer does not own that boundary.
        out.extend(super::super::cancel_interaction(&mut panels));
        render(&mut PaintList::default(),&mut atlas,area,&mut panels,&state,&Input {focus_lost:true,..Default::default()},&mut out);
        assert!(!panels.path_controls.editing());assert!(out.is_empty());assert_eq!(engine.state().unwrap()["project"],state["project"]);
    }
}

#[test]
fn collapsed_viewers_never_submit_a_fabricated_pixel_outside_the_window() {
    let styles = PaintList::default().styles();
    for width in [1.,8.,16.,17.,64.,900.,1600.] {
        for height in [1.,30.,52.,53.,80.,100.,101.,130.,600.,1000.] {
            let size = [width,height];
            let bounds = Rect::new(0.,0.,width,height);
            for root in [Node::dock(Panel::Composition.dock()),dock::editor_layout()] {
                let view = composition_view_styled(&root,size,&styles);
                if view.width() > 0. && view.height() > 0. {
                    assert!(bounds.contains_rect(view), "size={size:?} view={view:?}");
                }
            }
            if let Some(view) = floating_view_styled(Panel::Composition,size,&styles) {
                assert!(view.width() > 0. && view.height() > 0.);
                assert!(bounds.contains_rect(view), "floating size={size:?} view={view:?}");
            }
        }
    }
    assert!(floating_view_styled(Panel::Timeline,[900.,600.],&styles).is_none());
}

fn state() -> Value {
    serde_json::json!({"root":"navigation-fixture","revision":7,"frame":0,
        "project":{"composition_id":"main","width":1920,"height":1080,"fps":30,"layers":[]}})
}
fn draw(panels: &mut Panels, state: &Value, input: Input, atlas: &mut TextAtlas) -> Vec<Action> {
    let mut out = vec![];
    render(
        &mut PaintList::default(),
        atlas,
        Rect::new(0., 0., 900., 600.),
        panels,
        state,
        &input,
        &mut out,
    );
    out
}
#[test]
fn wheel_pan_cancel_fit_and_project_switch_emit_no_project_or_camera_commands() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut panels = Panels::default();
        panels.locale = locale;
        let state = state();
        let before = state.clone();
        assert!(draw(&mut panels, &state, Input::default(), &mut atlas).is_empty());
        let fit = panels.composition.navigation;
        let point = [320., 230.];
        assert!(draw(
            &mut panels,
            &state,
            Input {
                mouse: point,
                wheel: [0., 1.],
                ..Default::default()
            },
            &mut atlas
        )
        .is_empty());
        assert!(!panels.composition.navigation.is_fit());
        let zoom = panels.composition.navigation;
        for input in [
            Input {
                mouse: point,
                pressed: Some((MouseButton::Middle, point)),
                held: Some(MouseButton::Middle),
                ..Default::default()
            },
            Input {
                mouse: [370., 250.],
                held: Some(MouseButton::Middle),
                ..Default::default()
            },
        ] {
            assert!(draw(&mut panels, &state, input, &mut atlas).is_empty());
        }
        assert_ne!(panels.composition.navigation, zoom);
        assert!(draw(
            &mut panels,
            &state,
            Input {
                focus_lost: true,
                ..Default::default()
            },
            &mut atlas
        )
        .is_empty());
        assert_eq!(panels.composition.navigation, zoom);
        // Actual Fit hit target, computed from the same semantic sizes as the native control.
        let sheet = PaintList::default().styles();
        let fit_point = [
            sheet.metric("padding.panel", 8.)
                + sheet.metric("viewer.zoom.width", 64.)
                + sheet.metric("viewer.percent.width", 14.)
                + sheet.metric("viewer.controls.gap", 4.)
                + sheet.metric("viewer.fit.width", 40.) * 0.5,
            600. - sheet.metric("viewer.controls.height", 28.) * 0.5,
        ];
        assert!(draw(
            &mut panels,
            &state,
            Input {
                mouse: fit_point,
                pressed: Some((MouseButton::Left, fit_point)),
                ..Default::default()
            },
            &mut atlas
        )
        .is_empty());
        assert_eq!(panels.composition.navigation, fit);
        draw(
            &mut panels,
            &state,
            Input {
                mouse: point,
                wheel: [0., 1.],
                ..Default::default()
            },
            &mut atlas,
        );
        let mut switched = state.clone();
        switched["project"]["composition_id"] = "other".into();
        assert!(draw(&mut panels, &switched, Input::default(), &mut atlas).is_empty());
        assert!(panels.composition.navigation.is_fit());
        assert_eq!(state, before);
    }
}
#[test]
fn selection_uses_the_same_zoomed_panned_mapping_as_drawing() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let state = state();
    let mut panels = Panels::default();
    draw(&mut panels, &state, Input::default(), &mut atlas);
    let view = styled_viewer(
        Rect::new(0., 0., 900., 600.),
        &PaintList::default().styles(),
    );
    panels
        .composition
        .navigation
        .zoom_at(view, [1920., 1080.], [400., 250.], 1.317);
    panels
        .composition
        .navigation
        .pan(view, [1920., 1080.], [100., -50.]);
    let point = [370., 250.];
    let expected = panels
        .composition
        .navigation
        .mapping(view, [1920., 1080.])
        .unwrap()
        .local(point);
    let actions = draw(
        &mut panels,
        &state,
        Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            ..Default::default()
        },
        &mut atlas,
    );
    assert!(matches!(actions.as_slice(),[Action::ViewerPick{key,cell:0,point}] if *point==expected
        && *key==panels.viewer_views.key(&state)));
}

#[test]
fn malformed_mapping_is_bounded_visible_and_recovers_at_default_log_level() {
    const CHILD: &str = "MOTION_VIEWER_MAPPING_CHILD";
    if let Some(root) = std::env::var_os(CHILD) {
        let _logger = crate::logging::init(std::path::PathBuf::from(root)).unwrap();
        let mut panels = Panels::default();
        let mut atlas = TextAtlas::from_system(18.).unwrap();
        let good = state();
        let mut bad = good.clone();
        bad["project"]["width"] = 0.into();
        for _ in 0..90 { assert!(draw(&mut panels,&bad,Input::default(),&mut atlas).is_empty()); }
        assert_eq!(panels.composition.mapping_failures,61);
        assert!(draw(&mut panels,&good,Input::default(),&mut atlas).is_empty());
        assert_eq!(panels.composition.mapping_failures,0);
        for _ in 0..60 { assert!(draw(&mut panels,&bad,Input::default(),&mut atlas).is_empty()); }
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact","panels::composition::tests::malformed_mapping_is_bounded_visible_and_recovers_at_default_log_level"])
        .env(CHILD,root.path()).env_remove("MOTION_LOG").output().unwrap();
    assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
    let file = std::fs::read_to_string(root.path().join("motion-studio.log")).unwrap();
    let error = "viewer mapping unavailable operation=canvas_mapping";
    assert!(file.contains("logging initialized level=INFO"));
    assert_eq!(file.matches(error).count(),2);
    assert_eq!(String::from_utf8_lossy(&output.stderr).matches(error).count(),2);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(error));
}

#[test]
fn multiview_pan_rolls_back_on_escape_focus_clock_and_geometry_changes() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    for cancel in ["escape", "focus", "clock", "resize"] {
        let mut state = state();
        let mut panels = Panels::default();
        panels.viewer_views.settings.set_layout(crate::viewer_views::Layout::Four);
        let mut area = Rect::new(0.,0.,900.,600.);
        let mut render_input = |panels:&mut Panels,state:&Value,area:Rect,input:Input| {
            let mut out=vec![];
            render(&mut PaintList::default(),&mut atlas,area,panels,state,&input,&mut out);
            assert!(out.is_empty(),"navigation dispatched commands: {out:?}");
        };
        render_input(&mut panels,&state,area,Input::default());
        let tiles=panels.viewer_views.settings.layout().tiles(styled_viewer(area,&PaintList::default().styles()),2.).unwrap();
        let point=[(tiles[1].min[0]+tiles[1].max[0])*0.5,(tiles[1].min[1]+tiles[1].max[1])*0.5];
        let before=panels.viewer_views.settings.clone();
        render_input(&mut panels,&state,area,Input {mouse:point,pressed:Some((MouseButton::Middle,point)),held:Some(MouseButton::Middle),..Default::default()});
        let moved=[point[0]+25.,point[1]+18.];
        render_input(&mut panels,&state,area,Input {mouse:moved,held:Some(MouseButton::Middle),..Default::default()});
        assert_eq!(panels.viewer_views.settings.active(),1);
        assert_ne!(panels.viewer_views.settings.view(1).unwrap().navigation,before.view(1).unwrap().navigation,"{cancel}");
        let input=match cancel {
            "escape"=>Input {keys:vec![(Key::Escape,Default::default())],..Default::default()},
            "focus"=>{assert!(super::super::cancel_for_focus_loss(&mut panels).is_empty());Input {focus_lost:true,..Default::default()}},
            "clock"=>{state["frame"]=serde_json::json!(20.);Input::default()},
            "resize"=>{area=Rect::new(0.,0.,1000.,600.);Input::default()},
            _=>unreachable!(),
        };
        render_input(&mut panels,&state,area,input);
        assert_eq!(panels.viewer_views.settings.view(1).unwrap().navigation,before.view(1).unwrap().navigation,"{cancel}");
        assert!(panels.composition.pan.is_none());
        for i in [0,2,3] {assert_eq!(panels.viewer_views.settings.view(i),before.view(i),"{cancel} neighbour {i}");}
    }
}
