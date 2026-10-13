use motion_ui::{
    style::{box_style, default_stylesheet, text_style, State, States, StyleSheet},
    theme::Color,
    ui::Align,
    PaintList, Rect, TextAtlas,
};
#[test]
fn shared_tokens_change_all_consumers_and_custom_classes_inherit_states() {
    let sheet=default_stylesheet().with_overrides(br##"{"tokens":{"colors":{"focus":"#0f8","panel":"#16191F"}},"styles":{"button.export":{"extends":"button","padding":12}}}"##).unwrap();
    let focused = sheet
        .resolve("field", States::default().with(State::Focused))
        .unwrap();
    assert_eq!(focused.border, Color::rgb(0, 255, 136));
    assert_eq!(
        sheet
            .resolve("field.number", States::default())
            .unwrap()
            .foreground,
        focused.border
    );
    assert_eq!(
        sheet
            .resolve("panel", States::default())
            .unwrap()
            .background,
        Color::rgb(22, 25, 31)
    );
    let custom = sheet
        .resolve("button.export", States::default().with(State::Hovered))
        .unwrap();
    assert_eq!(custom.padding, 12.);
    assert_eq!(
        custom.background,
        sheet
            .resolve("button", States::default().with(State::Hovered))
            .unwrap()
            .background
    );
}
#[test]
fn state_precedence_retains_focus_border_and_disabled_overrides_primary_background() {
    let sheet = default_stylesheet();
    let states = States::default()
        .with(State::Hovered)
        .with(State::Pressed)
        .with(State::Focused);
    let style = sheet.resolve("button", states).unwrap();
    assert_eq!(style.border, Color::rgb(97, 168, 245));
    assert_eq!(style.background, Color::rgb(48, 69, 89));
    let disabled = sheet
        .resolve("button.primary", states.with(State::Disabled))
        .unwrap();
    assert_eq!(disabled.background, Color::grey(35));
    assert_eq!(disabled.foreground, Color::grey(133));
    assert_eq!(disabled.border_width, 0.);
    assert_eq!(
        sheet
            .resolve(
                "field",
                States::default().with(State::Focused).with(State::Invalid)
            )
            .unwrap()
            .border,
        Color::rgb(225, 120, 112)
    );
}
#[test]
fn malformed_tokens_properties_cycles_and_sizes_are_rejected_before_install() {
    for patch in [
        r#"{"version":2}"#,
        r#"{"styles":{"field":{"border":"$colors.missing"}}}"#,
        r#"{"styles":{"field":{"extends":"field.number"}}}"#,
        r#"{"styles":{"button":{"padding":"$sizes.missing"}}}"#,
        r#"{"styles":{"button":{"font_size":0}}}"#,
        r#"{"styles":{"button":{"border_width":999}}}"#,
        r#"{"styles":{"button":{"unknown_property":4}}}"#,
        r##"{"tokens":{"colors":{"focus":"#aaQQ11"}}}"##,
        r#"{"tokens":{"sizes":{"font.body":-14}}}"#,
        r#"{"tokens":{"font_family":""}}"#,
        r#"{"styles":{"bad class":{"extends":"widget"}}}"#,
    ] {
        assert!(
            default_stylesheet()
                .with_overrides(patch.as_bytes())
                .is_err(),
            "{patch}"
        );
    }
    assert!(StyleSheet::from_json(&vec![b' '; 256 * 1024 + 1]).is_err());
}
#[test]
fn failed_reload_retains_the_complete_current_style_sheet() {
    let mut sheet = default_stylesheet().clone();
    let before = sheet.resolve("button", States::default()).unwrap();
    let path = std::env::temp_dir().join(format!("motion-bad-style-{}.json", std::process::id()));
    std::fs::write(&path, b"invalid json").unwrap();
    assert!(sheet.reload(&path).is_err());
    std::fs::remove_file(path).unwrap();
    assert_eq!(sheet.resolve("button", States::default()).unwrap(), before);
}
#[test]
fn font_sizes_change_glyph_geometry_and_keep_paint_inside_the_widget() {
    let mut atlas = TextAtlas::from_system(14.).unwrap();
    let area = Rect::new(10., 20., 100., 32.);
    let small = default_stylesheet()
        .resolve("label", States::default())
        .unwrap();
    let mut large = small;
    large.font_size = 28.;
    let mut a = PaintList::default();
    let mut b = PaintList::default();
    text_style(&mut a, &mut atlas, area, "AB", small, Align::Left);
    text_style(&mut b, &mut atlas, area, "AB", large, Align::Left);
    assert!(!a.glyphs.is_empty() && !b.glyphs.is_empty());
    assert!(b.glyphs[0].size[0] > a.glyphs[0].size[0]);
    for glyph in b.glyphs {
        assert!(area.contains_rect(Rect::new(
            glyph.position[0],
            glyph.position[1],
            glyph.size[0],
            glyph.size[1]
        )));
    }
    let style = default_stylesheet()
        .with_overrides(br##"{"styles":{"panel":{"background":"#123456"}}}"##)
        .unwrap()
        .resolve("panel", States::default())
        .unwrap();
    let mut paint = PaintList::default();
    box_style(&mut paint, area, style);
    assert_eq!(paint.colors[0], Color::rgb(18, 52, 86));
}

#[test]
fn panel_host_installs_styles_for_frame_and_nested_draw_helpers() {
    use motion_ui::{
        dock::DockId,
        interaction::Router,
        panel::{PanelFrame, PanelHost, PanelLayout, PanelView},
        registry::{Language, PanelDescriptor},
        ui,
    };
    struct Probe;
    impl PanelView for Probe {
        type Action = ();
        fn draw(&mut self, frame: &mut PanelFrame<'_>) -> Vec<()> {
            assert_eq!(frame.styles.color("panel"), Color::rgb(18, 52, 86));
            assert_eq!(frame.paint.styles().color("focus"), Color::rgb(0, 255, 136));
            let mut child = frame.paint.child();
            ui::button(
                &mut child,
                frame.atlas,
                frame.layout.bounds,
                "测试 / Test",
                &motion_ui::Input {
                    mouse: [-1., -1.],
                    ..Default::default()
                },
                true,
            );
            assert!(child.colors.contains(&Color::rgb(18, 52, 86)));
            frame.paint.append(child);
            vec![]
        }
    }
    let mut host = PanelHost::default();
    host.register(
        PanelDescriptor::new(DockId(91), "样式测试", "Style test"),
        Probe,
    )
    .unwrap();
    host.set_stylesheet(default_stylesheet().with_overrides(br##"{"tokens":{"colors":{"panel":"#123456","focus":"#00ff88","field":"#123456"}}}"##).unwrap());
    let mut atlas = TextAtlas::from_system(12.).unwrap();
    host.draw(
        PanelLayout {
            window: 1,
            panel: DockId(91),
            bounds: Rect::new(0., 0., 180., 24.),
            clip: Rect::new(0., 0., 180., 24.),
        },
        Language::English,
        &Default::default(),
        &Router::default(),
        &mut PaintList::default(),
        &mut atlas,
    );
}
