use super::*;

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
    assert!(matches!(actions.as_slice(),[Action::SelectAt(value)] if *value == expected));
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
