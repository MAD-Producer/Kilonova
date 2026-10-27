//! Composition output switches share the timeline's measured icon slots.
use super::timeline_columns::ColumnRects;
use super::*;
use motion_core::LayerSwitch;

#[cfg(test)]
mod tests;

pub(super) fn available(state: &Value) -> bool {
    state["capabilities"]["layer_switches"]["supported"] == true
}

fn audio_supported(layer: &Value, state: &Value) -> bool {
    match layer["content"]["kind"].as_str() {
        Some("audio" | "composition") => true,
        Some("video") => state["project"]["video_assets"]
            .as_array()
            .is_some_and(|assets| {
                assets.iter().any(|asset| {
                    asset["id"] == layer["content"]["video"]["asset"]
                        && asset["audio_asset"].as_u64().is_some()
                })
            }),
        _ => false,
    }
}

pub(super) fn video_supported(layer: &Value) -> bool {
    layer["id"].as_u64().is_some_and(|id| id != 0)
        && layer["content"]["kind"]
            .as_str()
            .is_some_and(|kind| kind != "audio")
}

pub(super) fn header(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    ruler: Rect,
    locale: Locale,
) {
    for (index, slot) in columns.playback.iter().enumerate() {
        if let Some(slot) = slot {
            label(
                list,
                atlas,
                slot.rect(ruler.min[1], ruler.height()),
                if locale == Locale::Zh {
                    ["独", "效", "音"][index]
                } else {
                    ["S", "Fx", "A"][index]
                },
                true,
            );
        }
    }
}

pub(super) fn command(state: &Value, object: u64, switch: LayerSwitch, enabled: bool) -> Action {
    let command = Command::SetLayerSwitch {
        object,
        switch,
        enabled,
    };
    let composition = state["composition"]
        .as_str()
        .unwrap_or(motion_core::MAIN_COMPOSITION);
    Action::EditChecked {
        command: if composition == motion_core::MAIN_COMPOSITION {
            command
        } else {
            Command::InComposition {
                composition: composition.into(),
                command: Box::new(command),
            }
        },
        revision: state["revision"].as_u64().unwrap_or(0),
    }
}

pub(super) fn row(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    layer: &Value,
    state: &Value,
    y: f32,
    height: f32,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if !available(state) {
        return;
    }
    let Some(object) = layer["id"].as_u64().filter(|id| *id != 0) else {
        return;
    };
    let kind = layer["content"]["kind"].as_str().unwrap_or("");
    let editable =
        layer["locked"] != true && !input.focus_lost && !input.composing && !input.ime_consumed;
    for (index, slot) in columns.playback.iter().enumerate() {
        let Some(slot) = slot else {
            continue;
        };
        let (switch, checked, supported, icon) = match index {
            0 => (
                LayerSwitch::Solo,
                layer["solo"] == true,
                !matches!(kind, "camera" | "null" | ""),
                motion_ui::icons::Icon::Star,
            ),
            1 => (
                LayerSwitch::Effects,
                layer["effects_enabled"] == true,
                !matches!(kind, "camera" | "null" | "audio" | ""),
                motion_ui::icons::Icon::Fx,
            ),
            _ => {
                let source = &layer["content"][match kind {
                    "audio" => "audio",
                    "video" => "video",
                    _ => "clip",
                }];
                (
                    LayerSwitch::Audio,
                    source["muted"] == false,
                    audio_supported(layer, state),
                    motion_ui::icons::Icon::Volume,
                )
            }
        };
        let checked = checked && supported;
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            slot.rect(y, height),
            "",
            checked,
            input,
            supported && editable,
            false,
            Some(icon),
        )
        .clicked
        {
            out.push(command(state, object, switch, !checked));
        }
    }
}

pub(super) fn hint(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    ruler: Rect,
    bounds: Rect,
    input: &Input,
    locale: Locale,
) {
    let Some(index) = columns.playback.iter().position(|slot| {
        slot.is_some_and(|s| s.rect(ruler.min[1], ruler.height()).contains(input.mouse))
    }) else {
        return;
    };
    let text = if locale == Locale::Zh {
        [
            "独奏：当前合成仅输出标记图层（预览与导出）",
            "效果总开关：保留各效果原有开关",
            "音频开关：独立于画面可见性",
        ][index]
    } else {
        [
            "Solo: isolate marked layers in this composition for preview and export",
            "Effects bypass: preserve each effect's enabled state",
            "Audio: independent of video visibility",
        ][index]
    };
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let height = styles.metric("row.height", 22.);
    let width = (atlas.measure(text) + pad * 2.).min(bounds.width());
    let rect = Rect::new(
        input.mouse[0].min(bounds.max[0] - width).max(bounds.min[0]),
        ruler.max[1].min(bounds.max[1] - height).max(bounds.min[1]),
        width,
        height,
    );
    let style = hint_style(&styles);
    motion_ui::style::box_style(list, rect, style);
    motion_ui::style::text_style(
        list,
        atlas,
        Rect::new(
            rect.min[0] + pad,
            rect.min[1],
            (width - pad * 2.).max(0.),
            height,
        ),
        text,
        style,
        Align::Left,
    );
}

fn hint_style(styles: &motion_ui::style::StyleSheet) -> motion_ui::style::ResolvedStyle {
    // Process-site consecutive failures; an available tooltip resets the episode.
    // Reuse the existing bounded desktop counter (Warn first, Error at 60).
    static FAILURES: crate::fallback_observation::Counter =
        crate::fallback_observation::Counter::new();
    if let Some(style) = styles.resolve("tooltip", Default::default()) {
        FAILURES.success();
        style
    } else {
        FAILURES.failure(
            "layer_switch.tooltip",
            format_args!("style=tooltip fallback=label"),
        );
        // Every successfully constructed stylesheet requires and compiles label.
        styles
            .resolve("label", Default::default())
            .expect("validated framework label style")
    }
}
