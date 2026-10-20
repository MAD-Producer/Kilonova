//! Layer-level sampling switches share the timeline's layer row and column plan.
use super::timeline_columns::ColumnRects;
use super::*;
use motion_core::render_sampling::{FrameBlend, LayerSampling, Shutter};
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Visibility(pub [bool; 3]);
impl Default for Visibility {
    fn default() -> Self {
        Self([true; 3])
    }
}
pub(super) const NAMES: [&str; 3] = ["frame_blend", "motion_blur", "continuous_rasterization"];
pub(super) fn caption(locale: Locale, index: usize) -> &'static str {
    if locale == Locale::Zh {
        ["帧混合", "运动模糊", "连续栅格化"][index]
    } else {
        ["Frame blending", "Motion blur", "Continuous rasterization"][index]
    }
}
pub(super) fn available(state: &Value) -> bool {
    state["capabilities"]["layer_sampling"]["wgpu_preview"] == true
}
pub(super) fn header(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    ruler: Rect,
    locale: Locale,
) {
    for (index, slot) in columns.sampling.iter().enumerate() {
        if let Some(slot) = slot {
            label(
                list,
                atlas,
                slot.rect(ruler.min[1], ruler.height()),
                if locale == Locale::Zh {
                    ["帧", "模", "栅"][index]
                } else {
                    ["FB", "MB", "CR"][index]
                },
                true,
            );
        }
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
    let Some(object) = layer["id"].as_u64() else {
        return;
    };
    let sampling = if layer["sampling"].is_null() {
        LayerSampling::default()
    } else {
        let Some(value) = observe!(
            "sampling.settings",
            serde_json::from_value::<LayerSampling>(layer["sampling"].clone()),
            "object={object} revision={}",
            state["revision"]
        )
        .ok() else {
            return;
        };
        value
    };
    let kind = &layer["content"]["kind"];
    for (index, slot) in columns.sampling.iter().enumerate() {
        let Some(slot) = slot else {
            continue;
        };
        let supported = state["capabilities"]["layer_sampling"][NAMES[index]]["sources"]
            .as_array()
            .is_some_and(|sources| sources.contains(kind));
        let checked = match index {
            0 => sampling.frame_blend == FrameBlend::Linear,
            1 => sampling.motion_blur.is_some(),
            _ => sampling.continuous_rasterization,
        };
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            slot.rect(y, height),
            "",
            checked,
            input,
            supported && layer["locked"] != true && state["observing"] != true,
            false,
            None,
        )
        .clicked
        {
            let mut next = sampling;
            match index {
                0 => {
                    next.frame_blend = if checked {
                        FrameBlend::Hold
                    } else {
                        FrameBlend::Linear
                    }
                }
                1 => {
                    next.motion_blur = if checked {
                        None
                    } else {
                        Some(Shutter::default())
                    }
                }
                _ => next.continuous_rasterization = !checked,
            }
            let command = Command::SetLayerSampling {
                object,
                sampling: next,
            };
            let composition = state["composition"]
                .as_str()
                .unwrap_or(motion_core::MAIN_COMPOSITION);
            let command = if composition == motion_core::MAIN_COMPOSITION {
                command
            } else {
                Command::InComposition {
                    composition: composition.into(),
                    command: Box::new(command),
                }
            };
            out.push(Action::EditChecked {
                command,
                revision: state["revision"].as_u64().unwrap_or(0),
            });
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
    let Some(index) = columns.sampling.iter().position(|slot| {
        slot.is_some_and(|s| s.rect(ruler.min[1], ruler.height()).contains(input.mouse))
    }) else {
        return;
    };
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let height = styles.metric("row.height", 22.);
    let text = caption(locale, index);
    let width = (atlas.measure(text) + pad * 2.).min(bounds.width());
    let rect = Rect::new(
        input.mouse[0].min(bounds.max[0] - width).max(bounds.min[0]),
        ruler.max[1].min(bounds.max[1] - height).max(bounds.min[1]),
        width,
        height,
    );
    let style = styles.resolve("tooltip", Default::default()).unwrap();
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
