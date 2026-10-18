//! Mask properties share layer rows, columns and time mapping with transforms.
use super::*;
use crate::panels::timeline::{property_group_reserved, Row};
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
#[derive(Default)]
pub(in crate::panels) struct Rows {
    present: bool,
    open: bool,
    items: Vec<(usize, bool, Vec<u8>)>,
}
impl Rows {
    pub fn new(row: &Row, timeline: &TimelineState) -> Self {
        let filter = timeline.filters.get(&row.id).copied().unwrap_or_default();
        let mut rows = Self::default();
        rows.open = !timeline.collapsed_masks.contains(&row.id);
        for (index, mask) in row.masks.iter().enumerate() {
            let channels = visible(mask, filter);
            if channels.is_empty() {
                continue;
            }
            rows.present = true;
            if rows.open {
                let open = !timeline.collapsed_mask_items.contains(&(row.id, mask.id));
                rows.items
                    .push((index, open, if open { channels } else { vec![] }));
            }
        }
        rows
    }
    pub fn len(&self) -> usize {
        usize::from(self.present)
            + self
                .items
                .iter()
                .map(|(_, _, channels)| 1 + channels.len())
                .sum::<usize>()
    }
}
#[cfg(test)]
pub(in crate::panels) fn row_count(row: &Row, timeline: &TimelineState) -> usize {
    Rows::new(row, timeline).len()
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
    rows: &Rows,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    if rows.len() == 0 {
        return;
    }
    let colors = list.styles().colors();
    let locale = panels.locale;
    let owner = mask_keys::Owner {
        object: row.id,
        locked: row.locked || state["observing"] == true,
        offset: row.mask_offset,
    };
    let open = rows.open;
    let styles = list.styles();
    let slot = styles.metric("icon.slot", 20.);
    let padding = styles.metric("padding.panel", 8.);
    if y >= body.min[1] && y + height <= body.max[1] {
        let heading = property_group_reserved(
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
            slot + padding,
        );
        if click(input, heading) {
            if panels.path_controls.cancel() {
                out.push(Action::History(4));
            }
            if !panels.timeline.collapsed_masks.remove(&row.id) {
                panels.timeline.collapsed_masks.insert(row.id);
            }
        }
        let add = Rect::new(area.max[0] - padding - slot, y, slot, height);
        let allowed = !owner.locked
            && state["project"]["layers"]
                .as_array()
                .and_then(|layers| layers.iter().find(|l| l["id"].as_u64() == Some(row.id)))
                .is_some_and(|l| {
                    !matches!(
                        l["content"]["kind"].as_str(),
                        Some("null" | "adjustment" | "audio")
                    )
                });
        if allowed && click(input, add) {
            out.extend(crate::panels::cancel_interaction(panels));
            panels.select_only(Some(row.id));
        }
        if panels
            .path_controls
            .timeline_mask_add_button(list, add, row.id, state, input, allowed, out)
        {
            panels.select_only(Some(row.id));
            panels.timeline.collapsed_masks.remove(&row.id);
        }
    }
    y += height;
    if !open {
        return;
    }
    let indent = list.styles().metric("tree.indent", 14.);
    for (mask_index, open, channels) in &rows.items {
        let mask = &row.masks[*mask_index];
        let open = *open;
        if y >= body.min[1] && y + height <= body.max[1] {
            let heading = property_group_reserved(
                list,
                atlas,
                Rect::new(
                    area.min[0] + indent,
                    area.min[1],
                    (area.width() - indent).max(0.),
                    area.height(),
                ),
                y,
                height,
                &mask.name,
                open,
                slot * 4. + padding,
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
            if !owner.locked
                && super::mask_header::action_rects(&styles, area, y, height)
                    .iter()
                    .any(|rect| click(input, *rect))
            {
                out.extend(crate::panels::cancel_interaction(panels));
                panels.select_only(Some(row.id));
            }
            panels
                .path_controls
                .mask_header_actions(list, atlas, area, y, height, owner, mask, input, out);
        }
        y += height;
        if !open {
            continue;
        }
        for index in channels.iter().copied() {
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
            crate::panels::timeline::property_branch(list, fields.watch);
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
