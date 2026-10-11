//! Load and compile semantic styles once; state lookup never parses JSON or token references.
use crate::{
    paint,
    theme::Color,
    ui::{self, Align},
    PaintList, Rect, TextAtlas,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::Path, sync::OnceLock};
const MAX_BYTES: usize = 256 * 1024;
const MAX_STYLES: usize = 256;
const DEFAULT_SOURCE: &str = include_str!("../styles/dark.json");

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Selected,
    Hovered,
    Pressed,
    Focused,
    Invalid,
    Disabled,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct States(u8);
impl States {
    pub fn with(mut self, state: State) -> Self {
        self.0 |= 1 << (state as u8);
        self
    }
    pub fn contains(self, state: State) -> bool {
        self.0 & (1 << (state as u8)) != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedStyle {
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    pub border_width: f32,
    pub padding: f32,
    pub font_size: f32,
}
impl Default for ResolvedStyle {
    fn default() -> Self {
        Self {
            background: Color::rgba(0, 0, 0, 0.),
            foreground: Color::grey(220),
            border: Color::rgba(0, 0, 0, 0.),
            border_width: 0.,
            padding: 5.,
            font_size: 14.,
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum Size {
    Value(f32),
    Token(String),
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Patch {
    background: Option<String>,
    foreground: Option<String>,
    border: Option<String>,
    border_width: Option<Size>,
    padding: Option<Size>,
    font_size: Option<Size>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct Rule {
    extends: Option<String>,
    background: Option<String>,
    foreground: Option<String>,
    border: Option<String>,
    border_width: Option<Size>,
    padding: Option<Size>,
    font_size: Option<Size>,
    states: BTreeMap<State, Patch>,
}
impl Rule {
    fn patch(&self) -> Patch {
        Patch {
            background: self.background.clone(),
            foreground: self.foreground.clone(),
            border: self.border.clone(),
            border_width: self.border_width.clone(),
            padding: self.padding.clone(),
            font_size: self.font_size.clone(),
        }
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Tokens {
    font_family: String,
    colors: BTreeMap<String, String>,
    sizes: BTreeMap<String, f32>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    tokens: Tokens,
    styles: BTreeMap<String, Rule>,
}
#[derive(Clone)]
pub struct StyleSheet {
    document: Document,
    compiled: BTreeMap<String, [ResolvedStyle; 64]>,
}
impl StyleSheet {
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("stylesheet exceeds 256 KiB".into());
        }
        let document: Document =
            serde_json::from_slice(bytes).map_err(|e| format!("invalid stylesheet: {e}"))?;
        Self::compile(document)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut bytes = vec![];
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        Self::from_json(&bytes)
    }
    /// Patches inherit the bundled stylesheet; unknown properties and references still fail.
    pub fn with_overrides(&self, bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES {
            return Err("stylesheet override exceeds 256 KiB".into());
        }
        let override_value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if !override_value.is_object() {
            return Err("stylesheet override must be an object".into());
        }
        let mut base = serde_json::from_str::<serde_json::Value>(&self.to_json())
            .map_err(|e| e.to_string())?;
        merge(&mut base, override_value);
        let document = serde_json::from_value(base)
            .map_err(|e| format!("invalid stylesheet override: {e}"))?;
        Self::compile(document)
    }
    pub fn load_overrides(&self, path: &Path) -> Result<Self, String> {
        let mut bytes = vec![];
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        self.with_overrides(&bytes)
    }
    pub fn font_family(&self) -> &str {
        &self.document.tokens.font_family
    }
    pub fn base_font_size(&self) -> f32 {
        self.document.tokens.sizes["font.body"]
    }
    pub fn resolve(&self, class: &str, states: States) -> Option<ResolvedStyle> {
        self.compiled.get(class).map(|all| all[states.0 as usize])
    }
    /// Retains the current compiled sheet if the replacement fails to load or validate.
    pub fn reload(&mut self, path: &Path) -> Result<(), String> {
        let next = Self::load(path)?;
        *self = next;
        Ok(())
    }
    fn compile(document: Document) -> Result<Self, String> {
        if document.version != 1 {
            return Err("unsupported stylesheet version".into());
        }
        if document.styles.is_empty() || document.styles.len() > MAX_STYLES {
            return Err("stylesheet requires 1..256 styles".into());
        }
        for class in [
            "widget",
            "panel",
            "panel.header",
            "canvas",
            "canvas.content",
            "button",
            "field",
            "field.number",
            "selection",
            "label",
            "label.muted",
            "label.caption",
            "label.title",
            "error",
        ] {
            if !document.styles.contains_key(class) {
                return Err(format!("missing framework style {class}"));
            }
        }
        if document.tokens.font_family.is_empty() || document.tokens.font_family.len() > 128 {
            return Err("invalid UI font family".into());
        }
        if !document.tokens.sizes.contains_key("font.body") {
            return Err("missing font.body size token".into());
        }
        for (name, value) in &document.tokens.colors {
            valid_name(name)?;
            color(value)?;
        }
        for (name, value) in &document.tokens.sizes {
            valid_name(name)?;
            if !value.is_finite() || *value < 0. || *value > 512. {
                return Err(format!("invalid size token {name}"));
            }
        }
        let body = document.tokens.sizes["font.body"];
        if !(1.0..=128.0).contains(&body) {
            return Err("font.body must be 1..128 logical pixels".into());
        }
        let mut sheet = Self {
            document,
            compiled: BTreeMap::new(),
        };
        for name in sheet.document.styles.keys() {
            valid_name(name)?;
            let mut chain = vec![];
            sheet.chain(name, &mut vec![], &mut chain)?;
            let mut all = [ResolvedStyle::default(); 64];
            for (mask, style) in all.iter_mut().enumerate() {
                for rule in &chain {
                    sheet.apply(style, &rule.patch())?;
                }
                for state in [
                    State::Selected,
                    State::Hovered,
                    State::Pressed,
                    State::Focused,
                    State::Invalid,
                    State::Disabled,
                ] {
                    if States(mask as u8).contains(state) {
                        for rule in &chain {
                            if let Some(patch) = rule.states.get(&state) {
                                sheet.apply(style, patch)?;
                            }
                        }
                    }
                }
            }
            sheet.compiled.insert(name.clone(), all);
        }
        Ok(sheet)
    }
    fn chain<'a>(
        &'a self,
        name: &str,
        seen: &mut Vec<String>,
        out: &mut Vec<&'a Rule>,
    ) -> Result<(), String> {
        if seen.len() >= 64 || seen.iter().any(|s| s == name) {
            return Err(format!(
                "stylesheet inheritance cycle or excessive depth at {name}"
            ));
        }
        let rule = self
            .document
            .styles
            .get(name)
            .ok_or_else(|| format!("unknown style {name}"))?;
        seen.push(name.into());
        if let Some(parent) = &rule.extends {
            self.chain(parent, seen, out)?;
        }
        out.push(rule);
        seen.pop();
        Ok(())
    }
    fn apply(&self, style: &mut ResolvedStyle, patch: &Patch) -> Result<(), String> {
        let resolve_color = |value: &str| -> Result<Color, String> {
            if let Some(name) = value.strip_prefix("$colors.") {
                color(
                    self.document
                        .tokens
                        .colors
                        .get(name)
                        .ok_or_else(|| format!("unknown color token {name}"))?,
                )
            } else {
                color(value)
            }
        };
        let resolve_size = |value: &Size| -> Result<f32, String> {
            match value {
                Size::Value(v) => Ok(*v),
                Size::Token(name) => {
                    let key = name
                        .strip_prefix("$sizes.")
                        .ok_or_else(|| format!("invalid size reference {name}"))?;
                    self.document
                        .tokens
                        .sizes
                        .get(key)
                        .copied()
                        .ok_or_else(|| format!("unknown size token {key}"))
                }
            }
        };
        if let Some(v) = &patch.background {
            style.background = resolve_color(v)?;
        }
        if let Some(v) = &patch.foreground {
            style.foreground = resolve_color(v)?;
        }
        if let Some(v) = &patch.border {
            style.border = resolve_color(v)?;
        }
        if let Some(v) = &patch.border_width {
            style.border_width = resolve_size(v)?;
        }
        if let Some(v) = &patch.padding {
            style.padding = resolve_size(v)?;
        }
        if let Some(v) = &patch.font_size {
            style.font_size = resolve_size(v)?;
        }
        if !style.border_width.is_finite()
            || !(0.0..=32.).contains(&style.border_width)
            || !style.padding.is_finite()
            || !(0.0..=256.).contains(&style.padding)
            || !style.font_size.is_finite()
            || !(1.0..=128.).contains(&style.font_size)
        {
            return Err("invalid border, padding or font size".into());
        }
        Ok(())
    }
    fn to_json(&self) -> String {
        // Keep original reference forms when applying overrides, rather than flattening tokens.
        serde_json::to_string(&self.document).expect("validated stylesheet can serialize")
    }
}
pub fn default_stylesheet() -> &'static StyleSheet {
    static SHEET: OnceLock<StyleSheet> = OnceLock::new();
    SHEET.get_or_init(|| {
        StyleSheet::from_json(DEFAULT_SOURCE.as_bytes()).expect("bundled stylesheet must compile")
    })
}
fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
    {
        return Err(format!("invalid style or token name {name}"));
    }
    Ok(())
}
fn color(value: &str) -> Result<Color, String> {
    let hex = value
        .strip_prefix('#')
        .ok_or_else(|| format!("expected hex color, got {value}"))?;
    if !hex.bytes().all(|c| c.is_ascii_hexdigit()) || !matches!(hex.len(), 3 | 4 | 6 | 8) {
        return Err(format!("invalid hex color {value}"));
    }
    let full = if hex.len() <= 4 {
        hex.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        hex.into()
    };
    let parse = |i| u8::from_str_radix(&full[i..i + 2], 16).expect("validated hex pair");
    Ok(Color::rgba(
        parse(0),
        parse(2),
        parse(4),
        if full.len() == 8 {
            parse(6) as f32 / 255.
        } else {
            1.
        },
    ))
}
fn merge(base: &mut serde_json::Value, overrides: serde_json::Value) {
    match (base, overrides) {
        (serde_json::Value::Object(base), serde_json::Value::Object(next)) => {
            for (key, value) in next {
                if let Some(old) = base.get_mut(&key) {
                    merge(old, value);
                } else {
                    base.insert(key, value);
                }
            }
        }
        (base, value) => *base = value,
    }
}

pub fn box_style(list: &mut PaintList, area: Rect, style: ResolvedStyle) {
    paint::rect(list, area, style.background);
    paint::stroke(list, area, style.border, style.border_width);
}
pub fn text_style(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    style: ResolvedStyle,
    align: Align,
) {
    let mut content = PaintList::default();
    ui::text_sized(
        &mut content,
        atlas,
        area,
        text,
        style.foreground,
        align,
        style.font_size,
    );
    content.clip_to(area);
    list.append(content);
}
