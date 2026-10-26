use super::*;
use motion_ui::style::{box_style, text_style, State, States};

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
    let pad = styles.metric("padding.panel", 8.);
    let border = styles.metric("border", 1.);
    let composition_height = styles.metric("project.composition.height", 32.);
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
        panels.media_assets.reset_project_scroll();
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
            composition_height,
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
                composition_height,
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
                composition_height,
            ),
            locale.t(T::Composition),
            true,
        );
        if right_clicked(input, row) {
            panels.media_assets.cancel();
            panels.project.context_owner = panels.media_assets.window();
            panels.project.context_menu = Some(input.mouse);
        }
    }
    let footer = Rect::new(area.min[0], area.max[1] - 30.0, area.width(), 30.0);
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
    let list_area = Rect::new(
        area.min[0] + pad, header.max[1] + composition_height + border * 2.,
        (area.width() - pad * 2.).max(0.),
        (footer.min[1] - header.max[1] - composition_height - border * 2.).max(0.),
    );
    let query = panels.project.query.clone();
    super::media_assets::render_list(list, atlas, list_area, panels, state, input, Panel::Project, &query);
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

fn pword(locale: Locale, zh: &str, en: &str) -> String {
    if locale == Locale::Zh {
        zh.into()
    } else {
        en.into()
    }
}

pub(super) fn finish_context_menu(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    bounds: Rect,
    panels: &mut Panels,
    input: &Input,
    locale: Locale,
    owner: Option<Panel>,
    out: &mut Vec<Action>,
) {
    if panels.project.context_owner != owner { return; }
    let Some(anchor) = panels.project.context_menu else {
        return;
    };
    let styles = list.styles();
    let entries = [("合成设置...", "Composition Settings...", true, Some(Action::CompositionSettings), false)];
    let row = styles.metric("context.row.height", 20.);
    let pad = styles.metric("padding", 5.);
    let width = styles.metric("context.menu.width", 220.).min(bounds.width());
    let height = entries.len() as f32 * row + pad * 2.;
    let card = Rect::new(
        anchor[0].clamp(bounds.min[0], (bounds.max[0] - width).max(bounds.min[0])),
        (anchor[1] - height).clamp(bounds.min[1], (bounds.max[1] - height).max(bounds.min[1])),
        width,
        height,
    );
    if input.focus_lost
        || input.keys.iter().any(|(key, _)| *key == Key::Escape)
        || input
            .pressed
            .is_some_and(|(_, point)| !card.contains(point))
    {
        panels.project.context_menu = None;
        return;
    }
    box_style(
        list,
        card,
        styles.resolve("context_menu", States::default()).unwrap(),
    );
    let mut chosen = None;
    for (index, (zh, en, enabled, _action, _arrow)) in entries.iter().enumerate() {
        let area = Rect::new(
            card.min[0] + pad,
            card.min[1] + pad + index as f32 * row,
            width - pad * 2.,
            row,
        );
        let mut states = States::default();
        if *enabled && area.contains(input.mouse) {
            states = states.with(State::Selected);
        }
        if !*enabled {
            states = states.with(State::Disabled);
        }
        let style = styles.resolve("context_menu.row", states).unwrap();
        box_style(list, area, style);
        text_style(
            list,
            atlas,
            Rect::new(
                area.min[0] + pad,
                area.min[1],
                area.width() - pad * 2.,
                area.height(),
            ),
            &pword(locale, zh, en),
            style,
            Align::Left,
        );
        if *enabled
            && input
                .pressed
                .is_some_and(|(button, point)| button == MouseButton::Left && area.contains(point))
        {
            chosen = Some(index);
        }

    }
    if let Some(index) = chosen {
        panels.project.context_menu = None;
        if let Some(action) = entries[index].3.as_ref() {
            match action {
                Action::CompositionSettings => out.push(Action::CompositionSettings),
                Action::ShowPanel(dock) => out.push(Action::ShowPanel(*dock)),
                _ => {}
            }
        }
    }
}
