use super::*;
pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let colors = styles.colors();

    paint::rect(list, area, colors.PANEL);
    let Some(layer) = auxiliary::selected(panels, state).cloned() else {
        label(
            list,
            atlas,
            area.shrink(10.),
            panels.locale.t(T::SelectLayer),
            true,
        );
        return;
    };
    let content = &layer["content"];
    let audio = if content["kind"] == "audio" {
        &content["audio"]
    } else {
        &content["video"]
    };
    if !matches!(content["kind"].as_str(), Some("audio" | "video")) {
        label(
            list,
            atlas,
            area.shrink(10.),
            auxiliary::localized(
                panels.locale,
                "选择音频或视频图层",
                "Select an audio or video layer",
            ),
            true,
        );
        return;
    }
    let object = layer["id"].as_u64().unwrap();
    let volume = audio["volume"].as_f64().unwrap_or(1.) as f32;
    let muted = audio["muted"].as_bool().unwrap_or(false);
    let enabled = layer["locked"] != true;
    let mut y = area.min[1] + 10.;
    auxiliary::line(
        list,
        atlas,
        area,
        &mut y,
        layer["name"].as_str().unwrap_or("Audio"),
        false,
    );
    auxiliary::line(
        list,
        atlas,
        area,
        &mut y,
        &format!(
            "{}  {:.0}%",
            auxiliary::localized(panels.locale, "音量", "Volume"),
            volume * 100.
        ),
        false,
    );
    for (text, next) in [("−", (volume - 0.1).max(0.)), ("+", (volume + 0.1).min(2.))] {
        button(
            list,
            atlas,
            Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
            text,
            input,
            enabled,
            Action::Edit(Command::SetAudio {
                object,
                volume: Some(next),
                muted: None,
            }),
            out,
        );
        y += 30.;
    }
    button(
        list,
        atlas,
        Rect::new(area.min[0] + 10., y, area.width() - 20., 26.),
        auxiliary::localized(
            panels.locale,
            if muted { "取消静音" } else { "静音" },
            if muted { "Unmute" } else { "Mute" },
        ),
        input,
        enabled,
        Action::Edit(Command::SetAudio {
            object,
            volume: None,
            muted: Some(!muted),
        }),
        out,
    );
}
