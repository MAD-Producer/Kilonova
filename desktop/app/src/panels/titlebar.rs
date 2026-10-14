use super::*;
pub(crate) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    size: [f32; 2],
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let sheet = list.styles();
    let colors = sheet.colors();
    let area = Rect::new(0., 0., size[0], metrics::TITLE_BAR);
    paint::rect(list, area, colors.APP_BACKGROUND);
    ui::text_sized(
        list,
        atlas,
        Rect::new(8., 0., 20., metrics::TITLE_BAR),
        "M",
        colors.ACCENT,
        Align::Left,
        18.,
    );
    let name = state["project"]["name"].as_str().unwrap_or("Motion Studio");
    ui::text_sized(
        list,
        atlas,
        Rect::new(36., 0., (size[0] - 150.).max(0.), metrics::TITLE_BAR),
        &format!("Motion Studio — {name}"),
        colors.TEXT,
        Align::Left,
        12.,
    );
    for (index, (label, action)) in [
        ("−", Action::WindowMinimize),
        ("□", Action::WindowMaximize),
        ("×", Action::WindowClose),
    ]
    .into_iter()
    .enumerate()
    {
        let rect = Rect::new(
            size[0] - 84. + index as f32 * 28.,
            0.,
            28.,
            metrics::TITLE_BAR,
        );
        if ui::button_styled(list, atlas, rect, label, input, true, &sheet, "button.icon").clicked {
            out.push(action);
        }
    }
    if clicked(
        input,
        Rect::new(0., 0., (size[0] - 84.).max(0.), metrics::TITLE_BAR),
    ) {
        out.push(Action::WindowDrag);
    }
}
