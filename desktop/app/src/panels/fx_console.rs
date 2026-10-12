//! Modal catalogue search using the same plugin command as the effects panel.
use crate::{
    i18n::Locale,
    panels::{Action, FxConsole},
};
use motion_ui::{
    input::{Input, Key, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    theme::palette,
    ui::{self, Align},
};
use serde_json::{json, Value};

struct Entry {
    name: String,
    category: String,
    command: Value,
}
fn entries(catalogue: &Value, query: &str, locale: Locale, object: u64) -> Vec<Entry> {
    let query = query.trim().to_lowercase();
    let mut result = vec![];
    for package in catalogue["packages"].as_array().into_iter().flatten() {
        let manifest = &package["manifest"];
        for effect in manifest["effects"].as_array().into_iter().flatten() {
            let id = effect["id"].as_str().unwrap_or("");
            let name = if locale == Locale::Zh {
                effect["name"].as_str()
            } else {
                effect["english_name"].as_str()
            }
            .or_else(|| effect["name"].as_str())
            .unwrap_or(id);
            let category = manifest["name"].as_str().unwrap_or("");
            if !query.is_empty()
                && ![
                    name,
                    id,
                    category,
                    effect["name"].as_str().unwrap_or(""),
                    effect["english_name"].as_str().unwrap_or(""),
                    effect["category"].as_str().unwrap_or(""),
                ]
                .iter()
                .any(|s| s.to_lowercase().contains(&query))
            {
                continue;
            }
            result.push(Entry {
                name: name.into(),
                category: category.into(),
                command: json!({
                    "op":"add", "object":object, "plugin":manifest["id"],
                    "version":manifest["version"], "hash":package["hash"], "effect":id
                }),
            });
        }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

pub fn render_fx_console(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    console: &mut FxConsole,
    catalogue: &Value,
    input: &Input,
    locale: Locale,
    selected_layer: Option<u64>,
    out: &mut Vec<Action>,
) -> bool {
    if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
        return false;
    }
    if !input.text.is_empty() {
        console.query.push_str(&input.text);
        console.selected_index = 0;
    }
    if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
        console.query.pop();
        console.selected_index = 0;
    }
    let items = entries(
        catalogue,
        &console.query,
        locale,
        selected_layer.unwrap_or(0),
    );
    console.selected_index = console.selected_index.min(items.len().saturating_sub(1));
    for (key, _) in &input.keys {
        match key {
            Key::Down => {
                console.selected_index =
                    (console.selected_index + 1).min(items.len().saturating_sub(1))
            }
            Key::Up => console.selected_index = console.selected_index.saturating_sub(1),
            _ => {}
        }
    }
    let count = items
        .len()
        .min(10)
        .min(((size[1] - 116.).max(0.) / 32.) as usize);
    let width = 450f32.min((size[0] - 20.).max(1.));
    let height = (count as f32 * 32. + 96.).min((size[1] - 20.).max(1.));
    let x = (console.position[0] - width * 0.5).clamp(0., (size[0] - width).max(0.));
    let y = (console.position[1] - 60.).clamp(0., (size[1] - height).max(0.));
    let area = Rect::new(x, y, width, height);
    list.glyphs.retain(|g| {
        g.position[0] + g.size[0] <= area.min[0]
            || g.position[0] >= area.max[0]
            || g.position[1] + g.size[1] <= area.min[1]
            || g.position[1] >= area.max[1]
    });
    paint::rect(list, area, palette::CHROME);
    paint::stroke(list, area, palette::ACCENT, 1.);
    let input_area = Rect::new(x + 8., y + 8., (width - 16.).max(1.), 36.);
    paint::rect(list, input_area, palette::SUNKEN);
    let placeholder = if locale == Locale::Zh {
        "搜索效果…"
    } else {
        "Search effects…"
    };
    ui::text(
        list,
        atlas,
        input_area.shrink(6.),
        if console.query.is_empty() {
            placeholder
        } else {
            &console.query
        },
        palette::TEXT,
        Align::Left,
    );
    let start = console
        .selected_index
        .saturating_sub(count.saturating_sub(1));
    let enter = motion_ui::input::KeyChord::new(Key::Enter, false, false, false).pressed(input);
    for (i, item) in items.iter().enumerate().skip(start).take(count) {
        let row = Rect::new(
            x + 8.,
            y + 52. + (i - start) as f32 * 32.,
            (width - 16.).max(1.),
            30.,
        );
        if i == console.selected_index || row.contains(input.mouse) {
            paint::rect(list, row, palette::ACCENT.with_alpha(0.25));
        }
        ui::text(
            list,
            atlas,
            row.shrink(6.),
            &item.name,
            palette::TEXT,
            Align::Left,
        );
        let click = input
            .pressed
            .is_some_and(|(b, p)| b == motion_ui::input::MouseButton::Left && row.contains(p));
        if selected_layer.is_some() && (click || (enter && i == console.selected_index)) {
            out.push(Action::Plugin(item.command.clone()));
            return false;
        }
    }
    let hint = match (locale, selected_layer.is_some(), items.is_empty()) {
        (Locale::Zh, false, _) => "请先选择可编辑图层",
        (Locale::En, false, _) => "Select an editable layer first",
        (Locale::Zh, _, true) => "没有匹配的效果",
        (Locale::En, _, true) => "No effects found",
        (Locale::Zh, _, _) => "↑↓ 选择 · Enter 添加 · Esc 关闭",
        _ => "↑↓ Select · Enter Apply · Esc Close",
    };
    ui::text(
        list,
        atlas,
        Rect::new(x + 8., area.max[1] - 28., (width - 16.).max(1.), 24.),
        hint,
        palette::TEXT_MUTED,
        Align::Left,
    );
    !input.pressed.is_some_and(|(_, p)| !area.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_applies_once_and_keeps_disabled_empty_or_repeated_enter_open() {
        let catalogue = json!({"packages":[{"hash":"abc", "manifest":{"id":"builtin","version":"1","name":"内置",
            "effects":[{"id":"blur","name":"高斯模糊","english_name":"Gaussian Blur"}]}}]});
        let mut atlas = TextAtlas::from_system(13.).unwrap();
        let mut console = FxConsole {
            position: [400., 250.],
            query: "Gaussian".into(),
            selected_index: 0,
            owner: None,
        };
        let mut out = vec![];
        let mut input = Input {
            keys: vec![(Key::Enter, Default::default())],
            ..Default::default()
        };
        assert!(render_fx_console(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut console,
            &catalogue,
            &input,
            Locale::Zh,
            None,
            &mut out
        ));
        assert!(out.is_empty());
        input.key_repeats = vec![true];
        assert!(render_fx_console(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut console,
            &catalogue,
            &input,
            Locale::Zh,
            Some(42),
            &mut out
        ));
        assert!(out.is_empty());
        input.key_repeats.clear();
        assert!(!render_fx_console(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut console,
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
        console.query = "missing".into();
        assert!(render_fx_console(
            &mut PaintList::default(),
            &mut atlas,
            [900., 600.],
            &mut console,
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
