use super::*;

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.PANEL);
    let locale = panels.locale;
    let p = &state["project"];
    let raw_name = p["name"].as_str().unwrap_or(locale.t(T::Untitled));
    let name = if raw_name == "Untitled Project" {
        locale.t(T::Untitled)
    } else {
        raw_name
    };
    let thumb = Rect::new(area.min[0] + 8.0, area.min[1] + 16.0, 80.0, 48.0);
    paint::rect(list, thumb, colors.SUNKEN);
    paint::stroke(list, thumb, colors.BORDER, 1.0);
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
    let response = panels.project.search_field.draw(
        list,
        atlas,
        search,
        &mut panels.project.query,
        locale.t(T::Search),
        input,
        true,
    );
    panels.project.search = response.active;
    if response.changed {
        panels.project.asset_list.scroll.offset = 0.;
    }
    let header = Rect::new(area.min[0], search.max[1] + 4.0, area.width(), 28.0);
    paint::rect(list, header, colors.CHROME);
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
        paint::rect(list, row, colors.CHROME.with_alpha(0.45));
        paint::rect(
            list,
            Rect::new(row.min[0] + 6.0, row.min[1] + 9.0, 12.0, 14.0),
            colors.ACCENT,
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
    let filtered: Vec<_> = assets
        .iter()
        .filter_map(|(asset, kind)| {
            let layer = layers.iter().find(|l| {
                l["content"]["asset"] == asset["id"]
                    || l["content"]["audio"]["asset"] == asset["id"]
                    || l["content"]["video"]["asset"] == asset["id"]
            });
            let name = layer
                .and_then(|l| l["name"].as_str())
                .unwrap_or_else(|| asset["path"].as_str().unwrap_or(kind));
            name.to_lowercase()
                .contains(&panels.project.query.to_lowercase())
                .then_some((name, *kind, layer.and_then(|l| l["id"].as_u64())))
        })
        .collect();
    let labels: Vec<_> = filtered
        .iter()
        .map(|(name, kind, _)| format!("{name} · {kind}"))
        .collect();
    let items: Vec<_> = labels
        .iter()
        .map(|label| motion_ui::widgets::ListItem {
            label,
            depth: 0,
            expanded: None,
            enabled: true,
        })
        .collect();
    panels.project.asset_list.selected = filtered
        .iter()
        .position(|(_, _, id)| id.is_some() && *id == panels.selected);
    let list_area = Rect::new(
        area.min[0] + 8.,
        header.max[1] + 34.,
        (area.width() - 16.).max(0.),
        (footer.min[1] - header.max[1] - 34.).max(0.),
    );
    let response = panels.project.asset_list.draw(
        list,
        atlas,
        list_area,
        &items,
        input,
        panels.focused == Some(Panel::Project) && !panels.project.search,
    );
    if let Some(index) = response.selected {
        if let Some(id) = filtered[index].2 {
            panels.selected = Some(id);
        }
    }
    paint::rect(list, footer, colors.CHROME);
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
