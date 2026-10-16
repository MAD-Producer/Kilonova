//! Mask properties share layer rows, columns and time mapping with transforms.
use super::*;
use crate::panels::timeline::{property_group, Row};
use crate::panels::{
    types::{PropertyFilter, TimelineState},
    Panels,
};
use motion_ui::dock::Panel;

pub(in crate::panels) fn key_frames(mask: &LayerMask, index: u8) -> Vec<i32> {
    mask_keys::frames(mask, index)
}
fn visible(mask: &LayerMask, filter: PropertyFilter) -> Vec<u8> {
    mask_path::CHANNELS
        .into_iter()
        .filter(|index| match filter {
            PropertyFilter::All => true,
            PropertyFilter::Animated => mask_keys::animated(mask, *index),
            PropertyFilter::Properties(_) => false,
        })
        .collect()
}
pub(in crate::panels) fn row_count(row: &Row, timeline: &TimelineState) -> usize {
    let filter = timeline.filters.get(&row.id).copied().unwrap_or_default();
    let masks: Vec<_> = row
        .masks
        .iter()
        .filter(|mask| !visible(mask, filter).is_empty())
        .collect();
    if masks.is_empty() {
        return 0;
    }
    1 + if timeline.collapsed_masks.contains(&row.id) {
        0
    } else {
        masks
            .iter()
            .map(|mask| {
                1 + if timeline.collapsed_mask_items.contains(&(row.id, mask.id)) {
                    0
                } else {
                    visible(mask, filter).len()
                }
            })
            .sum::<usize>()
    }
}
pub(in crate::panels) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    body: Rect,
    track: Rect,
    mut y: f32,
    height: f32,
    ppf: f32,
    offset: f32,
    row: &Row,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if row_count(row, &panels.timeline) == 0 {
        return;
    }
    let colors = list.styles().colors();
    let locale = panels.locale;
    let owner = mask_keys::Owner {
        object: row.id,
        locked: row.locked || state["observing"] == true,
        offset: row.mask_offset,
    };
    let open = !panels.timeline.collapsed_masks.contains(&row.id);
    if y >= body.min[1] && y + height <= body.max[1] {
        let heading = property_group(
            list,
            atlas,
            area,
            y,
            height,
            if locale == Locale::Zh {
                "蒙版"
            } else {
                "Masks"
            },
            open,
        );
        if click(input, heading) {
            if panels.path_controls.cancel() {
                out.push(Action::History(4));
            }
            if !panels.timeline.collapsed_masks.remove(&row.id) {
                panels.timeline.collapsed_masks.insert(row.id);
            }
        }
    }
    y += height;
    if !open {
        return;
    }
    let filter = panels
        .timeline
        .filters
        .get(&row.id)
        .copied()
        .unwrap_or_default();
    for mask in &row.masks {
        let channels = visible(mask, filter);
        if channels.is_empty() {
            continue;
        }
        let open = !panels
            .timeline
            .collapsed_mask_items
            .contains(&(row.id, mask.id));
        if y >= body.min[1] && y + height <= body.max[1] {
            let heading = property_group(
                list,
                atlas,
                Rect::new(
                    area.min[0] + 20.,
                    area.min[1],
                    area.width() - 20.,
                    area.height(),
                ),
                y,
                height,
                &mask.name,
                open,
            );
            if click(input, heading) {
                if panels.path_controls.cancel() {
                    out.push(Action::History(4));
                }
                if !panels
                    .timeline
                    .collapsed_mask_items
                    .remove(&(row.id, mask.id))
                {
                    panels
                        .timeline
                        .collapsed_mask_items
                        .insert((row.id, mask.id));
                }
            }
        }
        y += height;
        if !open {
            continue;
        }
        for index in channels {
            let py = y;
            y += height;
            if py < body.min[1] || py + height > body.max[1] {
                continue;
            }
            let (property, title, range) = super::mask_inspector::spec(index, locale);
            let values = if index == mask_path::CHANNEL {
                vec![]
            } else {
                mask_keys::values(
                    mask,
                    index,
                    state["frame"].as_f64().unwrap_or(0.) - f64::from(owner.offset),
                )
            };
            let fields = super::mask_inspector::property_row(
                list.styles().as_ref(),
                area,
                py,
                0,
                values.len(),
                Panel::Timeline,
            );
            paint::rect(
                list,
                Rect::new(area.min[0], py, area.width(), height),
                colors.PANEL,
            );
            let number_hit = (0..values.len()).any(|axis| {
                click(
                    input,
                    super::mask_inspector::property_row(
                        list.styles().as_ref(),
                        area,
                        py,
                        axis,
                        values.len(),
                        Panel::Timeline,
                    )
                    .number,
                )
            });
            let lane = Rect::new(track.min[0], py, track.width(), height);
            let key_hit = input.pressed.is_some_and(|(_, p)| lane.contains(p));
            if click(input, fields.caption) || click(input, fields.watch) || number_hit || key_hit {
                panels.select_only(Some(row.id));
                panels
                    .path_controls
                    .keys
                    .choose(state, owner, mask.id, index, out);
                if click(input, fields.caption) || number_hit {
                    panels.timeline.set_graph_mode(false);
                }
            }
            panels.path_controls.keys.watch(
                list,
                fields.watch,
                state,
                owner,
                mask,
                index,
                input,
                out,
            );
            label(list, atlas, fields.caption, title, colors.TEXT_MUTED);
            if index == mask_path::CHANNEL {
                let slot = list.styles().metric("icon.slot", 20.);
                panels.path_controls.path_value(
                    list,
                    Rect::new(fields.number.min[0], py, slot, height),
                    owner,
                    mask.id,
                    input,
                    out,
                );
                if !owner.locked && click(input, fields.caption) {
                    panels.path_controls.edit_mask_path(mask.id, out);
                }
            }
            let host = if panels.focused == Some(Panel::Curves) {
                Panel::Curves
            } else {
                Panel::Timeline
            };
            for axis in 0..values.len() {
                let rect = super::mask_inspector::property_row(
                    list.styles().as_ref(),
                    area,
                    py,
                    axis,
                    values.len(),
                    Panel::Timeline,
                )
                .number;
                panels.path_controls.mask_number(
                    list,
                    atlas,
                    rect,
                    state,
                    owner,
                    mask.id,
                    property.clone(),
                    axis,
                    &values,
                    range,
                    host,
                    input,
                    out,
                );
                if !owner.locked && rect.contains(input.mouse) {
                    panels.cursor =
                        Some(motion_ui::widgets::numeric_behavior::cursor(input, false));
                }
            }
            if !panels.timeline.graph_mode {
                panels.path_controls.keys.lane(
                    list, lane, ppf, offset, state, owner, mask, index, input, out,
                );
            }
        }
    }
}
pub(in crate::panels) fn process(
    list: &PaintList,
    rows: &[Row],
    body: Rect,
    ppf: f32,
    offset: f32,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let controls = &mut panels.path_controls;
    if let Some(object) = controls.keys.owner() {
        if panels.selected != Some(object) {
            if controls.keys.cancel() {
                out.push(Action::History(4));
            }
        } else if let Some(row) = rows.iter().find(|row| row.id == object) {
            controls.keys.process_drag(
                state,
                mask_keys::Owner {
                    object,
                    locked: row.locked || state["observing"] == true,
                    offset: row.mask_offset,
                },
                &row.masks,
                body,
                ppf,
                offset,
                input,
                out,
                list.styles().metric("motion.drag_threshold", 3.),
            );
        } else if controls.keys.cancel() {
            out.push(Action::History(4));
        }
    }
    if let Some((rect, dragging)) = controls.numeric_cursor().filter(|_| {
        matches!(
            controls.numeric_owner(),
            Some(Panel::Timeline | Panel::Curves)
        )
    }) {
        if dragging || rect.contains(input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                input, dragging,
            ));
        }
    }
}
pub(in crate::panels) fn reveal(panels: &mut Panels, state: &Value, object: u64, mask: u64) {
    panels.select_only(Some(object));
    panels.timeline.expanded.insert(object);
    panels.timeline.collapsed_masks.remove(&object);
    panels.timeline.collapsed_mask_items.remove(&(object, mask));
    panels.timeline.filters.remove(&object);
    panels.timeline.set_graph_mode(false);
    panels.focused = Some(Panel::Timeline);
    let rows = crate::panels::timeline::rows(state);
    let height = panels
        .styles
        .as_ref()
        .map_or(22., |s| s.metric("row.height", 22.));
    panels.timeline.scroll = rows
        .iter()
        .take_while(|row| row.id != object)
        .map(|row| crate::panels::timeline::row_count(row, &panels.timeline) as f32 * height)
        .sum();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mask_path_only_animation_is_visible_in_animated_filter_and_collapsed_layer_keys() {
        let mut layer = Layer::solid(7, "Mask", [100.; 2], [0.; 3], [1.; 4]);
        layer.timeline = Some(motion_core::LayerTimeline {
            in_frame: 12,
            out_frame: 120,
            offset_frame: 12,
        });
        let mut node = PathNode {
            id: 1,
            geometry: Track::constant([0.; 6]),
        };
        node.geometry.set_animated(-5, true).unwrap();
        node.geometry.set_at(18, [10.; 6]).unwrap();
        let mask = LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: false,
                nodes: vec![node],
            },
        );
        layer.masks.push(mask.clone());
        assert_eq!(
            visible(&mask, PropertyFilter::Animated),
            [mask_path::CHANNEL]
        );
        assert_eq!(visible(&mask, PropertyFilter::All), mask_path::CHANNELS);
        let state = serde_json::json!({"project":{"layers":[layer],"frames":120}});
        let rows = crate::panels::timeline::rows(&state);
        let row = &rows[0];
        assert!(row.keys.contains(&7.) && row.keys.contains(&30.));
        let mut timeline = TimelineState::default();
        timeline.filters.insert(7, PropertyFilter::Animated);
        assert_eq!(row_count(row, &timeline), 3);
        timeline.collapsed_mask_items.insert((7, 1));
        assert_eq!(row_count(row, &timeline), 2);
        timeline.collapsed_masks.insert(7);
        assert_eq!(row_count(row, &timeline), 1);
    }
}
