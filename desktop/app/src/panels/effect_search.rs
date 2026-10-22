//! Modal catalogue search using the same plugin command as the effects panel.
use crate::{
    i18n::Locale,
    panels::{Action, EffectSearch},
};
use motion_ui::{
    input::{Input, Key, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;

struct Entry {
    name: String,
    command: Value,
}
fn entries(catalogue: &Value, query: &str, locale: Locale, object: u64) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let mut result = vec![];
    for effect in super::effect_controls::entries(catalogue, locale)
        .into_iter()
        .filter(|e| e.supported && e.matches(&query))
    {
        result.push(Entry {
            name: effect.name.clone(),
            command: effect.command(object),
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn render_effect_search(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    search: &mut EffectSearch,
    catalogue: &Value,
    input: &Input,
    locale: Locale,
    selected_layer: Option<u64>,
    out: &mut Vec<Action>,
) -> bool {
    let styles = list.styles();
    let colors = styles.colors();
    if motion_ui::input::KeyChord::new(Key::Tab, false, false, false).pressed(input) {
        search.continuous = !search.continuous;
    }

    if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
        return false;
    }
    if !input.text.is_empty() {
        search.query.push_str(&input.text);
        search.selected_index = 0;
    }
    if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
        search.query.pop();
        search.selected_index = 0;
    }
    let items = entries(
        catalogue,
        &search.query,
        locale,
        selected_layer.unwrap_or(0),
    );
    search.selected_index = search.selected_index.min(items.len().saturating_sub(1));
    for (key, _) in &input.keys {
        match key {
            Key::Down => {
                search.selected_index =
                    (search.selected_index + 1).min(items.len().saturating_sub(1))
            }
            Key::Up => search.selected_index = search.selected_index.saturating_sub(1),
            _ => {}
        }
    }
    let pad = styles.metric("padding.panel", 8.);
    let gap = styles.metric("padding", 5.);
    let row_height = styles.metric("effect.search.row.height", 32.);
    let field_height = styles.metric("effect.search.field.height", 36.);
    let toggle_height = styles.metric("effect.secondary.height", 24.);
    let hint_height = styles.metric("row.height", 22.);
    let fixed_height = pad * 2. + field_height + gap * 2. + toggle_height + hint_height;
    let available_height = (size[1] - pad * 2.).max(0.);
    let count = items
        .len()
        .min(styles.metric("effect.search.max_rows", 10.) as usize)
        .min(((available_height - fixed_height).max(0.) / row_height) as usize);
    let width = styles
        .metric("effect.search.width", 450.)
        .min((size[0] - pad * 2.).max(0.));
    let height = (count as f32 * row_height + fixed_height).min(available_height);
    let x = (search.position[0] - width * 0.5).clamp(0., (size[0] - width).max(0.));
    let y = (search.position[1] - row_height * 2.).clamp(0., (size[1] - height).max(0.));
    let area = Rect::new(x, y, width, height);
    list.retain_glyphs(|g| {
        g.position[0] + g.size[0] <= area.min[0]
            || g.position[0] >= area.max[0]
            || g.position[1] + g.size[1] <= area.min[1]
            || g.position[1] >= area.max[1]
    });
    paint::rect(list, area, colors.CHROME);
    paint::stroke(list, area, colors.ACCENT, styles.metric("border", 1.));
    let input_area = Rect::new(x + pad, y + pad, (width - pad * 2.).max(0.), field_height);
    paint::rect(list, input_area, colors.SUNKEN);
    let placeholder = if locale == Locale::Zh {
        "搜索效果…"
    } else {
        "Search effects…"
    };
    ui::text(
        list,
        atlas,
        input_area.shrink(gap),
        if search.query.is_empty() {
            placeholder
        } else {
            &search.query
        },
        colors.TEXT,
        Align::Left,
    );
    let start = search
        .selected_index
        .saturating_sub(count.saturating_sub(1));
    let enter = motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
    for (i, item) in items.iter().enumerate().skip(start).take(count) {
        let row = Rect::new(
            x + pad,
            input_area.max[1] + gap + (i - start) as f32 * row_height,
            (width - pad * 2.).max(0.),
            styles.metric("effect.header.height", 30.),
        );
        if i == search.selected_index || row.contains(input.mouse) {
            paint::rect(list, row, styles.color("selected"));
        }
        ui::text(
            list,
            atlas,
            row.shrink(gap),
            &item.name,
            colors.TEXT,
            Align::Left,
        );
        let click = input
            .pressed
            .is_some_and(|(b, p)| b == motion_ui::input::MouseButton::Left && row.contains(p));
        if selected_layer.is_some() && (click || (enter && i == search.selected_index)) {
            out.push(Action::Plugin(item.command.clone()));
            return search.continuous;
        }
    }
    let toggle = Rect::new(
        x + pad,
        input_area.max[1] + gap + count as f32 * row_height,
        (width - pad * 2.).max(0.),
        toggle_height,
    );
    if motion_ui::widgets::Checkbox::draw(
        list,
        atlas,
        toggle,
        if locale == Locale::Zh {
            "连续添加（保持搜索打开）"
        } else {
            "Keep open to add more effects"
        },
        search.continuous,
        input,
        true,
        false,
        None,
    )
    .clicked
    {
        search.continuous = !search.continuous;
    }
    let hint = match (locale, selected_layer.is_some(), items.is_empty()) {
        (Locale::Zh, false, _) => "请先选择可编辑图层",
        (Locale::En, false, _) => "Select an editable layer first",
        (Locale::Zh, _, true) => "没有匹配的效果",
        (Locale::En, _, true) => "No effects found",
        (Locale::Zh, _, _) => "↑↓ 选择 · Enter 添加 · Tab 连续开关 · Esc 关闭",
        _ => "↑↓ Select · Enter Apply · Tab Keep open · Esc Close",
    };
    ui::text(
        list,
        atlas,
        Rect::new(
            x + pad,
            toggle.max[1] + gap,
            (width - pad * 2.).max(0.),
            hint_height,
        ),
        hint,
        colors.TEXT_MUTED,
        Align::Left,
    );
    !input.pressed.is_some_and(|(_, p)| !area.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn continuous_search_is_explicit_and_ignores_repeat_and_ime_confirmations() {
        let catalogue = json!({"packages":[{"hash":"abc", "manifest":{"id":"builtin","version":"1","name":"内置",
            "effects":[{"id":"blur","name":"模糊","english_name":"Blur"}]}}]});
        for locale in [Locale::En, Locale::Zh] {
            let mut atlas = TextAtlas::from_system(13.).unwrap();
            let mut search = EffectSearch {
                position: [200., 150.],
                query: "blur".into(),
                ..Default::default()
            };
            let mut out = Vec::new();
            let mut draw = |search: &mut EffectSearch, input: Input| {
                let mut actions = Vec::new();
                let open = render_effect_search(
                    &mut PaintList::default(),
                    &mut atlas,
                    [400., 300.],
                    search,
                    &catalogue,
                    &input,
                    locale,
                    Some(42),
                    &mut actions,
                );
                out.extend(actions);
                open
            };
            assert!(!search.continuous);
            assert!(draw(
                &mut search,
                Input {
                    keys: vec![(Key::Tab, Default::default())],
                    ..Default::default()
                }
            ));
            assert!(search.continuous);
            for (repeat, composing, ime_consumed) in [
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                assert!(draw(
                    &mut search,
                    Input {
                        keys: vec![(Key::Enter, Default::default())],
                        key_repeats: vec![repeat],
                        composing,
                        ime_consumed,
                        ..Default::default()
                    }
                ));
            }
            assert!(draw(
                &mut search,
                Input {
                    keys: vec![(Key::Enter, Default::default())],
                    ..Default::default()
                }
            ));
            assert_eq!(search.query, "blur");
            assert!(draw(&mut search, Input::default()));
            assert!(!draw(
                &mut search,
                Input {
                    keys: vec![(Key::Escape, Default::default())],
                    ..Default::default()
                }
            ));
            assert!(matches!(out.as_slice(), [Action::Plugin(command)] if command["object"] == 42));
        }
    }
    #[test]
    fn effect_search_applies_once_and_keeps_disabled_empty_or_repeated_enter_open() {
        let catalogue = json!({"packages":[{"hash":"abc", "manifest":{"id":"builtin","version":"1","name":"内置",
            "effects":[{"id":"blur","name":"高斯模糊","english_name":"Gaussian Blur"}]}}]});
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let mut search = EffectSearch {
            position: [400., 250.],
            query: "Gaussian".into(),
            selected_index: 0,
            owner: None,
            continuous: false,
        };
        let mut out = vec![];
        let mut input = Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        };
        assert!(render_effect_search(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut search,
            &catalogue,
            &input,
            Locale::Zh,
            None,
            &mut out
        ));
        assert!(out.is_empty());
        input.key_repeats = vec![true];
        assert!(render_effect_search(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut search,
            &catalogue,
            &input,
            Locale::Zh,
            Some(42),
            &mut out
        ));
        assert!(out.is_empty());
        input.key_repeats.clear();
        assert!(!render_effect_search(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut search,
            &catalogue,
            &input,
            Locale::Zh,
            Some(42),
            &mut out
        ));
        assert!(
            matches!(out.as_slice(), [Action::Plugin(value)] if value["object"] == 42 && value["effect"] == "blur")
        );
        out.clear();
        search.query = "missing".into();
        assert!(render_effect_search(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut search,
            &catalogue,
            &input,
            Locale::Zh,
            Some(42),
            &mut out
        ));
        assert!(out.is_empty());
    }
    #[test]
    fn chinese_ui_matches_english_name_and_category() {
        let catalogue = json!({"packages":[{"hash":"abc", "manifest":{"id":"builtin","version":"1","name":"内置",
            "effects":[{"id":"blur","name":"高斯模糊","english_name":"Gaussian Blur","category":"blur-sharpen"}]}}]});
        assert_eq!(entries(&catalogue, "Gaussian", Locale::Zh, 42).len(), 1);
        assert_eq!(entries(&catalogue, "blur-sharpen", Locale::Zh, 42).len(), 1);
        assert!(entries(&catalogue, "missing", Locale::Zh, 42).is_empty());
    }
    #[test]
    fn search_preserves_package_identity_for_shared_plugin_command() {
        let catalogue = json!({"packages":[{"hash":"abc", "manifest":{"id":"builtin","version":"1.0.0","name":"内置",
            "effects":[{"id":"blur","name":"模糊","english_name":"Blur"}]}}]});
        let found = entries(&catalogue, "模糊", Locale::En, 42);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Blur");
        assert_eq!(
            found[0].command,
            json!({"op":"add","object":42,"plugin":"builtin","version":"1.0.0","hash":"abc","effect":"blur"})
        );
    }
}
