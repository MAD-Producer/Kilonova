mod composition;
use crate::i18n::Locale;
use motion_ui::{
    input::{Input, Key, MouseButton, Rect},
    paint::{self, PaintList},
    text::TextAtlas,
    ui::{self, Align},
};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone)]
pub struct FontChoice {
    pub name: String,
    pub path: PathBuf,
    pub index: u32,
}
pub fn system_fonts() -> Vec<FontChoice> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    let mut faces: Vec<_> = db
        .faces()
        .filter_map(|f| {
            let path = match &f.source {
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path.clone(),
                _ => return None,
            };
            Some(FontChoice {
                name: format!(
                    "{} · {}",
                    f.families
                        .first()
                        .map(|n| n.0.as_str())
                        .unwrap_or(&f.post_script_name),
                    f.weight.0
                ),
                path,
                index: f.index,
            })
        })
        .collect();
    faces.sort_by(|a, b| a.name.cmp(&b.name));
    faces
}
pub fn imported_faces(path: &std::path::Path) -> Result<Vec<FontChoice>, String> {
    if path.metadata().map_err(|e| e.to_string())?.len()
        > motion_media::fonts::MAX_FONT_BYTES as u64
    {
        return Err("font exceeds 32 MiB".into());
    }
    let mut db = fontdb::Database::new();
    db.load_font_file(path).map_err(|e| e.to_string())?;
    let faces = db
        .faces()
        .map(|f| FontChoice {
            name: format!(
                "{} · {}",
                f.families
                    .first()
                    .map(|n| n.0.as_str())
                    .unwrap_or(&f.post_script_name),
                f.index
            ),
            path: path.to_path_buf(),
            index: f.index,
        })
        .collect::<Vec<_>>();
    if faces.is_empty() {
        return Err("font contains no supported faces".into());
    }
    Ok(faces)
}
pub enum Event {
    Settings(Value),
    CreateComposition(Value),
    Text {
        text: String,
        size: f32,
        width: u32,
        font: Option<FontChoice>,
    },
    ImportFont,
    Cancelled,
}
enum Kind {
    Settings,
    NewComposition,
    Text,
}
pub struct Dialog {
    kind: Kind,
    settings_tab: usize,
    preset_dropdown: motion_ui::widgets::Dropdown,
    preset_index: usize,
    aspect_locked: bool,
    aspect: f64,
    fields: Vec<String>,
    focus: usize,
    replace: bool,
    pub error: Option<String>,
    pub fonts: Vec<FontChoice>,
    font: Option<usize>,
    font_open: bool,
    font_scroll: usize,
}
fn word(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh { zh } else { en }.into()
}
fn button(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    input: &Input,
    rect: Rect,
    label: &str,
) -> bool {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(
        list,
        rect,
        if rect.contains(input.mouse) {
            colors.SUNKEN
        } else {
            colors.CHROME
        },
    );
    ui::text(
        list,
        atlas,
        rect.shrink(4.),
        label,
        colors.TEXT,
        Align::Center,
    );
    input
        .pressed
        .is_some_and(|(b, p)| b == MouseButton::Left && rect.contains(p))
}
impl Dialog {
    pub fn new_composition(width: u32, height: u32) -> Self {
        let mut dialog = Self::settings(
            &serde_json::json!({"project":{"name":"合成 1","width":width,"height":height,"fps":30,"frames":300}}),
        );
        dialog.kind = Kind::NewComposition;
        dialog
    }
    pub fn settings(state: &Value) -> Self {
        let p = &state["project"];
        Self {
            kind: Kind::Settings,
            settings_tab: 0,
            preset_dropdown: Default::default(),
            preset_index: 0,
            aspect_locked: false,
            aspect: 16. / 9.,
            fields: vec![
                p["name"].as_str().unwrap_or("").into(),
                p["width"].to_string(),
                p["height"].to_string(),
                p["fps"].to_string(),
                p["frames"].to_string(),
            ],
            focus: 0,
            replace: true,
            error: None,
            fonts: vec![],
            font: None,
            font_open: false,
            font_scroll: 0,
        }
    }
    pub fn text(fonts: Vec<FontChoice>) -> Self {
        let font = fonts
            .iter()
            .position(|f| f.name.contains("Microsoft YaHei") || f.name.contains("Noto Sans CJK"))
            .or_else(|| (!fonts.is_empty()).then_some(0));
        Self {
            kind: Kind::Text,
            settings_tab: 0,
            preset_dropdown: Default::default(),
            preset_index: 0,
            aspect_locked: false,
            aspect: 16. / 9.,
            fields: vec!["Motion Studio".into(), "64".into(), "1024".into()],
            focus: 0,
            replace: true,
            error: None,
            fonts,
            font,
            font_open: false,
            font_scroll: 0,
        }
    }
    pub fn add_font(&mut self, font: FontChoice) {
        self.font = Some(self.fonts.len());
        self.fonts.push(font);
    }
    pub fn render(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        size: [f32; 2],
        input: &Input,
        locale: Locale,
    ) -> Option<Event> {
        if !matches!(self.kind, Kind::Text) {
            return self.composition_ui(list, atlas, size, input, locale);
        }
        let styles = list.styles();
        let colors = styles.colors();

        if input.keys.iter().any(|(k, _)| *k == Key::Escape) {
            return Some(Event::Cancelled);
        }
        let card = Rect::new((size[0] - 500.) / 2., (size[1] - 410.) / 2., 500., 410.);
        paint::rect(
            list,
            Rect::new(0., 0., size[0], size[1]),
            colors.SUNKEN.with_alpha(0.76),
        );
        paint::rect(list, card, colors.PANEL);
        paint::stroke(list, card, colors.BORDER, 1.);
        let title = match self.kind {
            Kind::Settings | Kind::NewComposition => {
                word(locale, "合成设置", "Composition Settings")
            }
            Kind::Text => word(locale, "新建文字", "New Text Layer"),
        };
        ui::text(
            list,
            atlas,
            Rect::new(card.min[0] + 24., card.min[1] + 14., 452., 30.),
            &title,
            colors.TEXT,
            Align::Left,
        );
        let labels = match self.kind {
            Kind::Settings | Kind::NewComposition => vec![
                word(locale, "名称", "Name"),
                word(locale, "宽度", "Width"),
                word(locale, "高度", "Height"),
                word(locale, "帧率", "Frame Rate"),
                word(locale, "总帧数", "Duration in Frames"),
            ],
            Kind::Text => vec![
                word(locale, "文字", "Text"),
                word(locale, "字号", "Font Size"),
                word(locale, "文本宽度", "Text Width"),
            ],
        };
        for (i, (label, value)) in labels.iter().zip(self.fields.iter()).enumerate() {
            let y = card.min[1] + 58. + i as f32 * 44.;
            ui::text(
                list,
                atlas,
                Rect::new(card.min[0] + 24., y, 120., 32.),
                label,
                colors.TEXT_MUTED,
                Align::Left,
            );
            let field = Rect::new(card.min[0] + 156., y, 320., 32.);
            paint::rect(list, field, colors.SUNKEN);
            if self.focus == i {
                paint::stroke(list, field, colors.ACCENT, 1.);
            }
            ui::text(
                list,
                atlas,
                field.shrink(6.),
                value,
                colors.TEXT,
                Align::Left,
            );
            if input
                .pressed
                .is_some_and(|(b, p)| b == MouseButton::Left && field.contains(p))
            {
                self.focus = i;
                self.replace = true;
            }
        }
        if !input.text.is_empty() {
            if self.replace {
                self.fields[self.focus].clear();
                self.replace = false;
            }
            self.fields[self.focus].push_str(&input.text);
        }
        for (key, modifiers) in &input.keys {
            match key {
                Key::Tab => {
                    self.focus = if modifiers.shift {
                        (self.focus + self.fields.len() - 1) % self.fields.len()
                    } else {
                        (self.focus + 1) % self.fields.len()
                    };
                    self.replace = true;
                }
                Key::Backspace => {
                    if self.replace {
                        self.fields[self.focus].clear();
                        self.replace = false;
                    } else {
                        self.fields[self.focus].pop();
                    }
                }
                _ => {}
            }
        }
        if matches!(self.kind, Kind::Text) {
            let y = card.min[1] + 198.;
            let name = self
                .font
                .and_then(|i| self.fonts.get(i))
                .map(|f| f.name.as_str())
                .unwrap_or("IBM Plex Mono");
            if button(
                list,
                atlas,
                input,
                Rect::new(card.min[0] + 24., y, 320., 30.),
                name,
            ) {
                self.font_open = !self.font_open;
                self.font_scroll = self
                    .font
                    .unwrap_or(0)
                    .min(self.fonts.len().saturating_sub(5));
            }
            if button(
                list,
                atlas,
                input,
                Rect::new(card.min[0] + 354., y, 122., 30.),
                &word(locale, "导入字体", "Import Font"),
            ) {
                return Some(Event::ImportFont);
            }
        }
        if self.font_open {
            if input.wheel[1] > 0. {
                self.font_scroll = (self.font_scroll + 1).min(self.fonts.len().saturating_sub(5));
            } else if input.wheel[1] < 0. {
                self.font_scroll = self.font_scroll.saturating_sub(1);
            }
            for index in self.font_scroll..(self.font_scroll + 5).min(self.fonts.len()) {
                if button(
                    list,
                    atlas,
                    input,
                    Rect::new(
                        card.min[0] + 24.,
                        card.min[1] + 234. + (index - self.font_scroll) as f32 * 24.,
                        452.,
                        22.,
                    ),
                    &self.fonts[index].name,
                ) {
                    self.font = Some(index);
                    self.font_open = false;
                }
            }
            return None;
        }
        if let Some(error) = &self.error {
            ui::text(
                list,
                atlas,
                Rect::new(card.min[0] + 24., card.max[1] - 98., 452., 42.),
                error,
                colors.KEYFRAME,
                Align::Left,
            );
        }
        if button(
            list,
            atlas,
            input,
            Rect::new(card.max[0] - 240., card.max[1] - 46., 100., 30.),
            &word(locale, "取消", "Cancel"),
        ) {
            return Some(Event::Cancelled);
        }
        let apply = button(
            list,
            atlas,
            input,
            Rect::new(card.max[0] - 126., card.max[1] - 46., 100., 30.),
            &word(locale, "应用", "Apply"),
        ) || input.keys.iter().any(|(k, _)| *k == Key::Enter);
        if !apply {
            return None;
        }
        let result = (|| -> Result<Event, String> {
            match self.kind {
                Kind::Settings | Kind::NewComposition => {
                    let width = self.fields[1]
                        .parse::<u32>()
                        .map_err(|_| word(locale, "宽度必须为整数", "Width must be an integer"))?;
                    let height = self.fields[2]
                        .parse::<u32>()
                        .map_err(|_| word(locale, "高度必须为整数", "Height must be an integer"))?;
                    let fps = self.fields[3].parse::<u32>().map_err(|_| {
                        word(locale, "帧率必须为整数", "Frame rate must be an integer")
                    })?;
                    let frames = self.fields[4].parse::<u32>().map_err(|_| {
                        word(locale, "总帧数必须为整数", "Duration must be an integer")
                    })?;
                    motion_core::Project::new(width, height, fps, frames)
                        .map_err(|e| e.to_string())?;
                    let settings = serde_json::json!({"name":self.fields[0],"width":width,"height":height,"fps":fps,"frames":frames,"timing":"preserve_seconds","shorten":"reject"});
                    Ok(if matches!(self.kind, Kind::NewComposition) {
                        Event::CreateComposition(settings)
                    } else {
                        Event::Settings(settings)
                    })
                }
                Kind::Text => {
                    let size = self.fields[1]
                        .parse::<f32>()
                        .map_err(|_| word(locale, "字号无效", "Invalid font size"))?;
                    let width = self.fields[2]
                        .parse::<u32>()
                        .map_err(|_| word(locale, "文本宽度无效", "Invalid text width"))?;
                    if self.fields[0].is_empty()
                        || !size.is_finite()
                        || !(4. ..=256.).contains(&size)
                        || width == 0
                        || width > 4096
                    {
                        return Err(word(
                            locale,
                            "请检查文字、字号和宽度",
                            "Check text, font size and width",
                        ));
                    }
                    Ok(Event::Text {
                        text: self.fields[0].clone(),
                        size,
                        width,
                        font: self.font.and_then(|i| self.fonts.get(i)).cloned(),
                    })
                }
            }
        })();
        match result {
            Ok(event) => Some(event),
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }
}
