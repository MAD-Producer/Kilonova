//! Acknowledgement for failed transitions without unsaved changes to decide on.
use super::Target;
use crate::i18n::Locale;
use motion_ui::{
    input::Key,
    paint::{self, PaintList},
    ui::{self, Align},
    widgets::ScrollArea,
    Input, Rect, TextAtlas,
};

fn wrap(atlas: &mut TextAtlas, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut used = 0.;
        for character in paragraph.chars() {
            let advance = atlas.measure(character.encode_utf8(&mut [0; 4]));
            if !line.is_empty() && used + advance > width {
                lines.push(std::mem::take(&mut line));
                used = 0.;
            }
            line.push(character);
            used += advance;
        }
        lines.push(line);
    }
    lines
}

pub(crate) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    input: &Input,
    locale: Locale,
    target: &Target,
    error: &str,
    scroll: &mut ScrollArea,
) -> bool {
    let sheet = list.styles();
    let pad = sheet.metric("padding.panel", 8.) * 2.;
    let row = sheet.metric("row.height", 22.);
    let button_height = sheet.metric("panel.header.height", 24.);
    let width = sheet
        .metric("dialog.notice.width", 480.)
        .min((size[0] - pad * 2.).max(0.));
    let height = sheet
        .metric("dialog.notice.max_height", 320.)
        .min((size[1] - pad * 2.).max(0.));
    let card = Rect::new(
        (size[0] - width) * 0.5,
        (size[1] - height) * 0.5,
        width,
        height,
    );
    paint::rect(
        list,
        Rect::new(0., 0., size[0], size[1]),
        sheet.color("shadow"),
    );
    paint::rect(list, card, sheet.color("panel"));
    paint::stroke(
        list,
        card,
        sheet.color("border"),
        sheet.metric("border", 1.),
    );
    let zh = locale == Locale::Zh;
    let title = match target {
        Target::Open(_) => {
            if zh {
                "无法打开工程"
            } else {
                "Could not open project"
            }
        }
        Target::Import(_) => {
            if zh {
                "无法导入工程"
            } else {
                "Could not import project"
            }
        }
        Target::New(_) => {
            if zh {
                "无法新建工程"
            } else {
                "Could not create project"
            }
        }
        Target::Close => {
            if zh {
                "无法关闭工程"
            } else {
                "Could not close project"
            }
        }
    };
    ui::text(
        list,
        atlas,
        Rect::new(card.min[0] + pad, card.min[1] + pad, width - pad * 2., row),
        title,
        sheet.color("error"),
        Align::Left,
    );
    let details = match target {
        Target::Open(path) | Target::Import(path) => format!(
            "{}\n{}\n\n{error}",
            if zh { "路径：" } else { "Path:" },
            path.display()
        ),
        _ => error.to_owned(),
    };
    let button = Rect::new(
        card.max[0] - pad - sheet.metric("dialog.notice.button_width", 96.),
        card.max[1] - pad - button_height,
        sheet.metric("dialog.notice.button_width", 96.),
        button_height,
    );
    let body = Rect::new(
        card.min[0] + pad,
        card.min[1] + pad * 2. + row,
        (width - pad * 2.).max(0.),
        (button.min[1] - card.min[1] - pad * 3. - row).max(0.),
    );
    // Reserve a padding slot for the shared scrollbar before wrapping, so a
    // long path never hides its last characters behind the scrollbar.
    let lines = wrap(atlas, &details, (body.width() - pad).max(0.));
    let frame = scroll.begin(list, body, lines.len() as f32 * row, input, true);
    let mut content = PaintList::default();
    content.stylesheet = list.stylesheet.clone();
    for (index, text) in lines.iter().enumerate() {
        let line = Rect::new(
            frame.content.min[0],
            frame.content.min[1] + index as f32 * row,
            frame.viewport.width(),
            row,
        );
        if line.max[1] > frame.viewport.min[1] && line.min[1] < frame.viewport.max[1] {
            ui::text(
                &mut content,
                atlas,
                line,
                text,
                sheet.color("text"),
                Align::Left,
            );
        }
    }
    content.clip_to(frame.viewport);
    list.append(content);
    ui::button(
        list,
        atlas,
        button,
        if zh { "确定" } else { "OK" },
        input,
        true,
    )
    .clicked
        || input
            .key_presses()
            .any(|(key, _, repeat)| !repeat && matches!(key, Key::Enter | Key::Escape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapping_preserves_unicode_paths_and_every_original_cause_character() {
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let details = format!(
            "E:/{}\n\n系统找不到指定的文件。 (os error 2)",
            "移动的工程文件夹/".repeat(40)
        );
        let lines = wrap(&mut atlas, &details, 180.);
        assert_eq!(lines.concat(), details.replace('\n', ""));
        assert!(lines.iter().all(|line| atlas.measure(line) <= 180.));
        assert!(lines.len() > 10);
    }
}
