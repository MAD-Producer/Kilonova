use motion_ui::{
    dock::DockId,
    interaction::Router,
    paint::Shape,
    panel::{PanelFrame, PanelHost, PanelLayout, PanelView},
    registry::{Language, PanelDescriptor},
    style::default_stylesheet,
    tabs::{self, Event, Tab},
    Color, Input, MouseButton, PaintList, Rect, TextAtlas,
};

fn tab(language: Language) -> Tab<'static> {
    Tab {
        title: if language == Language::Chinese {
            "项目"
        } else {
            "Project"
        },
        context: None,
        badge: None,
        locked: false,
        closable: false,
    }
}

struct Header;
impl PanelView for Header {
    type Action = Event;

    fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<Event> {
        let input = frame.input_for(0);
        tabs::draw(
            frame.paint,
            frame.atlas,
            frame.layout.bounds,
            &[tab(frame.language)],
            0,
            &input,
            false,
            frame.styles,
        )
        .into_iter()
        .collect()
    }
}

fn menu_mesh(list: &PaintList) -> (Vec<[f32; 2]>, Color) {
    let meshes = list
        .shapes
        .iter()
        .zip(&list.colors)
        .filter_map(|(shape, color)| match shape {
            Shape::Mesh(mesh) => Some((mesh.points().collect::<Vec<_>>(), *color)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(meshes.len(), 1, "fixture has exactly one menu icon");
    meshes.into_iter().next().unwrap()
}

#[test]
fn direct_tab_call_uses_the_explicit_sheet_instead_of_the_list_fallback() {
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    let sheet = default_stylesheet()
        .with_overrides(br##"{"styles":{"icon":{"foreground":"#ff0000"}}}"##)
        .unwrap();
    let mut list = PaintList::default();
    assert!(tabs::draw(
        &mut list,
        &mut atlas,
        Rect::new(0., 0., 304., 26.),
        &[tab(Language::English)],
        0,
        &Input::default(),
        false,
        &sheet,
    )
    .is_none());
    assert_eq!(menu_mesh(&list).1, Color::rgb(255, 0, 0));
    let mut control = PaintList::default();
    assert!(tabs::draw(
        &mut control,
        &mut atlas,
        Rect::new(0., 0., 304., 26.),
        &[tab(Language::English)],
        0,
        &Input::default(),
        false,
        default_stylesheet(),
    )
    .is_none());
    assert_eq!(menu_mesh(&control).0, menu_mesh(&list).0);
    assert_ne!(menu_mesh(&control).1, menu_mesh(&list).1);
}

#[test]
fn panel_host_theme_reload_reaches_tab_menu_without_moving_its_click_target() {
    let mut host = PanelHost::default();
    host.register(PanelDescriptor::new(DockId(1), "标题", "Header"), Header)
        .unwrap();
    let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
    for window in [0, 3] {
        for language in [Language::Chinese, Language::English] {
            for width in [100., 304., 800.] {
                let area = Rect::new(13., 17., width, 26.);
                let layout = PanelLayout {
                    window,
                    panel: DockId(1),
                    bounds: area,
                    clip: area,
                };
                let regions = host.hit_regions(&[layout]);
                let input = Input::default();
                let mut router = Router::default();
                router.begin_frame(window, &input, &regions);
                host.set_stylesheet(default_stylesheet().clone());
                let mut original = PaintList::default();
                assert!(host
                    .draw(layout, language, &input, &router, &mut original, &mut atlas)
                    .is_empty());
                let (points, original_color) = menu_mesh(&original);
                assert!(!points.is_empty());
                for (overrides, expected) in [
                    (
                        br##"{"styles":{"icon":{"foreground":"#ff0000"}}}"##.as_slice(),
                        Color::rgb(255, 0, 0),
                    ),
                    (
                        br##"{"styles":{"icon":{"foreground":"#0000ff"}}}"##.as_slice(),
                        Color::rgb(0, 0, 255),
                    ),
                ] {
                    assert_ne!(
                        original_color, expected,
                        "default icon is a negative control"
                    );
                    host.set_stylesheet(default_stylesheet().with_overrides(overrides).unwrap());
                    let mut reloaded = PaintList::default();
                    assert!(host
                        .draw(layout, language, &input, &router, &mut reloaded, &mut atlas)
                        .is_empty());
                    let (reloaded_points, actual) = menu_mesh(&reloaded);
                    assert_eq!(
                        actual, expected,
                        "window={window} language={language:?} width={width}"
                    );
                    assert_eq!(
                        reloaded_points, points,
                        "theme reload must not move the menu"
                    );
                    assert!(reloaded_points.iter().all(|p| area.contains(*p)));

                    let menu = tabs::layouts(&mut atlas, area, &[tab(language)], 0)[0]
                        .menu
                        .unwrap();
                    let point = [
                        (menu.min[0] + menu.max[0]) * 0.5,
                        (menu.min[1] + menu.max[1]) * 0.5,
                    ];
                    let click = Input {
                        mouse: point,
                        pressed: Some((MouseButton::Left, point)),
                        held: Some(MouseButton::Left),
                        ..Default::default()
                    };
                    router.begin_frame(window, &click, &regions);
                    assert_eq!(
                        host.draw(
                            layout,
                            language,
                            &click,
                            &router,
                            &mut PaintList::default(),
                            &mut atlas
                        ),
                        vec![Event::Menu(0)],
                    );
                    router.end_frame(&Input {
                        released: Some((MouseButton::Left, point)),
                        ..Default::default()
                    });
                }
                host.set_stylesheet(default_stylesheet().clone());
                let mut restored = PaintList::default();
                assert!(host
                    .draw(layout, language, &input, &router, &mut restored, &mut atlas)
                    .is_empty());
                assert_eq!(menu_mesh(&restored), (points, original_color));
            }
        }
    }
}
