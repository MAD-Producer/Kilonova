use super::*;

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
) {
    paint::rect(list, area, palette::PANEL);
    let locale = panels.locale;
    let p = &state["project"];
    let raw_name = p["name"].as_str().unwrap_or(locale.t(T::Untitled));
    let name = if raw_name == "Untitled Project" {
        locale.t(T::Untitled)
    } else {
        raw_name
    };
    let thumb = Rect::new(area.min[0] + 8.0, area.min[1] + 16.0, 80.0, 48.0);
    paint::rect(list, thumb, palette::SUNKEN);
    paint::stroke(list, thumb, palette::BORDER, 1.0);
    let x = thumb.max[0] + 10.0;
    let width = (area.max[0] - x - 10.0).max(1.0);
    label(
        list,
        atlas,
        Rect::new(x, thumb.min[1], width, 24.0),
        name,
        false,
    );
    let fps = p["fps"].as_u64().unwrap_or(30) as u32;
    label(
        list,
        atlas,
        Rect::new(x, thumb.min[1] + 20.0, width, 22.0),
        &format!("{} × {}", p["width"], p["height"]),
        true,
    );
    label(
        list,
        atlas,
        Rect::new(x, thumb.min[1] + 36.0, width, 22.0),
        &format!("{} {}", fps, locale.t(T::Fps)),
        true,
    );
    label(
        list,
        atlas,
        Rect::new(
            area.min[0] + 12.0,
            area.min[1] + 80.0,
            area.width() - 24.0,
            22.0,
        ),
        &format!(
            "{} {}",
            locale.t(T::Duration),
            timecode(p["frames"].as_f64().unwrap_or(0.0), fps)
        ),
        true,
    );
    let search = Rect::new(
        area.min[0] + 12.0,
        area.min[1] + 108.0,
        area.width() - 24.0,
        28.0,
    );
    paint::rect(list, search, palette::SUNKEN);
    paint::stroke(
        list,
        search,
        if panels.project.search {
            palette::ACCENT
        } else {
            palette::BORDER
        },
        1.0,
    );
    let focus = clicked(input, search);
    if focus {
        panels.project.search = true;
    } else if input.pressed.is_some() {
        panels.project.search = false;
    }
    if panels.project.search {
        panels.project.query.push_str(&input.text);
        if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
            panels.project.query.pop();
        }
    }
    label(
        list,
        atlas,
        search.shrink(6.0),
        if panels.project.query.is_empty() {
            locale.t(T::Search)
        } else {
            &panels.project.query
        },
        panels.project.query.is_empty(),
    );
    let header = Rect::new(area.min[0], search.max[1] + 4.0, area.width(), 28.0);
    paint::rect(list, header, palette::CHROME);
    label(
        list,
        atlas,
        Rect::new(
            header.min[0] + 14.0,
            header.min[1],
            header.width() * 0.62,
            28.0,
        ),
        locale.t(T::SourceName),
        true,
    );
    label(
        list,
        atlas,
        Rect::new(
            header.min[0] + header.width() * 0.64,
            header.min[1],
            header.width() * 0.34,
            28.0,
        ),
        locale.t(T::Type),
        true,
    );
    if name
        .to_lowercase()
        .contains(&panels.project.query.to_lowercase())
    {
        let row = Rect::new(
            area.min[0] + 8.0,
            header.max[1] + 1.0,
            area.width() - 16.0,
            32.0,
        );
        paint::rect(list, row, palette::CHROME.with_alpha(0.45));
        paint::rect(
            list,
            Rect::new(row.min[0] + 6.0, row.min[1] + 9.0, 12.0, 14.0),
            palette::ACCENT,
        );
        label(
            list,
            atlas,
            Rect::new(
                row.min[0] + 26.0,
                row.min[1],
                row.width() * 0.59 - 26.0,
                32.0,
            ),
            name,
            false,
        );
        label(
            list,
            atlas,
            Rect::new(
                row.min[0] + row.width() * 0.64,
                row.min[1],
                row.width() * 0.34,
                32.0,
            ),
            locale.t(T::Composition),
            true,
        );
    }
    let footer = Rect::new(area.min[0], area.max[1] - 30.0, area.width(), 30.0);
    let layers = p["layers"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let assets: Vec<_> = [
        ("assets", "Image"),
        ("video_assets", "Video"),
        ("audio_assets", "Audio"),
    ]
    .into_iter()
    .flat_map(|(key, kind)| {
        p[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(move |a| (a, kind))
    })
    .collect();
    let mut asset_y = header.max[1] + 34.0;
    for (asset, kind) in &assets {
        if asset_y + 30.0 > footer.min[1] {
            break;
        }
        let layer = layers.iter().find(|l| {
            l["content"]["asset"] == asset["id"]
                || l["content"]["audio"]["asset"] == asset["id"]
                || l["content"]["video"]["asset"] == asset["id"]
        });
        let name = layer
            .and_then(|l| l["name"].as_str())
            .unwrap_or_else(|| asset["path"].as_str().unwrap_or(kind));
        if !name
            .to_lowercase()
            .contains(&panels.project.query.to_lowercase())
        {
            continue;
        }
        let row = Rect::new(area.min[0] + 8.0, asset_y, area.width() - 16.0, 30.0);
        if panels.selected.is_some() && layer.and_then(|l| l["id"].as_u64()) == panels.selected {
            paint::rect(list, row, palette::ACCENT.with_alpha(0.18));
        }
        label(
            list,
            atlas,
            Rect::new(row.min[0] + 8., asset_y, row.width() * 0.62, 30.),
            name,
            false,
        );
        label(
            list,
            atlas,
            Rect::new(
                row.min[0] + row.width() * 0.64,
                asset_y,
                row.width() * 0.34,
                30.,
            ),
            kind,
            true,
        );
        if clicked(input, row) {
            panels.selected = layer.and_then(|l| l["id"].as_u64());
        }
        asset_y += 30.0;
    }
    paint::rect(list, footer, palette::CHROME);
    label(
        list,
        atlas,
        footer.shrink(10.0),
        &format!(
            "{} {}   ·   {} {}",
            1 + assets.len(),
            locale.t(T::Items),
            p["layers"].as_array().map_or(0, Vec::len),
            locale.t(T::Layers)
        ),
        true,
    );
}
