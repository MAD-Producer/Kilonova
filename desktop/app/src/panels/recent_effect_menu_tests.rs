use super::*;
use serde_json::json;

#[test]
fn long_effect_names_keep_the_localized_target_count_visible_before_the_ellipsis() {
    for locale in [Locale::En, Locale::Zh] {
        let mut atlas = TextAtlas::from_system_language(12., locale == Locale::Zh).unwrap();
        let mut panels = panels(locale);
        let name = "VeryLongEffectName".repeat(40);
        let catalogue = std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue);
        catalogue["packages"][0]["manifest"]["effects"][0]["name"] = json!(name);
        catalogue["packages"][0]["manifest"]["effects"][0]["english_name"] = json!(name);
        let prefix = if locale == Locale::Zh {
            "最近 · 2 个图层："
        } else {
            "Recent · 2 layers: "
        };
        let caption = effect_recent::rows(&mut panels, &state())[0].0.clone();
        assert!(caption.starts_with(prefix));
        for size in [[900., 600.], [1600., 1000.]] {
            let mut list = PaintList::default();
            let mut out = vec![];
            menus::render(
                &mut list,
                &mut atlas,
                size,
                &mut panels,
                &Input::default(),
                &state(),
                &mut out,
            );
            assert!(out.is_empty());
            let row_y = metrics::TITLE_BAR + metrics::MENU_BAR + 4. + 64.;
            let actual = list
                .glyphs
                .iter()
                .filter(|glyph| glyph.position[1] >= row_y && glyph.position[1] < row_y + 30.)
                .collect::<Vec<_>>();
            let mut expected = PaintList::default();
            label(
                &mut expected,
                &mut atlas,
                Rect::new(0., row_y, 256., 30.),
                prefix,
                false,
            );
            assert_eq!(
                expected.glyphs.len(),
                prefix.chars().filter(|c| !c.is_whitespace()).count()
            );
            assert!(actual.len() > expected.glyphs.len());
            assert!(actual.len() < caption.chars().filter(|c| !c.is_whitespace()).count());
            for (actual, expected) in actual.iter().zip(&expected.glyphs) {
                assert_eq!(actual.uv, expected.uv);
                assert_eq!(actual.atlas_size, expected.atlas_size);
                assert_eq!(actual.size, expected.size);
                assert!(actual.size[0] > 0. && actual.size[1] > 0.);
            }
        }
    }
}

fn catalogue() -> std::sync::Arc<Value> {
    std::sync::Arc::new(
        json!({"packages":[{"enabled":true,"hash":"hash-a","manifest":{
        "id":"test.effects","version":"1.0.0","effects":[
            {"id":"blur","name":"模糊","english_name":"Blur","renderer":"image"},
            {"id":"glow","name":"辉光","english_name":"Glow","renderer":"image"}
        ]}}]}),
    )
}

fn state() -> Value {
    json!({"root":"recent-menu","composition":"main","project":{"layers":[
        {"id":7,"name":"One","content":{"kind":"solid"},"effects":[]},
        {"id":8,"name":"Two","content":{"kind":"vector"},"effects":[]}
    ]}})
}

fn panels(locale: Locale) -> Panels {
    let mut panels = Panels {
        locale,
        selected: Some(7),
        selected_layers: vec![7, 8],
        menu: Some(4),
        ..Default::default()
    };
    panels.effect_controls.catalogue = catalogue();
    panels.browser.record_recent("test.effects/blur".into());
    panels
}

fn click_row(atlas: &mut TextAtlas, locale: Locale, index: usize) -> Input {
    let x = 8.
        + [T::File, T::Edit, T::Composition, T::Layer]
            .iter()
            .map(|key| atlas.measure(locale.t(*key)) + 22.)
            .sum::<f32>();
    let point = [
        x + 50.,
        metrics::TITLE_BAR + metrics::MENU_BAR + 4. + index as f32 * 32. + 15.,
    ];
    Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    }
}

#[test]
fn the_effect_menu_applies_a_recent_entry_to_the_exact_selected_scope_in_both_locales() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        let mut panels = panels(locale);
        let mut out = vec![];
        let input = click_row(&mut atlas, locale, 2);
        menus::render(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut panels,
            &input,
            &state(),
            &mut out,
        );
        assert!(
            matches!(out.as_slice(),[Action::Plugin(request)] if
            request["op"]=="add" && request["objects"]==json!([7,8])
            && request["composition"]=="main" && request["plugin"]=="test.effects"
            && request["version"]=="1.0.0" && request["hash"]=="hash-a" && request["effect"]=="blur"),
            "recent effect row did not emit the existing atomic add protocol: {out:?}"
        );
        assert!(panels.menu.is_none());
    }
}

#[test]
fn recent_rows_refresh_for_history_order_locale_and_immutable_catalogue_changes() {
    let mut panels = panels(Locale::En);
    panels.browser.record_recent("test.effects/glow".into());
    let rows = effect_recent::rows(&mut panels, &state());
    assert!(rows[0].0.contains("Glow") && rows[0].0.contains("2 layer"));
    assert!(rows[1].0.contains("Blur"));
    let Action::Plugin(request) = effect_recent::apply(1, &mut panels, &state()).unwrap() else {
        panic!()
    };
    assert_eq!(request["effect"], "blur");
    assert!(effect_recent::rows(&mut panels, &state())[0]
        .0
        .contains("Blur"));
    panels.locale = Locale::Zh;
    assert!(effect_recent::rows(&mut panels, &state())[0]
        .0
        .contains("模糊"));
    let catalogue = std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue);
    catalogue["packages"][0]["hash"] = json!("hash-b");
    catalogue["packages"][0]["manifest"]["version"] = json!("2.0.0");
    catalogue["packages"][0]["manifest"]["effects"][0]["name"] = json!("更新后的模糊");
    assert!(effect_recent::rows(&mut panels, &state())[0]
        .0
        .contains("更新后的模糊"));
    let Action::Plugin(request) = effect_recent::apply(0, &mut panels, &state()).unwrap() else {
        panic!()
    };
    assert_eq!(request["hash"], "hash-b");
    assert_eq!(request["version"], "2.0.0");
    panels.selected = Some(8);
    panels.selected_layers = vec![8];
    let Action::Plugin(request) = effect_recent::apply(0, &mut panels, &state()).unwrap() else {
        panic!()
    };
    assert_eq!(
        request["objects"],
        json!([8]),
        "target scope must never be cached with metadata"
    );
}

#[test]
fn invalid_or_incompatible_targets_disable_the_whole_menu_scope() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    for locale in [Locale::En, Locale::Zh] {
        for case in 0..12 {
            let mut panels = panels(locale);
            let mut state = state();
            match case {
                0 => state["observing"] = json!(true),
                1 => state["project"]["layers"][1]["locked"] = json!(true),
                2 => state["project"]["layers"][1]["content"]["kind"] = json!("audio"),
                3 => state["project"]["layers"][1]["content"]["kind"] = json!("null"),
                4 => {
                    state["project"]["layers"].as_array_mut().unwrap().pop();
                }
                5 => {
                    panels.selected = None;
                    panels.selected_layers.clear();
                }
                6 => panels.selected = Some(999),
                7 => panels.selected_layers = vec![7, 8, 8],
                8 => {
                    panels.selected = None;
                    panels.selected_layers = vec![0];
                }
                9 => state["composition"] = Value::Null,
                10 => state["composition"] = json!(""),
                11 => state["project"]["layers"] = Value::Null,
                _ => unreachable!(),
            }
            let rows = effect_recent::rows(&mut panels, &state);
            assert_eq!(rows.len(), 1);
            assert!(!rows[0].2, "case {case} {locale:?}");
            let history = panels.browser.recent_keys().to_vec();
            assert!(effect_recent::apply(0, &mut panels, &state).is_none());
            let input = click_row(&mut atlas, locale, 2);
            let mut out = vec![];
            menus::render(
                &mut PaintList::default(),
                &mut atlas,
                [900., 600.],
                &mut panels,
                &input,
                &state,
                &mut out,
            );
            assert!(out.is_empty(), "case {case} {locale:?}");
            assert_eq!(
                panels.menu,
                Some(4),
                "disabled rows must not dismiss the menu"
            );
            assert_eq!(panels.browser.recent_keys(), history);
        }
    }
}

#[test]
fn unavailable_packages_and_unsupported_renderers_do_not_apply_or_erase_history() {
    let mut panels = panels(Locale::En);
    let history = panels.browser.recent_keys().to_vec();
    std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue)["packages"][0]["enabled"] =
        json!(false);
    assert!(effect_recent::rows(&mut panels, &state()).is_empty());
    assert!(effect_recent::apply(0, &mut panels, &state()).is_none());
    assert_eq!(panels.browser.recent_keys(), history);
    let catalogue = std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue);
    catalogue["packages"][0]["enabled"] = json!(true);
    catalogue["packages"][0]["manifest"]["effects"][0]["compatibility"] = json!("unsupported");
    assert!(!effect_recent::rows(&mut panels, &state())[0].2);
    assert!(effect_recent::apply(0, &mut panels, &state()).is_none());
    let catalogue = std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue);
    catalogue["packages"][0]["manifest"]["effects"][0]["compatibility"] = Value::Null;
    catalogue["packages"][0]["manifest"]["effects"][0]["renderer"] = json!("volume");
    let mut state = state();
    state["project"]["layers"][1]["content"]["kind"] = json!("adjustment");
    assert!(!effect_recent::rows(&mut panels, &state)[0].2);
    assert!(effect_recent::apply(0, &mut panels, &state).is_none());
    panels.browser.record_recent("missing/effect".into());
    assert_eq!(effect_recent::rows(&mut panels, &state).len(), 1);
}

#[test]
fn cancelling_or_losing_focus_never_changes_history_or_emits_a_command() {
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    for input in [
        Input {
            focus_lost: true,
            ..Default::default()
        },
        Input {
            pressed: Some((MouseButton::Left, [1., 400.])),
            mouse: [1., 400.],
            ..Default::default()
        },
    ] {
        let mut panels = panels(Locale::En);
        panels.browser.record_recent("test.effects/glow".into());
        let history = panels.browser.recent_keys().to_vec();
        let state = state();
        let original = state.clone();
        let mut out = vec![];
        menus::render(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut panels,
            &input,
            &state,
            &mut out,
        );
        assert!(out.is_empty());
        assert!(panels.menu.is_none());
        assert_eq!(panels.browser.recent_keys(), history);
        assert_eq!(state, original);
    }
}

#[test]
fn recent_history_remains_bounded_and_the_existing_small_window_scroll_reaches_its_last_item() {
    let mut panels = panels(Locale::En);
    let effects=(0..40).map(|n|json!({"id":format!("effect{n}"),"name":format!("效果{n}"),"english_name":format!("Effect {n}"),"renderer":"image"})).collect::<Vec<_>>();
    std::sync::Arc::make_mut(&mut panels.effect_controls.catalogue)["packages"][0]["manifest"]
        ["effects"] = json!(effects);
    for n in 0..40 {
        panels
            .browser
            .record_recent(format!("test.effects/effect{n}"));
    }
    let rows = effect_recent::rows(&mut panels, &state());
    assert_eq!(rows.len(), 24);
    assert!(rows[0].0.contains("Effect 39"));
    assert!(rows[23].0.contains("Effect 16"));
    let mut atlas = TextAtlas::from_system(18.).unwrap();
    let mut input = click_row(&mut atlas, Locale::En, 2);
    input.mouse[0] = input.mouse[0].min(70.);
    input.pressed = None;
    input.mouse[1] = 90.;
    input.wheel = [0., -1.];
    for _ in 0..40 {
        let mut out = vec![];
        menus::render(
            &mut PaintList::default(),
            &mut atlas,
            [320., 140.],
            &mut panels,
            &input,
            &state(),
            &mut out,
        );
        assert!(out.is_empty());
    }
    assert_eq!(panels.menu_scroll, 24);
    let point = [70., 111.];
    let input = Input {
        mouse: point,
        pressed: Some((MouseButton::Left, point)),
        ..Default::default()
    };
    let mut out = vec![];
    menus::render(
        &mut PaintList::default(),
        &mut atlas,
        [320., 140.],
        &mut panels,
        &input,
        &state(),
        &mut out,
    );
    assert!(matches!(out.as_slice(),[Action::Plugin(request)] if request["effect"]=="effect16"));
}
