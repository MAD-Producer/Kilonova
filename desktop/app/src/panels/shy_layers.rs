//! Shy is authored list metadata; hiding is a persisted workspace projection.
use super::timeline_columns::ColumnRects;
use super::*;

#[cfg(test)]
mod tests;

/// Both inputs are top-to-bottom IDs. Resolve filtered insertion positions
/// against the complete stack, keeping hidden layers in their original order.
pub(super) fn reorder_index(
    full: &[u64],
    shown: &[u64],
    object: u64,
    slot: usize,
) -> Option<usize> {
    let from = full.iter().position(|id| *id == object)?;
    let target = match shown.get(slot) {
        Some(target) => full.iter().position(|id| id == target)?,
        None if slot == shown.len() => full.iter().position(|id| Some(id) == shown.last())? + 1,
        None => return None,
    };
    super::timeline::reorder_storage_index(full.len(), from, target)
}

pub(super) fn reconcile_selection(panels: &mut Panels, state: &Value) -> bool {
    if !panels.timeline.hide_shy {
        return false;
    }
    // A canvas/property selection may still target a shy layer. Only the
    // timeline's selection/navigation projection excludes hidden rows.
    if panels.focused.is_some_and(|panel| panel != Panel::Timeline) {
        return false;
    }
    let hidden = |id: u64| {
        state["project"]["layers"].as_array().is_some_and(|layers| {
            layers
                .iter()
                .any(|layer| layer["id"].as_u64() == Some(id) && layer["shy"] == true)
        })
    };
    let changed =
        panels.selected.is_some_and(hidden) || panels.selected_layers.iter().any(|id| hidden(*id));
    panels.selected_layers.retain(|id| !hidden(*id));
    if panels.selected.is_some_and(hidden) {
        let next = panels.selected_layers.last().copied();
        let retained = panels.selected_layers.clone();
        panels.select_only(next);
        panels.selected_layers = retained;
    }
    if panels.selection_anchor.is_some_and(hidden) {
        panels.selection_anchor = panels.selected;
    }
    if panels.timeline.reveal_layer.is_some_and(hidden) {
        panels.timeline.reveal_layer = None;
    }
    changed
}

pub(super) fn toolbar(
    list: &mut PaintList,
    rect: Rect,
    panels: &mut Panels,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let styles = list.styles();
    let mut states = motion_ui::style::States::default();
    if panels.timeline.hide_shy {
        states = states.with(motion_ui::style::State::Selected);
    }
    if motion_ui::icons::button(
        list,
        rect,
        motion_ui::icons::Icon::Minimize,
        input,
        &styles,
        states,
    )
    .clicked
    {
        out.extend(super::cancel_interaction(panels));
        out.push(Action::HideShyLayers(!panels.timeline.hide_shy));
    }
}

pub(super) fn header(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    ruler: Rect,
    locale: Locale,
) {
    if let Some(slot) = columns.shy {
        label(
            list,
            atlas,
            slot.rect(ruler.min[1], ruler.height()),
            if locale == Locale::Zh { "隐" } else { "Shy" },
            true,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn row(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    columns: &ColumnRects,
    object: u64,
    shy: bool,
    locked: bool,
    state: &Value,
    y: f32,
    height: f32,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let Some(slot) = columns.shy else {
        return;
    };
    let rect = slot.rect(y, height);
    if motion_ui::widgets::Checkbox::draw(
        list,
        atlas,
        rect,
        "",
        shy,
        input,
        object != 0 && !locked && state["observing"] != true,
        false,
        Some(motion_ui::icons::Icon::Minimize),
    )
    .clicked
    {
        let command = Command::SetLayerShy { object, shy: !shy };
        let command = match state["composition"]
            .as_str()
            .filter(|id| *id != "comp-main")
        {
            Some(composition) => Command::InComposition {
                composition: composition.into(),
                command: Box::new(command),
            },
            None => command,
        };
        out.push(Action::EditChecked {
            command,
            revision: state["revision"].as_u64().unwrap_or(0),
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn hint(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    toolbar: Rect,
    columns: &ColumnRects,
    ruler: Rect,
    bounds: Rect,
    input: &Input,
    locale: Locale,
    hidden: bool,
) {
    let text = if toolbar.contains(input.mouse) {
        match (locale, hidden) {
            (Locale::Zh, true) => "显示已标记图层",
            (Locale::Zh, false) => "隐藏已标记图层",
            (_, true) => "Show shy layers",
            (_, false) => "Hide shy layers",
        }
    } else if columns.shy.is_some_and(|slot| {
        slot.rect(ruler.min[1], bounds.max[1] - ruler.min[1])
            .contains(input.mouse)
    }) {
        if locale == Locale::Zh {
            "列表隐藏标记，不影响渲染"
        } else {
            "Shy: list organization only"
        }
    } else {
        return;
    };
    let styles = list.styles();
    let pad = styles.metric("padding.panel", 8.);
    let width = (atlas.measure(text) + pad * 2.).min(bounds.width());
    let height = styles.metric("row.height", 22.);
    let rect = Rect::new(
        input.mouse[0].min(bounds.max[0] - width).max(bounds.min[0]),
        (input.mouse[1] + height)
            .min(bounds.max[1] - height)
            .max(bounds.min[1]),
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
            rect.width() - pad * 2.,
            rect.height(),
        ),
        text,
        style,
        Align::Left,
    );
}
