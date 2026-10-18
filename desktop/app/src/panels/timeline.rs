use super::*;
use super::{
    toolbar::icon_button,
    types::{ScrubRuler, Tool},
};
use std::hash::{Hash, Hasher};

pub(super) struct Row {
    pub(super) spatial: bool,
    pub(super) camera: bool,
    pub(super) effects: usize,
    pub(super) audio: usize,
    pub(super) masks: Vec<motion_core::masks::LayerMask>,
    pub(super) mask_offset: i32,
    pub(super) vector: Option<motion_core::vector::VectorContent>,
    pub(super) vector_size: [f32; 2],
    pub(super) id: u64,
    pub(super) name: String,
    name_lower: String,
    pub(super) visible: bool,
    pub(super) locked: bool,
    pub(super) clip: [i32; 2],
    pub(super) range: [f64; 2],
    pub(super) keys: Vec<f64>,
    references: Vec<f64>,
    pub(super) properties: Vec<PropertyRow>,
}

pub(super) struct PropertyRow {
    pub property: Property,
    pub key: &'static str,
    pub title: T,
    pub axis: Option<usize>,
    pub axes: Vec<usize>,
    pub keys: Vec<f64>,
}

pub(super) struct PropertyLayout {
    pub watch: Rect,
    pub caption: Rect,
    pub number: Rect,
}
pub(super) fn property_layout(
    styles: &motion_ui::style::StyleSheet,
    area: Rect,
    y: f32,
    height: f32,
    component: usize,
    components: usize,
) -> PropertyLayout {
    // Reserve a readable caption for two nested operator levels before sharing
    // the remaining width between components. Every property family uses this
    // value edge, so expanding a shape never shifts its values independently.
    let pad = styles.metric("padding.panel", 8.);
    let caption = styles.metric("timeline.property.caption_offset", 118.);
    let preferred =
        caption + styles.metric("tree.indent", 14.) * 2. + styles.metric("label.column", 72.) + pad;
    let value = (area.width() * 0.52).max(preferred).min(area.width() * 0.7);
    let cell_width = ((area.width() - value - pad).max(0.) / components.max(1) as f32)
        .min(styles.metric("value.min_width", 64.));
    PropertyLayout {
        watch: Rect::new(
            area.min[0] + styles.metric("timeline.property.watch_offset", 98.),
            y,
            styles.metric("timeline.property.watch_width", 14.),
            height,
        ),
        caption: Rect::new(
            area.min[0] + caption,
            y,
            (value - caption - pad).max(0.),
            height,
        ),
        number: Rect::new(
            area.min[0] + value + component as f32 * cell_width,
            y,
            (cell_width - styles.metric("border", 1.) * 2.).max(0.),
            height,
        ),
    }
}
pub(super) fn property_icon(
    list: &mut PaintList,
    area: Rect,
    icon: motion_ui::icons::Icon,
    active: bool,
    enabled: bool,
) {
    let colors = list.styles().colors();
    motion_ui::icons::draw(
        list,
        area,
        icon,
        list.styles().metric("timeline.property.icon_size", 16.),
        if !enabled {
            list.styles().color("disabled")
        } else if active {
            colors.ACCENT
        } else {
            colors.TEXT_MUTED
        },
    );
}
pub(super) fn property_group(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    y: f32,
    height: f32,
    title: &str,
    open: bool,
) -> Rect {
    let styles = list.styles();
    let pad = styles.metric("padding", 5.);
    let slot = styles.metric("icon.slot", 20.);
    let start = styles.metric("timeline.property.group_offset", 78.);
    let group = Rect::new(
        area.min[0] + start,
        y,
        (area.width() - start - pad).max(0.),
        height,
    );
    motion_ui::style::box_style(
        list,
        group,
        styles
            .resolve(
                "timeline.property.group",
                motion_ui::style::States::default(),
            )
            .unwrap(),
    );
    motion_ui::icons::draw(
        list,
        Rect::new(group.min[0], y, slot, height),
        if open {
            motion_ui::icons::Icon::ChevronDown
        } else {
            motion_ui::icons::Icon::ChevronRight
        },
        styles.metric("icon.small.size", 14.),
        styles.color("muted"),
    );
    label(
        list,
        atlas,
        Rect::new(
            group.min[0] + slot,
            y,
            (group.width() - slot - pad).max(0.),
            height,
        ),
        title,
        false,
    );
    group
}
pub(super) fn property_branch(list: &mut PaintList, watch: Rect) {
    let styles = list.styles();
    let x = watch.min[0] - styles.metric("icon.slot", 20.) * 0.5;
    let middle = (watch.min[1] + watch.max[1]) * 0.5;
    let end = watch.min[0] - styles.metric("padding", 5.);
    paint::line(
        list,
        vec![[x, watch.min[1]], [x, watch.max[1]]],
        styles.color("border"),
    );
    paint::line(
        list,
        vec![[x, middle], [end, middle]],
        styles.color("border"),
    );
}
pub(super) fn property_rows(state: &Value, object: u64) -> Vec<PropertyRow> {
    let layer = state["project"]["layers"]
        .as_array()
        .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)));
    let timeline = state["timeline_layers"]
        .as_array()
        .and_then(|ts| ts.iter().find(|t| t["object"].as_u64() == Some(object)));
    property_rows_for(layer, timeline)
}
fn property_rows_for(layer: Option<&Value>, timeline: Option<&Value>) -> Vec<PropertyRow> {
    let three_d = layer.is_some_and(|l| l["three_d"].as_bool() == Some(true));
    if layer.is_some_and(|l| l["content"]["kind"].as_str() == Some("audio")) {
        return vec![];
    }
    let mut rows = vec![];
    let camera = layer.filter(|l| l["content"]["kind"] == "camera").map(|l| &l["content"]["camera"]);
    let properties = if let Some(camera) = camera {
        let mut rows = vec![
            (Property::Target, "target", T::CameraTarget, vec![0, 1, 2]),
            (Property::Roll, "roll", T::CameraRoll, vec![0]),
            (Property::Fov, "fov", T::CameraFov, vec![0]),
        ];
        if camera["mode"] == "orbit" {
            rows.extend([
                (Property::Radius, "radius", T::CameraRadius, vec![0]),
                (Property::Azimuth, "azimuth", T::CameraAzimuth, vec![0]),
                (Property::Elevation, "elevation", T::CameraElevation, vec![0]),
            ]);
        } else {
            rows.insert(0, (Property::Position, "position", T::Position, vec![0, 1, 2]));
        }
        rows
    } else { vec![
        (
            Property::Position,
            "position",
            T::Position,
            if three_d { vec![0, 1, 2] } else { vec![0, 1] },
        ),
        (
            Property::Scale,
            "scale",
            T::Scale,
            if three_d { vec![0, 1, 2] } else { vec![0, 1] },
        ),
        (
            Property::Rotation,
            "rotation",
            T::Rotation,
            if three_d { vec![0, 1, 2] } else { vec![2] },
        ),
        (Property::Opacity, "opacity", T::Opacity, vec![0]),
    ] };
    for (property, key, title, axes) in properties {
        let track = timeline
            .map(|t| &t["properties"][key])
            .unwrap_or(&Value::Null);
        let separated = track["separated"].as_bool().unwrap_or(false);
        let selections = if separated {
            axes.iter().map(|a| Some(*a)).collect()
        } else {
            vec![None]
        };
        for axis in selections {
            let source = axis
                .map(|a| &track["axes"][["x", "y", "z"][a]])
                .unwrap_or(track);
            let keys = source["keys"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|k| k["frame"].as_f64())
                .collect();
            rows.push(PropertyRow {
                property,
                key,
                title,
                axis,
                axes: axis.map(|a| vec![a]).unwrap_or_else(|| axes.clone()),
                keys,
            });
        }
    }
    rows
}
pub(super) fn rows(state: &Value) -> Vec<Row> {
    let total = state["project"]["frames"].as_u64().unwrap_or(1) as u32;
    let timeline_by_object: std::collections::HashMap<_, _> = state["timeline_layers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|timeline| Some((timeline["object"].as_u64()?, timeline)))
        .collect();
    let mut rows: Vec<_> = state["project"]["layers"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .map(|l| {
            let id = l["id"].as_u64().unwrap_or(0);
            let t = timeline_by_object.get(&id).copied();
            let properties = property_rows_for(Some(l), t);
            let mut keys = Vec::new();
            if let Some(t) = t {
                for property in ["position", "rotation", "scale", "opacity", "target", "roll", "fov", "radius", "azimuth", "elevation"] {
                    let track = &t["properties"][property];
                    for k in track["keys"].as_array().into_iter().flatten() {
                        if let Some(f) = k["frame"].as_f64() {
                            keys.push(f);
                        }
                    }
                    for axis in ["x", "y", "z"] {
                        for k in track["axes"][axis]["keys"].as_array().into_iter().flatten() {
                            if let Some(f) = k["frame"].as_f64() {
                                keys.push(f);
                            }
                        }
                    }
                }
            }
            let masks: Vec<motion_core::masks::LayerMask> = if l["masks"].is_null() {
                Vec::new()
            } else {
                observe!(
                    "timeline.masks",
                    serde_json::from_value(l["masks"].clone()),
                    "object={}",
                    l["id"]
                )
                .unwrap_or_default()
            };
            let mask_offset = l["timeline"]["offset_frame"].as_i64().unwrap_or(0) as i32;
            for mask in &masks {
                for index in 0..4 {
                    for frame in super::path_controls::timeline::key_frames(mask, index) {
                        keys.push(f64::from(frame) + f64::from(mask_offset));
                    }
                }
            }
            let vector: Option<motion_core::vector::VectorContent> =
                if l["content"]["vector"].is_null() && l["content"]["kind"] != "vector" {
                    None
                } else {
                    observe!(
                        "timeline.vector",
                        serde_json::from_value(l["content"]["vector"].clone()),
                        "object={}",
                        l["id"]
                    )
                    .ok()
                };
            let vector_size = observe!(
                "timeline.vector_size",
                serde_json::from_value(l["size"].clone()),
                "object={}",
                l["id"]
            )
            .unwrap_or([100.; 2]);
            if let Some(v) = &vector {
                for frame in super::path_controls::groups::timeline::key_frames(v, vector_size) {
                    keys.push(f64::from(frame) + f64::from(mask_offset));
                }
            }
            keys.sort_by(|a, b| a.total_cmp(b));
            keys.dedup();
            let offset = l["timeline"]["offset_frame"].as_i64().unwrap_or(0);
            let visible_begin = t
                .and_then(|t| {
                    t["subframe_range"]["in_frame"]
                        .as_f64()
                        .or_else(|| t["in_frame"].as_f64())
                })
                .unwrap_or(0.);
            let visible_end = t
                .and_then(|t| {
                    t["subframe_range"]["out_frame"]
                        .as_f64()
                        .or_else(|| t["out_frame"].as_f64())
                })
                .unwrap_or(f64::from(total));
            let references = l["audio_markers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|m| m["frame"].as_i64().map(|f| (f + offset) as f64))
                .filter(|f| *f >= visible_begin.max(0.) && *f < visible_end.min(f64::from(total)))
                .collect();
            Row {
                spatial: l["content"]["kind"] != "audio",
                camera: l["content"]["kind"] == "camera",
                effects: l["effects"].as_array().map_or(0, Vec::len),
                audio: if super::audio::is_audio(l) {
                    super::audio::rows(l)
                } else {
                    0
                },
                masks,
                mask_offset,
                vector,
                vector_size,
                id,
                name: l["name"].as_str().unwrap_or("Layer").into(),
                name_lower: l["name"].as_str().unwrap_or("Layer").to_lowercase(),
                visible: l["visible"].as_bool().unwrap_or(true),
                locked: l["locked"].as_bool().unwrap_or(false),
                clip: [
                    t.and_then(|t| t["in_frame"].as_i64()).unwrap_or(0) as i32,
                    t.and_then(|t| t["out_frame"].as_i64())
                        .unwrap_or(i64::from(total)) as i32,
                ],
                range: [
                    t.and_then(|t| {
                        t["subframe_range"]["in_frame"]
                            .as_f64()
                            .or_else(|| t["in_frame"].as_f64())
                    })
                    .unwrap_or(0.),
                    t.and_then(|t| {
                        t["subframe_range"]["out_frame"]
                            .as_f64()
                            .or_else(|| t["out_frame"].as_f64())
                    })
                    .unwrap_or(total as f64),
                ],
                keys,
                references,
                properties,
            }
        })
        .collect();
    if state["project"]["camera"]["created"]
        .as_bool()
        .unwrap_or(false)
    {
        rows.insert(
            0,
            Row {
                spatial: false,
                camera: true,
                effects: 0,
                audio: 0,
                vector: None,
                vector_size: [0.; 2],
                masks: vec![],
                mask_offset: 0,
                id: 0,
                name: "Camera".into(),
                name_lower: "camera".into(),
                visible: true,
                locked: true,
                clip: [0, total as i32],
                range: [0., total as f64],
                keys: vec![],
                references: vec![],
                properties: vec![],
            },
        );
    }
    rows
}
/// Convert a displayed insertion slot into a storage index for
/// `Command::Reorder`. Displayed layer rows are the storage stack reversed;
/// `None` marks no-op drops (the layer's own edge slots).
pub(super) fn reorder_storage_index(len: usize, from_row: usize, slot: usize) -> Option<usize> {
    if len == 0 || from_row >= len {
        return None;
    }
    let slot = slot.min(len);
    if slot == from_row || slot == from_row + 1 {
        return None;
    }
    let storage_from = len - 1 - from_row;
    let mut index = len - slot;
    if storage_from < index {
        index -= 1;
    }
    Some(index.min(len - 1))
}

fn scroll_extent_signature(
    rows_key: &(String, String, u64),
    timeline: &types::TimelineState,
    query: &str,
    area_height: f32,
    row_height: f32,
) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    rows_key.hash(&mut hash);
    query.hash(&mut hash);
    area_height.to_bits().hash(&mut hash);
    row_height.to_bits().hash(&mut hash);
    timeline.expanded.len().hash(&mut hash);
    for id in &timeline.expanded {
        id.hash(&mut hash);
    }
    timeline.collapsed_transforms.len().hash(&mut hash);
    for id in &timeline.collapsed_audio {
        id.hash(&mut hash);
    }
    for id in &timeline.collapsed_transforms {
        id.hash(&mut hash);
    }
    timeline.collapsed_masks.len().hash(&mut hash);
    timeline.collapsed_mask_items.len().hash(&mut hash);
    for object in &timeline.collapsed_masks {
        object.hash(&mut hash);
    }
    for item in &timeline.collapsed_mask_items {
        item.hash(&mut hash);
    }
    for item in &timeline.vector_open {
        item.hash(&mut hash);
    }
    format!("{:?}", timeline.expression_rows).hash(&mut hash);
    timeline.filters.len().hash(&mut hash);
    for entry in &timeline.filters {
        entry.hash(&mut hash);
    }
    hash.finish()
}
fn calculate_scroll_max(
    layouts: &[super::timeline_layout::LayerRows],
    area_height: f32,
    row_height: f32,
) -> f32 {
    (layouts.iter().map(|row| row.len()).sum::<usize>() as f32 * row_height - (area_height - 64.0))
        .max(0.0)
}
pub(super) fn row_count(row: &Row, timeline: &types::TimelineState) -> usize {
    super::timeline_layout::LayerRows::new(row, timeline, 0., Locale::En).len()
}
pub(super) fn filtered_properties(
    state: &Value,
    object: u64,
    filter: types::PropertyFilter,
) -> Vec<PropertyRow> {
    property_rows(state, object)
        .into_iter()
        .filter(|row| filter.admits(row.property, !row.keys.is_empty()))
        .collect()
}
pub(super) fn property_visible(
    panels: &Panels,
    state: &Value,
    object: u64,
    property: Property,
) -> bool {
    let animated = property_rows(state, object)
        .iter()
        .any(|r| r.property == property && !r.keys.is_empty());
    panels
        .timeline
        .filters
        .get(&object)
        .copied()
        .unwrap_or_default()
        .admits(property, animated)
}

fn select_row(
    panels: &mut Panels,
    rows: &[&Row],
    index: usize,
    object: u64,
    toggle: bool,
    range: bool,
) {
    panels.audio_controls.clear_selection();
    if object == 0 {
        panels.select_only(Some(object));
    } else if range {
        if let Some(anchor) = panels
            .selection_anchor
            .or(panels.selected)
            .and_then(|id| rows.iter().position(|row| row.id == id))
        {
            let (start, end) = if anchor <= index {
                (anchor, index)
            } else {
                (index, anchor)
            };
            if !toggle {
                panels.selected_layers.clear();
            }
            panels.animation_controls.clear_key();
            for row in rows[start..=end].iter() {
                panels.add_selection(row.id);
            }
            panels.selected = Some(object);
        } else {
            panels.select_only(Some(object));
        }
    } else if toggle {
        panels.toggle_selection(object);
    } else {
        panels.select_only(Some(object));
    }
}

pub(super) fn render(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    super::audio::prepare(panels, state, input, out);
    if panels.focused == Some(Panel::Timeline) {
        super::properties::process_field(panels, state, input, out);
    }
    let styles = list.styles();
    panels.timeline_anchor.process(&styles, state, input, out);
    let row_height = styles.metric("row.height", 22.);
    let toolbar_height = styles.metric("field.height", 18.) + styles.metric("padding", 5.) * 2.;
    let toolbar_top = area.min[1] + styles.metric("border", 1.) * 2.;
    if panels.timeline.moving.is_some()
        && !input.composing
        && !input.ime_consumed
        && motion_ui::input::KeyChord::new(motion_ui::input::Key::Escape, false, false, false)
            .pressed(input)
    {
        out.extend(super::cancel_interaction(panels));
    }
    let colors = styles.colors();
    let footer_height = styles.metric("field.height", 18.).min(area.height());

    panels.timeline_columns.begin(state);
    let context_input = input;
    panels.path_controls.groups.begin_frame(list, context_input);
    panels
        .path_controls
        .groups
        .prepare(state, panels.selected, context_input, out);
    let neutral = Input::default();
    let input = if panels.timeline.timeline_context_menu.is_some()
        || panels.timeline_columns.editing()
        || panels.layer_actions.editing()
        || panels.path_controls.mask_menu()
    {
        &neutral
    } else {
        input
    };
    if let Some(pos) = panels.timeline.timeline_context_menu.take() {
        panels.layer_actions.open(None, pos, state);
    }
    paint::rect(list, area, colors.PANEL);
    let total = last_frame(state) + 1.0;
    let fps = state["project"]["fps"].as_u64().unwrap_or(30).max(1) as f32;
    let graph_mode = panels.timeline.graph_mode;
    let cache_key = (
        state["root"].as_str().unwrap_or("").to_string(),
        state["composition"].as_str().unwrap_or("").to_string(),
        state["revision"].as_u64().unwrap_or(0),
    );
    if panels
        .timeline
        .rows_cache
        .as_ref()
        .is_none_or(|(key, _)| key != &cache_key)
    {
        panels.timeline.rows_cache = Some((cache_key, std::sync::Arc::new(rows(state))));
    }
    let all_rows = panels.timeline.rows_cache.as_ref().unwrap().1.clone();
    let query = panels.timeline.timeline_query.to_lowercase();
    let rows: Vec<_> = all_rows
        .iter()
        .filter(|row| query.is_empty() || row.name_lower.contains(&query))
        .collect();
    panels.timeline.expression_rows.clear();
    if panels.expressions.owner == Some(Panel::Timeline) {
        if let Some(target) = panels.expressions.draft.clone() {
            panels.timeline.expression_rows.push(target);
        }
    }
    let layouts: Vec<_> = rows
        .iter()
        .map(|row| {
            super::timeline_layout::LayerRows::new(
                row,
                &panels.timeline,
                state["frame"].as_f64().unwrap_or(0.),
                panels.locale,
            )
        })
        .collect();
    let extent_signature = {
        let rows_key = &panels.timeline.rows_cache.as_ref().unwrap().0;
        scroll_extent_signature(
            rows_key,
            &panels.timeline,
            &query,
            area.height() - footer_height,
            row_height,
        )
    };
    let scroll_max = if panels
        .timeline
        .scroll_extent_cache
        .is_some_and(|(signature, _)| signature == extent_signature)
    {
        panels.timeline.scroll_extent_cache.unwrap().1
    } else {
        let max = calculate_scroll_max(&layouts, area.height() - footer_height, row_height);
        panels.timeline.scroll_extent_cache = Some((extent_signature, max));
        max
    };
    let gutter = GUTTER.min(area.width() * 0.52);
    let columns = super::timeline_columns::ColumnRects::new(
        area.min[0],
        gutter,
        panels.timeline.column_visibility,
    );
    let track = Rect::new(
        area.min[0] + gutter,
        area.min[1],
        (area.width() - gutter - 8.0).max(1.0),
        area.height(),
    );
    let ruler = Rect::new(track.min[0], area.min[1] + 36.0, track.width(), 28.0);
    panels.timeline.zoom_track = Some(track);
    let wheel = input.wheel[1];
    let gesture = panels.timeline.graph_box.editing()
        || panels.timeline.key_drag.is_some()
        || panels.timeline.graph_drag.is_some()
        || panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
        || panels.timeline.scrubbing
        || panels.timeline.horizontal_drag.is_some()
        || panels.properties.field.is_some()
        || panels.audio_controls.editing()
        || panels.path_controls.editing();
    if area.contains(input.mouse) && wheel.is_finite() && wheel != 0. && !gesture {
        let graph_wheel = graph_mode
            && input.mouse[0] >= track.min[0]
            && input.mouse[1] >= ruler.max[1]
            && !input.modifiers.shift
            && !input.modifiers.alt;
        if input.modifiers.control && !input.modifiers.alt || graph_wheel {
            let old_zoom = panels.timeline.zoom.max(1.);
            let fraction = ((input.mouse[0] - track.min[0]) / track.width()).clamp(0., 1.);
            let next = old_zoom * (-wheel * 0.005).clamp(-8., 8.).exp();
            super::timeline_zoom::zoom_at(&mut panels.timeline, total as f32, next, fraction);
        } else if input.modifiers.control && input.modifiers.alt && graph_mode {
            // The graph owns vertical scaling; one wheel event has one owner.
        } else if input.modifiers.shift {
            panels.timeline.offset = (panels.timeline.offset + wheel).max(0.);
        } else if !(graph_mode && input.mouse[0] >= track.min[0]) {
            panels.timeline.scroll = (panels.timeline.scroll + wheel).clamp(0., scroll_max);
        }
    }
    panels.timeline.scroll = panels.timeline.scroll.clamp(0.0, scroll_max);
    panels.timeline.offset = panels
        .timeline
        .offset
        .min((total as f32 - total as f32 / panels.timeline.zoom.max(1.0)).max(0.0));
    let candidates_for = |exclude: Option<u64>, include_playhead: bool| {
        let mut candidates = vec![0., total];
        if include_playhead {
            candidates.push(state["frame"].as_f64().unwrap_or(0.));
        }
        for row in all_rows.iter().filter(|r| Some(r.id) != exclude) {
            candidates.extend(row.range);
            candidates.extend(row.keys.iter().copied());
            candidates.extend(row.references.iter().copied());
        }
        for key in ["in_frame", "out_frame"] {
            if let Some(v) = state["project"]["work_area"][key].as_f64() {
                candidates.push(v);
            }
        }
        candidates
    };
    let seek_candidates = candidates_for(None, false);
    paint::rect(
        list,
        Rect::new(area.min[0], area.min[1], area.width(), 64.0),
        colors.CHROME,
    );
    motion_ui::style::text_style(
        list,
        atlas,
        Rect::new(area.min[0] + 10.0, area.min[1], gutter - 20.0, 30.0),
        &timecode(
            state["frame"].as_f64().unwrap_or(0.0),
            state["project"]["fps"].as_u64().unwrap_or(30) as u32,
        ),
        motion_ui::style::ResolvedStyle {
            foreground: colors.ACCENT,
            ..styles
                .as_ref()
                .resolve("label", Default::default())
                .unwrap()
        },
        Align::Left,
    );
    let locale = panels.locale;
    if gutter >= 220. {
        if ui::button(
            list,
            atlas,
            Rect::new(area.min[0] + gutter - 98., toolbar_top, 90., toolbar_height),
            if graph_mode {
                if locale == Locale::Zh {
                    "时间轴"
                } else {
                    "Timeline"
                }
            } else if locale == Locale::Zh {
                "曲线编辑"
            } else {
                "Graph editor"
            },
            input,
            true,
        )
        .clicked
        {
            out.extend(super::cancel_interaction(panels));
            panels.timeline.set_graph_mode(!graph_mode);
            let neutral = Input {
                mouse: input.mouse,
                ..Default::default()
            };
            render(list, atlas, area, panels, state, &neutral, out);
            return;
        }
    }
    let search = Rect::new(
        track.min[0] + 12.,
        toolbar_top,
        (track.width() - 176.).min(250.).max(1.),
        toolbar_height,
    );
    paint::rect(list, search, colors.SUNKEN);
    paint::stroke(
        list,
        search,
        if panels.timeline.timeline_search {
            colors.ACCENT
        } else {
            colors.BORDER
        },
        1.,
    );
    if clicked(input, search) {
        panels.timeline.timeline_search = true;
    } else if input.pressed.is_some() {
        panels.timeline.timeline_search = false;
    }
    if panels.timeline.timeline_search {
        panels.timeline.timeline_query.push_str(&input.text);
        if input.keys.iter().any(|(k, _)| *k == Key::Backspace) {
            panels.timeline.timeline_query.pop();
        }
    }
    label(
        list,
        atlas,
        search.shrink(5.),
        if panels.timeline.timeline_query.is_empty() {
            if locale == Locale::Zh {
                "搜索图层…"
            } else {
                "Search layers…"
            }
        } else {
            &panels.timeline.timeline_query
        },
        true,
    );
    let zoom_x = track.max[0] - 146.;
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x, toolbar_top, toolbar_height, toolbar_height),
        "−",
        input,
        true,
    )
    .clicked
    {
        let next = panels.timeline.zoom.max(1.) / 1.25;
        super::timeline_zoom::zoom_at(&mut panels.timeline, total as f32, next, 0.5);
    }
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x + 86., toolbar_top, toolbar_height, toolbar_height),
        "+",
        input,
        true,
    )
    .clicked
    {
        let next = panels.timeline.zoom.max(1.) * 1.25;
        super::timeline_zoom::zoom_at(&mut panels.timeline, total as f32, next, 0.5);
    }
    if ui::button(
        list,
        atlas,
        Rect::new(zoom_x + 117., toolbar_top, toolbar_height, toolbar_height),
        "↔",
        input,
        true,
    )
    .clicked
    {
        panels.timeline.zoom = 1.;
        panels.timeline.offset = 0.;
    }
    label(
        list,
        atlas,
        Rect::new(zoom_x + 29., toolbar_top, 58., toolbar_height),
        &format!("{:.0}%", panels.timeline.zoom.max(1.) * 100.),
        true,
    );
    let navigation = super::timeline_navigation::Geometry::new(track, footer_height, total as f32, &panels.timeline, &styles);
    super::timeline_navigation::update(&navigation, &mut panels.timeline, input, total as f32, gesture);
    let ppf = track.width() / total as f32 * panels.timeline.zoom.max(1.0);
    let offset = panels.timeline.offset;
    let x_of = |f: f64| track.min[0] + (f as f32 - offset) * ppf;
    super::audio::process_drag(panels, state, input, ppf, out);
    for (i, (tool, action)) in [
        (Tool::First, Action::Seek(0.0)),
        (
            if panels.playing {
                Tool::Pause
            } else {
                Tool::Play
            },
            Action::TogglePlay,
        ),
        (Tool::Last, Action::Seek(last_frame(state))),
    ]
    .into_iter()
    .enumerate()
    {
        icon_button(
            list,
            Rect::new(
                area.min[0] + 146.0 + i as f32 * 38.0,
                toolbar_top,
                32.0,
                toolbar_height,
            ),
            tool,
            input,
            true,
            action,
            out,
        );
    }
    label(
        list,
        atlas,
        Rect::new(
            area.min[0] + 90.0,
            ruler.min[1],
            (columns.name.max_x() - (area.min[0] + 90.0)).max(1.0),
            28.0,
        ),
        locale.t(T::SourceName),
        true,
    );
    if let Some(matte) = columns.matte {
        label(
            list,
            atlas,
            matte.rect(ruler.min[1], 28.0),
            if locale == Locale::Zh {
                "遮罩"
            } else {
                "Track matte"
            },
            true,
        );
    }
    if let Some(blend) = columns.blend {
        label(
            list,
            atlas,
            blend.rect(ruler.min[1], 28.0),
            locale.t(T::BlendMode),
            true,
        );
    }
    if let (Some(link), Some(parent)) = (columns.link, columns.parent) {
        label(
            list,
            atlas,
            Rect::new(
                link.x + 2.0,
                ruler.min[1],
                parent.max_x() - link.x - 2.0,
                28.0,
            ),
            locale.t(T::ParentLink),
            true,
        );
    }
    ui::icon(
        list,
        Rect::new(area.min[0] + 10.0, ruler.min[1] + 3.0, 16.0, 24.0),
        Icon::Eye,
    );
    ui::icon(
        list,
        Rect::new(area.min[0] + 30.0, ruler.min[1] + 3.0, 16.0, 24.0),
        Icon::Lock,
    );
    let scrub_frame = render_time_ruler(
        list,
        atlas,
        ruler,
        panels,
        state,
        input,
        ppf,
        offset,
        &seek_candidates,
        out,
    );
    let body = Rect::new(
        area.min[0],
        ruler.max[1],
        area.width(),
        (area.max[1] - footer_height - ruler.max[1]).max(0.0),
    );
    let mut row_offset = 0;
    let mut timeline_hit = false;
    let mut row_bounds: Vec<(u64, f32, f32)> = Vec::new();
    let mut background_row = 0;
    for (row, layout) in rows.iter().zip(&layouts) {
        super::timeline_layout::paint_rows(
            list,
            body,
            body.min[1] + background_row as f32 * row_height - panels.timeline.scroll,
            row_height,
            layout.len(),
            panels.selection_contains(row.id),
            input,
        );
        background_row += layout.len();
    }
    if !graph_mode {
        super::timeline_layout::paint_track_boundary(list, track, body);
    }
    for (index, (row, layout)) in rows.iter().zip(&layouts).enumerate() {
        let expandable = row.id != 0 && (row.spatial || row.audio > 0);
        let expanded = layout.expanded;
        let y = body.min[1] + row_offset as f32 * row_height - panels.timeline.scroll;
        let row_count = layout.len();
        row_offset += row_count;
        if row.id != 0 {
            row_bounds.push((row.id, y, row_count as f32 * row_height));
        }
        if y + row_count as f32 * row_height < body.min[1] - row_height
            || y > body.max[1] + row_height
        {
            continue;
        }
        let row_group = Rect::new(area.min[0], y, area.width(), row_count as f32 * row_height);
        if clicked(input, row_group) {
            timeline_hit = true;
        }
        if row.audio > 0 {
            if let Some((MouseButton::Right, point)) =
                input.pressed.filter(|(_, p)| row_group.contains(*p))
            {
                panels.select_only(Some(row.id));
                let audio_y = y + layout.audio.start as f32 * row_height;
                let audio_rows = layout.audio.len();
                if expanded
                    && point[1] >= audio_y
                    && point[1] < audio_y + audio_rows as f32 * row_height
                {
                    panels
                        .layer_actions
                        .open_audio(row.id, point, state, locale);
                } else if point[1] < audio_y {
                    panels.layer_actions.open(Some(row.id), point, state);
                }
            }
        }
        if expanded {
            render_layer_properties(
                list,
                atlas,
                Rect::new(area.min[0], body.min[1], gutter, body.height()),
                body,
                track,
                y,
                row_height,
                ppf,
                offset,
                row,
                layout,
                panels,
                state,
                input,
                out,
            );
        }
        if y < body.min[1] || y + row_height > body.max[1] {
            continue;
        }
        let name = Rect::new(
            columns.name.x,
            y,
            columns.name.width,
            row_height,
        );
        let selected = panels.selection_contains(row.id);
        let expand = Rect::new(area.min[0] + 75., y, 14., row_height);
        if expandable {
            let x = expand.min[0] + 3.;
            let cy = y + row_height * 0.5;
            paint::line(
                list,
                if expanded {
                    vec![[x, cy - 2.], [x + 3., cy + 2.], [x + 6., cy - 2.]]
                } else {
                    vec![[x, cy - 3.], [x + 4., cy], [x, cy + 3.]]
                },
                colors.TEXT_MUTED,
            );
            if clicked(input, expand) {
                if expanded {
                    panels.timeline.expanded.remove(&row.id);
                } else {
                    panels.timeline.expanded.insert(row.id);
                    panels.timeline.collapsed_geometry.insert(row.id);
                    panels.timeline.reopen_properties(row.id);
                }
            }
        }
        let row_area = Rect::new(body.min[0], y, body.width(), row_height);
        let name_editing = panels
            .layer_actions
            .row(list, atlas, name, row_area, row.id, state, input, locale);
        if right_clicked(input, row_area) {
            if panels.selection_contains(row.id) {
                panels.selected = Some(row.id);
            } else {
                panels.select_only(Some(row.id));
            }
        }
        if !name_editing {
            label(list, atlas, name, &row.name, false);
        }
        paint::rect(
            list,
            Rect::new(area.min[0] + 66.0, y + 7.0, 6.0, 12.0),
            colors.TRACKS[index % colors.TRACKS.len()],
        );
        label(
            list,
            atlas,
            Rect::new(area.min[0] + 50.0, y, 14.0, row_height),
            &(index + 1).to_string(),
            true,
        );
        let layer = state["project"]["layers"]
            .as_array()
            .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)));
        let icon = match layer.map(|l| l["content"]["kind"].as_str().unwrap_or("")) {
            Some("text") => motion_ui::icons::Icon::Text,
            Some("image") => motion_ui::icons::Icon::Image,
            Some("video") => motion_ui::icons::Icon::Video,
            Some("audio") => motion_ui::icons::Icon::Audio,
            Some("camera") => motion_ui::icons::Icon::Camera,
            _ if row.id == 0 => motion_ui::icons::Icon::Camera,
            _ => motion_ui::icons::Icon::Shape,
        };
        motion_ui::icons::draw(
            list,
            Rect::new(area.min[0] + 91., y, 18., row_height),
            icon,
            18.,
            colors.TEXT_MUTED,
        );
        if let Some(owner) = super::timeline_columns::owner(state, row.id) {
            let cell_height = (row_height - 4.).max(1.);
            let matte = columns.matte.map(|slot| slot.rect(y + 2., cell_height));
            let blend = columns.blend.map(|slot| slot.rect(y + 2., cell_height));
            let parent = columns.parent.map(|slot| slot.rect(y + 2., cell_height));
            let link = columns
                .link
                .map(|slot| slot.rect(y + 4., (row_height - 8.).max(1.)));
            let blend_supported = !matches!(
                owner["content"]["kind"].as_str(),
                Some("null" | "camera" | "audio" | "adjustment")
            ) && row.id != 0;
            let blend_enabled = blend_supported && !row.locked;
            let matte_enabled = blend_enabled && matte.is_some();
            let parent_enabled = row.id == 0 || !row.locked && row.spatial;
            if (blend_enabled && blend.is_some_and(|rect| clicked(input, rect)))
                || (matte_enabled && matte.is_some_and(|rect| clicked(input, rect)))
                || (parent_enabled
                    && (parent.is_some_and(|rect| clicked(input, rect))
                        || link.is_some_and(|rect| clicked(input, rect))))
            {
                out.extend(super::cancel_interaction(panels));
            }
            if let Some(link) = link {
                motion_ui::icons::draw(
                    list,
                    link,
                    motion_ui::icons::Icon::Link,
                    14.,
                    if parent_enabled {
                        colors.TEXT_MUTED
                    } else {
                        colors.TEXT_MUTED.with_alpha(0.4)
                    },
                );
            }
            let mut link_input = input.clone();
            if parent_enabled && link.is_some_and(|rect| clicked(input, rect)) {
                if let Some(parent) = parent {
                    link_input.pressed =
                        Some((MouseButton::Left, [parent.min[0] + 1., parent.min[1] + 1.]));
                }
            }
            let opened_matte = match matte {
                Some(rect) => panels.timeline_columns.cell(
                    list,
                    atlas,
                    rect,
                    row.id,
                    super::timeline_columns::Column::Matte,
                    &super::timeline_columns::matte_name(owner, state, locale),
                    input,
                    matte_enabled,
                ),
                _ => false,
            };
            let opened_blend = match blend {
                Some(rect) if blend_supported => panels.timeline_columns.cell(
                    list,
                    atlas,
                    rect,
                    row.id,
                    super::timeline_columns::Column::Blend,
                    super::timeline_columns::blend_name(owner, locale),
                    input,
                    blend_enabled,
                ),
                _ => false,
            };
            let opened_parent = match parent {
                Some(rect) => panels.timeline_columns.cell(
                    list,
                    atlas,
                    rect,
                    row.id,
                    super::timeline_columns::Column::Parent,
                    &super::timeline_columns::parent_name(owner, state, locale),
                    &link_input,
                    parent_enabled,
                ),
                None => false,
            };
            if opened_blend || opened_parent || opened_matte {
                panels.select_only(Some(row.id));
            }
        }
        let eye = Rect::new(area.min[0] + 10.0, y, 16.0, row_height);
        let lock = Rect::new(area.min[0] + 28.0, y, 16.0, row_height);
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            eye,
            "",
            row.visible,
            input,
            row.id != 0,
            false,
            Some(motion_ui::icons::Icon::Eye),
        )
        .clicked
        {
            out.push(Action::Edit(Command::Flags {
                object: row.id,
                visible: !row.visible,
                locked: row.locked,
            }));
        }
        if motion_ui::widgets::Checkbox::draw(
            list,
            atlas,
            lock,
            "",
            row.locked,
            input,
            row.id != 0,
            false,
            Some(motion_ui::icons::Icon::Lock),
        )
        .clicked
        {
            out.push(Action::Edit(Command::Flags {
                object: row.id,
                visible: row.visible,
                locked: !row.locked,
            }));
        }
        if clicked(input, name) {
            select_row(
                panels,
                &rows,
                index,
                row.id,
                input.modifiers.control,
                input.modifiers.shift,
            );
            panels.properties.field = None;
            if row.id != 0 && panels.timeline.timeline_query.is_empty() {
                panels.timeline.reorder = Some((row.id, input.mouse, false));
            }
        }
        if graph_mode {
            continue;
        }
        let left = x_of(row.range[0]).max(track.min[0]);
        let right = x_of(row.range[1]).min(track.max[0]);
        if right > left {
            let bar = Rect::new(left, y + 5.0, (right - left).max(0.0), row_height - 10.0);
            paint::rect(
                list,
                bar,
                colors.TRACKS[index % colors.TRACKS.len()].with_alpha(if row.visible {
                    0.7
                } else {
                    0.2
                }),
            );
            if selected {
                paint::stroke(list, bar, colors.ACCENT, 1.0);
            }
            if !row.locked && bar.contains(input.mouse) {
                panels.cursor = Some(motion_ui::cursor::Cursor::Grab);
            }
            for (head, x) in [
                (true, x_of(row.range[0])),
                (false, x_of(row.range[1])),
            ] {
                let handle = Rect::new(x - 4.0, y + 4.0, 8.0, row_height - 8.0);
                if selected && x >= track.min[0] && x <= track.max[0] && !row.locked {
                    paint::rect(list, handle, colors.ACCENT);
                    if handle.contains(input.mouse) {
                        panels.cursor = Some(motion_ui::cursor::Cursor::ResizeEW);
                    }
                    if clicked(input, handle) {
                        panels.timeline.trimming = Some((
                            row.id,
                            row.range[0],
                            row.range[1],
                            head,
                            if head { row.range[0] } else { row.range[1] },
                        ));
                        out.push(Action::History(2));
                    }
                }
            }
            if clicked(input, bar) && panels.timeline.trimming.is_none() {
                if input.modifiers.control
                    || input.modifiers.shift
                    || !panels.selection_contains(row.id)
                {
                    select_row(
                        panels,
                        &rows,
                        index,
                        row.id,
                        input.modifiers.control,
                        input.modifiers.shift,
                    );
                }
                if !row.locked && !input.modifiers.control && !input.modifiers.shift {
                    panels.selected = Some(row.id);
                    panels.selection_anchor = Some(row.id).filter(|id| *id != 0);
                    panels.animation_controls.clear_key();
                    let ids = panels.selection_ids();
                    let moving = ids
                        .into_iter()
                        .filter_map(|id| {
                            all_rows
                                .iter()
                                .find(|candidate| candidate.id == id)
                                .filter(|candidate| !candidate.locked)
                                .map(|candidate| {
                                    (
                                        candidate.id,
                                        candidate.clip[0],
                                        candidate.clip[1] - candidate.clip[0],
                                    )
                                })
                        })
                        .collect::<Vec<_>>();
                    if !moving.is_empty() {
                        panels.timeline.moving = Some(super::types::MovingLayers {
                            start_mouse: input.mouse[0],
                            initial: moving[0].1,
                            last: moving[0].1,
                            layers: moving,
                        });
                        out.push(Action::History(2));
                    }
                }
            }
        }
        for f in row.keys.iter().filter(|_| !expanded) {
            let x = x_of(*f);
            if x >= track.min[0] + 4.0 && x < track.max[0] - 4.0 {
                paint::diamond(list, [x, y + row_height * 0.5], 4.0, colors.KEYFRAME);
            }
        }
    }
    super::path_controls::timeline::process(
        list, &all_rows, body, ppf, offset, panels, state, input, out,
    );
    super::path_controls::groups::timeline::process(
        list, panels, state, body, ppf, offset, input, out,
    );
    if !graph_mode && clicked(input, body) && !timeline_hit {
        panels.select_only(None);
        panels.properties.field = None;
    }
    if graph_mode {
        super::timeline_graph::render(
            list,
            atlas,
            Rect::new(track.min[0], body.min[1], track.width(), body.height() + footer_height),
            panels,
            state,
            input,
            offset as f64,
            ppf,
            out,
        );
    }
    if let Some((object, start, end, head, last)) = panels.timeline.trimming.as_mut() {
        let _ = observe!(
            "timeline.trim_release",
            (input.dragging() || input.released.is_some())
                .then_some(())
                .ok_or("gesture retained without held button or release"),
            "object={object} phase=trimming"
        );
        let position = ((input.mouse[0] - track.min[0]) / ppf + offset) as f64;
        let position = if input.modifiers.alt {
            position
        } else {
            position.round()
        };
        let snap = if input.modifiers.alt {
            None
        } else {
            motion_ui::snap::nearest(
                candidates_for(Some(*object), true),
                position,
                ppf as f64,
                8.,
            )
        };
        panels.timeline.snap_guide = snap.map(|s| s.target);
        let position = snap.map_or(position, |s| s.value);
        let position = position.clamp(
            -(motion_core::MAX_FRAMES as f64),
            motion_core::MAX_FRAMES as f64,
        );
        let next = if *head {
            position.min(*end - 0.000001)
        } else {
            position.max(*start + 0.000001)
        };
        if next != *last && (input.dragging() || input.released.is_some()) {
            out.push(Action::Edit(Command::SetLayerRange {
                object: *object,
                in_frame: if *head { next } else { *start },
                out_frame: if *head { *end } else { next },
            }));
            *last = next;
        }
        if input.released.is_some() {
            out.push(Action::History(3));
            panels.timeline.trimming = None;
        }
    }
    if let Some(moving) = panels.timeline.moving.as_mut() {
        let _ = observe!(
            "timeline.move_release",
            (input.dragging() || input.released.is_some())
                .then_some(())
                .ok_or("gesture retained without held button or release"),
            "phase=moving layer_count={} initial={}",
            moving.layers.len(),
            moving.initial
        );
        let (_, _, duration) = moving.layers[0];
        let raw = moving.initial as f64 + (input.mouse[0] - moving.start_mouse) as f64 / ppf as f64;
        let mut candidates = vec![0., total, state["frame"].as_f64().unwrap_or(0.)];
        for row in all_rows
            .iter()
            .filter(|row| !moving.layers.iter().any(|(id, _, _)| *id == row.id))
        {
            candidates.extend(row.range);
            candidates.extend(row.keys.iter().copied());
            candidates.extend(row.references.iter().copied());
        }
        for key in ["in_frame", "out_frame"] {
            if let Some(v) = state["project"]["work_area"][key].as_f64() {
                candidates.push(v);
            }
        }
        let snap = if input.modifiers.alt {
            None
        } else {
            let a = motion_ui::snap::nearest(candidates.iter().copied(), raw, ppf as f64, 8.);
            let b = motion_ui::snap::nearest(
                candidates.iter().copied(),
                raw + duration as f64,
                ppf as f64,
                8.,
            );
            match (a, b) {
                (Some(a), Some(b)) => Some(if a.distance_pixels <= b.distance_pixels {
                    (a.value, a.target)
                } else {
                    (b.value - duration as f64, b.target)
                }),
                (Some(a), None) => Some((a.value, a.target)),
                (None, Some(b)) => Some((b.value - duration as f64, b.target)),
                _ => None,
            }
        };
        panels.timeline.snap_guide = snap.map(|s| s.1);
        let delta = moving.clamp_delta(
            (snap.map_or(raw, |s| s.0).round() as i64).saturating_sub(i64::from(moving.initial)),
        );
        let next = moving.initial + delta;
        if next != moving.last && (input.dragging() || input.released.is_some()) {
            out.push(Action::EditBatch(
                moving
                    .layers
                    .iter()
                    .map(|(object, in_frame, _)| Command::MoveLayerClip {
                        object: *object,
                        in_frame: in_frame + delta,
                    })
                    .collect(),
            ));
            moving.last = next;
        }
        if input.released.is_some() {
            out.push(Action::History(3));
            panels.timeline.moving = None;
        }
    }
    // Layer z-order drag: an accent line previews the insertion slot and
    // releasing issues a single Reorder edit. Displayed rows are the layer
    // stack reversed, so the slot converts back to a storage index.
    if let Some((object, press, active)) = panels.timeline.reorder {
        let slot_of = |y: f32| {
            let mut slot = row_bounds.len();
            let mut line = row_bounds.last().map_or(body.min[1], |(_, top, h)| top + h);
            for (i, (_, top, h)) in row_bounds.iter().enumerate() {
                if y < top + h * 0.5 {
                    slot = i;
                    line = *top;
                    break;
                }
            }
            (slot, line)
        };
        if input.held == Some(MouseButton::Left) {
            let active = active || (input.mouse[1] - press[1]).abs() > 4.;
            if active {
                let (slot, line) = slot_of(input.mouse[1]);
                paint::rect(
                    list,
                    Rect::new(
                        area.min[0] + 60.,
                        line - 1.,
                        track.max[0] - area.min[0] - 60.,
                        2.,
                    ),
                    colors.ACCENT,
                );
                panels.cursor = Some(motion_ui::cursor::Cursor::Grabbing);
                panels.drag_readout = Some((input.mouse, format!("#{}", slot + 1)));
            }
            panels.timeline.reorder = Some((object, press, active));
        } else {
            if active {
                let (slot, _) = slot_of(input.mouse[1]);
                let from = row_bounds.iter().position(|(id, _, _)| *id == object);
                let len = state["project"]["layers"].as_array().map_or(0, Vec::len);
                if let Some(index) = from.and_then(|from| reorder_storage_index(len, from, slot)) {
                    out.push(Action::Edit(Command::Reorder { object, index }));
                }
            }
            panels.timeline.reorder = None;
        }
    }
    if panels.timeline.scrubbing {
        panels.cursor = Some(motion_ui::cursor::Cursor::ResizeEW);
        let frame = scrub_frame.unwrap_or(state["frame"].as_f64().unwrap_or(0.));
        panels.drag_readout = Some((
            input.mouse,
            format!(
                "{}  ({:+.0} f)",
                timecode(frame, fps as u32),
                frame - state["frame"].as_f64().unwrap_or(0.)
            ),
        ));
    } else if let Some(moving) = &panels.timeline.moving {
        panels.cursor = Some(motion_ui::cursor::Cursor::Grabbing);
        panels.drag_readout = Some((
            input.mouse,
            format!(
                "{}  ({:+} f)",
                timecode(moving.last as f64, fps as u32),
                moving.last - moving.initial
            ),
        ));
    } else if let Some((_, start, end, head, last)) = &panels.timeline.trimming {
        panels.cursor = Some(motion_ui::cursor::Cursor::ResizeEW);
        panels.drag_readout = Some((
            input.mouse,
            format!(
                "{}  ({:+.2} f)",
                timecode(*last, fps as u32),
                last - if *head { *start } else { *end }
            ),
        ));
    }
    if panels.timeline.scrubbing
        || panels.timeline.moving.is_some()
        || panels.timeline.trimming.is_some()
    {
        if let Some(frame) = panels.timeline.snap_guide {
            let x = x_of(frame);
            if x >= track.min[0] && x <= track.max[0] {
                paint::rect(
                    list,
                    Rect::new(x, ruler.min[1], 1., body.max[1] - ruler.min[1]),
                    motion_ui::Color::rgb(255, 210, 60),
                );
            }
        }
    } else {
        panels.timeline.snap_guide = None;
    }
    let frame = if panels.timeline.scrubbing {
        scrub_frame.unwrap_or(state["frame"].as_f64().unwrap_or(0.))
    } else {
        state["frame"].as_f64().unwrap_or(0.)
    };
    let target = x_of(frame);
    let x = panels.motion.value(
        (0, 2),
        target,
        target,
        styles.as_ref().metric("motion.seek_ms", 90.),
        panels.timeline.scrubbing || panels.playing,
    );
    if x >= track.min[0] && x < track.max[0] {
        paint::rect(
            list,
            Rect::new(x, ruler.min[1], 1.0, body.max[1] - ruler.min[1]),
            colors.ACCENT,
        );
        paint::diamond(list, [x, ruler.min[1] + 6.0], 6.0, colors.ACCENT);
    }

    if graph_mode {
        label(list, atlas, Rect::new(area.min[0], area.max[1] - footer_height, gutter, footer_height),
            if panels.timeline.speed_graph { if locale == Locale::Zh { "速度图表" } else { "Speed graph" } }
            else { if locale == Locale::Zh { "数值图表" } else { "Value graph" } }, true);
    }
    super::timeline_navigation::paint(list, track, footer_height, total as f32, &panels.timeline, input);
    panels
        .timeline_columns
        .overlay(list, atlas, area, state, context_input, locale, out);
    if right_clicked(input, body)
        && !panels.layer_actions.editing()
        && !panels.path_controls.mask_menu()
        && !panels.expressions.menu_open()
    {
        panels.layer_actions.open(None, input.mouse, state);
    }
    panels
        .path_controls
        .mask_overlay(list, atlas, area, state, context_input, locale, out);
    if let Some((rect, dragging)) = panels.path_controls.groups.cursor {
        if dragging || rect.contains(context_input.mouse) {
            panels.cursor = Some(motion_ui::widgets::numeric_behavior::cursor(
                context_input,
                dragging,
            ));
        }
    }
}

#[cfg(test)]
mod reorder_tests {
    use super::reorder_storage_index;

    #[test]
    fn drag_top_layer_below_bottom_maps_to_storage_front() {
        // Displayed [C, B, A] over storage [A, B, C]: C below A is slot 3.
        assert_eq!(reorder_storage_index(3, 0, 3), Some(0));
    }
    #[test]
    fn drag_bottom_layer_to_top_maps_to_storage_back() {
        assert_eq!(reorder_storage_index(3, 2, 0), Some(2));
    }
    #[test]
    fn own_edge_slots_are_noops() {
        assert_eq!(reorder_storage_index(3, 1, 1), None);
        assert_eq!(reorder_storage_index(3, 1, 2), None);
    }
    #[test]
    fn middle_move_adjusts_for_removal() {
        // Displayed [C, B, A]: move A (row 2) above B (slot 1) → storage [A?]
        // A at storage 0, remove then insert above B's slot → [B, A, C].
        assert_eq!(reorder_storage_index(3, 2, 1), Some(1));
    }
    #[test]
    fn degenerate_inputs_are_noops() {
        assert_eq!(reorder_storage_index(0, 0, 0), None);
        assert_eq!(reorder_storage_index(3, 3, 0), None);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    fn fixture() -> Value {
        serde_json::json!({"frame":21,"project":{"frames":120,"fps":30,"layers":[{"id":7,"name":"Layer","visible":true,"three_d":true}]},"sampledLayers":[{"id":7,"position":[10,20,30],"rotation":[0,0,0],"scale":[100,100,100],"opacity":1}],"timeline_layers":[{"object":7,"properties":{"position":{"separated":true,"axes":{"x":{"keys":[{"frame":0},{"frame":60}]},"y":{"keys":[{"frame":15},{"frame":90}]},"z":{"keys":[]}}}}}]})
    }
    #[test]
    fn playhead_never_snaps_to_itself_and_repaint_drops_stale_guide() {
        let mut panels = Panels::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 1200., 600.);
        let gutter = GUTTER.min(1200. * 0.52);
        let ppf = (1200. - gutter - 8.) / 120.;
        let point = |frame: f32| [gutter + frame * ppf, 45.];
        let mut actions = vec![];
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &Input {
                mouse: point(22.),
                pressed: Some((MouseButton::Left, point(22.))),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            &mut actions,
        );
        assert!(matches!(actions.as_slice(),[Action::Seek(frame)] if *frame==22.));
        assert_eq!(panels.timeline.snap_guide, None);
        actions.clear();
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &Input {
                mouse: point(60.),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            &mut actions,
        );
        assert_eq!(panels.timeline.snap_guide, Some(60.));
        actions.clear();
        // Pending seeks can repaint at a newer pointer without consuming its input.
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &Input {
                mouse: point(40.),
                ..Default::default()
            },
            &mut actions,
        );
        assert!(actions.is_empty());
        assert_eq!(panels.timeline.snap_guide, None);
        let mut alt = Input {
            mouse: point(60.),
            ..Default::default()
        };
        alt.modifiers.alt = true;
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &alt,
            &mut actions,
        );
        assert!(actions.is_empty());
        assert_eq!(panels.timeline.snap_guide, None);
    }
    #[test]
    fn holding_the_same_timeline_frame_leaves_room_for_presentation() {
        let mut panels = Panels::default();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let point = [760., 45.];
        let mut input = Input {
            mouse: point,
            pressed: Some((MouseButton::Left, point)),
            held: Some(MouseButton::Left),
            ..Default::default()
        };
        let area = Rect::new(0., 0., 1200., 600.);
        let mut actions = vec![];
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &input,
            &mut actions,
        );
        assert!(matches!(actions.as_slice(), [Action::Seek(_)]));
        actions.clear();
        input.begin_frame();
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &input,
            &mut actions,
        );
        assert!(actions.is_empty());
        input.mouse[0] += 40.;
        render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &fixture(),
            &input,
            &mut actions,
        );
        assert!(matches!(actions.as_slice(), [Action::Seek(_)]));
    }
    #[test]
    fn separated_rows_use_their_own_key_times_and_static_axis_stays_empty() {
        let rows = property_rows(&fixture(), 7);
        assert_eq!(rows[0].keys, vec![0., 60.]);
        assert_eq!(rows[1].keys, vec![15., 90.]);
        assert!(rows[2].keys.is_empty());
        assert_eq!(rows[0].axis, Some(0));
        assert_eq!(rows[1].axis, Some(1));
        assert_eq!(rows[2].axis, Some(2));
    }
    #[test]
    fn timeline_numeric_input_uses_the_shared_component_command() {
        let state = fixture();
        let mut panels = Panels {
            selected: Some(7),
            focused: Some(Panel::Timeline),
            ..Default::default()
        };
        panels.timeline.expanded.insert(7);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 1200., 600.);
        let point = [335., 146.];
        let mut actions = vec![];
        for input in [
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            Input {
                mouse: point,
                text: "32.5".into(),
                ..Default::default()
            },
            Input {
                mouse: point,
                keys: vec![(Key::Enter, Default::default())],
                ..Default::default()
            },
        ] {
            render(
                &mut PaintList::default(),
                &mut atlas,
                area,
                &mut panels,
                &state,
                &input,
                &mut actions,
            );
        }
        assert!(
            matches!(actions.as_slice(),[Action::Edit(Command::SetComponent{object:7,property:Property::Position,axis:motion_core::Axis::X,frame:21,value})] if *value==32.5)
        );
    }

    #[test]
    fn row_selection_supports_toggle_and_range_without_losing_primary() {
        let state = serde_json::json!({
            "project": {"frames": 120, "layers": [
                {"id": 7, "name": "Bottom", "content": {"kind": "solid"}},
                {"id": 8, "name": "Middle", "content": {"kind": "solid"}},
                {"id": 9, "name": "Top", "content": {"kind": "solid"}}
            ]}
        });
        let owned = rows(&state);
        let references = owned.iter().collect::<Vec<_>>();
        let mut panels = Panels::default();
        select_row(&mut panels, &references, 0, references[0].id, false, false);
        select_row(&mut panels, &references, 2, references[2].id, false, true);
        assert_eq!(panels.selected, Some(references[2].id));
        assert_eq!(panels.selection_ids().len(), 3);
        select_row(&mut panels, &references, 1, references[1].id, false, true);
        assert_eq!(panels.selection_ids().len(), 2);
        assert!(panels.selection_contains(references[0].id));
        assert!(!panels.selection_contains(references[2].id));
        select_row(&mut panels, &references, 2, references[2].id, false, true);
        select_row(&mut panels, &references, 1, references[1].id, true, false);
        assert_eq!(panels.selection_ids().len(), 2);
        assert!(!panels.selection_contains(references[1].id));
    }
}

pub(super) fn render_layer_properties(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    body: Rect,
    track: Rect,
    y: f32,
    row_height: f32,
    ppf: f32,
    offset: f32,
    row: &Row,
    layout: &super::timeline_layout::LayerRows,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    out: &mut Vec<Action>,
) {
    let colors = list.styles().colors();
    let locale = panels.locale;
    let gutter = area.width();
    let total = last_frame(state) + 1.;
    let graph_mode = panels.timeline.graph_mode;
    let transform_expanded = layout.transform_open;
    let x_of = |frame: f64| track.min[0] + (frame as f32 - offset) * ppf;
    if !layout.transform.is_empty() {
        let group_y = y + layout.transform.start as f32 * row_height;
        if group_y >= body.min[1] && group_y + row_height <= body.max[1] {
            let group = property_group(
                list,
                atlas,
                Rect::new(area.min[0], group_y, gutter, row_height),
                group_y,
                row_height,
                locale.t(T::Transform),
                transform_expanded,
            );
            if clicked(input, group) {
                if transform_expanded {
                    panels.timeline.collapsed_transforms.insert(row.id);
                } else {
                    panels.timeline.collapsed_transforms.remove(&row.id);
                    panels.timeline.reopen_properties(row.id);
                }
            }
        }
        if let Some(index) = layout.anchor {
            let py = y + index as f32 * row_height;
            if py >= body.min[1] && py + row_height <= body.max[1] {
                let fields = property_layout(&list.styles(), area, py, row_height, 0, 2);
                property_branch(list, fields.watch);
                property_icon(
                    list,
                    fields.watch,
                    motion_ui::icons::Icon::Anchor,
                    false,
                    !row.locked,
                );
                label(
                    list,
                    atlas,
                    fields.caption,
                    if locale == Locale::Zh {
                        "锚点"
                    } else {
                        "Anchor"
                    },
                    true,
                );
                if let Some(layer) = state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|layer| layer["id"].as_u64() == Some(row.id))
                {
                    panels.timeline_anchor.draw(
                        list,
                        atlas,
                        area,
                        py,
                        Panel::Timeline,
                        layer,
                        input,
                        row.locked,
                        out,
                    );
                }
            }
        }
        let sample = state["sampledLayers"]
            .as_array()
            .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(row.id)));
        for (i, property_index) in layout.properties.iter().enumerate() {
            let property = &row.properties[*property_index];
            let py = y + layout.property_offsets[i] as f32 * row_height;
            if py < body.min[1] || py + row_height > body.max[1] {
                continue;
            }
            let axis = property.axis.map(|a| {
                [
                    motion_core::Axis::X,
                    motion_core::Axis::Y,
                    motion_core::Axis::Z,
                ][a]
            });
            let property_area = Rect::new(area.min[0], py, gutter, row_height);
            let fields = property_layout(
                list.styles().as_ref(),
                property_area,
                py,
                row_height,
                0,
                property.axes.len(),
            );
            property_branch(list, fields.watch);
            property_icon(
                list,
                fields.watch,
                motion_ui::icons::Icon::Stopwatch,
                !property.keys.is_empty(),
                !row.locked,
            );
            let expression_target = motion_core::ExpressionTarget::Property {
                object: row.id,
                property: property.property,
                axis,
            };
            if let Some(index) = layout.expression_offsets[i] {
                let ey = y + index as f32 * row_height;
                if ey >= body.min[1] && ey + row_height <= body.max[1] {
                    panels.expressions.row(
                        list,
                        atlas,
                        Rect::new(
                            fields.caption.min[0],
                            ey,
                            (track.min[0] - fields.caption.min[0]).max(0.),
                            row_height,
                        ),
                        Rect::new(track.min[0], ey, track.width(), row_height),
                        state,
                        expression_target.clone(),
                        input,
                        locale,
                        !row.locked && state["observing"] != true,
                        out,
                    );
                }
            }
            if panels.expressions.context(
                expression_target.clone(),
                property_area,
                input,
                Panel::Timeline,
            ) {
                panels.layer_actions.cancel();
            }
            if clicked(input, fields.watch) && !row.locked && state["observing"] != true {
                if input.modifiers.alt {
                    panels
                        .expressions
                        .open(state, expression_target, Panel::Timeline);
                } else {
                    out.push(Action::Edit(Command::Animate {
                        object: row.id,
                        property: property.property,
                        axis,
                        frame: state["frame"].as_f64().unwrap_or(0.).floor() as u32,
                        enabled: property.keys.is_empty(),
                    }));
                }
            }
            let text = property
                .axis
                .map(|a| format!("{} {}", locale.t(property.title), ["X", "Y", "Z"][a]))
                .unwrap_or_else(|| locale.t(property.title).into());
            label(list, atlas, fields.caption, &text, true);
            if clicked(input, fields.caption) {
                panels.select_only(Some(row.id));
                panels
                    .animation_controls
                    .choose_track(row.id, property.property, axis);
                panels.animation_controls.clear_key();
            }
            for (j, component) in property.axes.iter().enumerate() {
                let value = if property.property == Property::Opacity {
                    sample
                        .map(|s| s[property.key].as_f64().unwrap_or(1.) * 100.)
                        .unwrap_or(100.)
                } else {
                    sample
                        .map(|s| s[property.key].as_f64().or_else(|| s[property.key][*component].as_f64()).unwrap_or(0.))
                        .unwrap_or(0.)
                };
                let field = property_layout(
                    list.styles().as_ref(),
                    property_area,
                    py,
                    row_height,
                    j,
                    property.axes.len(),
                )
                .number;
                if clicked(input, field) {
                    panels.select_only(Some(row.id));
                    panels
                        .animation_controls
                        .choose_track(row.id, property.property, axis);
                    panels.timeline.graph_component = *component;
                }
                super::properties::draw_value(
                    list,
                    atlas,
                    field,
                    panels,
                    state,
                    input,
                    row.id,
                    property.property,
                    *component,
                    value,
                    row.locked,
                    true,
                    out,
                );
            }
            for frame in property.keys.iter().filter(|_| !graph_mode) {
                let x = x_of(*frame);
                if x >= track.min[0] + 4. && x < track.max[0] - 4. {
                    let current = (frame - state["frame"].as_f64().unwrap_or(0.)).abs() < 0.0001;
                    paint::diamond(
                        list,
                        [x, py + row_height * 0.5],
                        4.,
                        if current {
                            colors.KEYFRAME
                        } else {
                            colors.ACCENT
                        },
                    );
                    if clicked(input, Rect::new(x - 5., py, 10., row_height)) {
                        panels.select_only(Some(row.id));
                        panels.animation_controls.select_key(
                            row.id,
                            property.property,
                            axis,
                            *frame as i64,
                        );
                        out.push(Action::Seek(frame.clamp(0., total - 1.)));
                    }
                }
            }
        }
    }
    if !layout.audio.is_empty() {
        let audio_y = y + layout.audio.start as f32 * row_height;
        let open = layout.audio.len() > 1;
        if audio_y >= body.min[1] && audio_y + row_height <= body.max[1] {
            let group = property_group(
                list,
                atlas,
                Rect::new(area.min[0], audio_y, gutter, row_height),
                audio_y,
                row_height,
                if locale == Locale::Zh {
                    "音频"
                } else {
                    "Audio"
                },
                open,
            );
            if clicked(input, group) {
                if open {
                    panels.timeline.collapsed_audio.insert(row.id);
                } else {
                    panels.timeline.collapsed_audio.remove(&row.id);
                }
            }
        }
        if open {
            super::audio::render_rows(
                list,
                atlas,
                Rect::new(area.min[0], body.min[1], gutter, body.height()),
                body,
                track,
                audio_y + row_height,
                row_height,
                ppf,
                offset,
                row.id,
                panels,
                state,
                input,
                out,
            );
        }
    }
    if !layout.geometry.is_empty() {
        let gy = y + layout.geometry.start as f32 * row_height;
        if gy >= body.min[1] && gy + row_height <= body.max[1] {
            let heading = property_group(
                list,
                atlas,
                area,
                gy,
                row_height,
                if locale == Locale::Zh {
                    "几何设置"
                } else {
                    "Geometry"
                },
                layout.geometry_open,
            );
            if clicked(input, heading) {
                if !panels.timeline.collapsed_geometry.remove(&row.id) {
                    panels.timeline.collapsed_geometry.insert(row.id);
                }
            }
        }
        if layout.geometry_open {
            let py = gy + row_height;
            if py >= body.min[1] && py + row_height <= body.max[1] {
                let fields = property_layout(&list.styles(), area, py, row_height, 0, 1);
                property_branch(list, fields.watch);
                label(list, atlas, fields.caption, "3D", true);
                if let Some(layer) = state["project"]["layers"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|layer| layer["id"].as_u64() == Some(row.id))
                {
                    let enabled = !row.locked && state["observing"] != true;
                    if motion_ui::widgets::Checkbox::draw(
                        list,
                        atlas,
                        fields.number,
                        "",
                        layer["three_d"] == true,
                        input,
                        enabled,
                        false,
                        None,
                    )
                    .clicked
                    {
                        out.push(Action::Edit(Command::SetLayer3d {
                            object: row.id,
                            enabled: layer["three_d"] != true,
                        }));
                    }
                }
            }
        }
    }
    let start = y + layout.masks.start as f32 * row_height;
    super::path_controls::timeline::render(
        list,
        atlas,
        area,
        body,
        track,
        start,
        row_height,
        ppf,
        offset,
        row,
        &layout.mask_rows,
        panels,
        state,
        input,
        out,
    );
    let vector_start = y + layout.vectors.start as f32 * row_height;
    super::path_controls::groups::timeline::render(
        list,
        atlas,
        area,
        body,
        track,
        vector_start,
        row_height,
        ppf,
        offset,
        row,
        &layout.vector_rows,
        panels,
        state,
        input,
        out,
    );
    if !layout.effects.is_empty() {
        let ey = y + layout.effects.start as f32 * row_height;
        if ey >= body.min[1] && ey + row_height <= body.max[1] {
            let heading = property_group(
                list,
                atlas,
                area,
                ey,
                row_height,
                if locale == Locale::Zh {
                    "效果"
                } else {
                    "Effects"
                },
                layout.effects_open,
            );
            if clicked(input, heading) {
                if !panels.timeline.expanded_effects.remove(&row.id) {
                    panels.timeline.expanded_effects.insert(row.id);
                }
            }
        }
        if layout.effects_open {
            let effects = state["project"]["layers"].as_array().into_iter().flatten()
                .find(|layer| layer["id"].as_u64() == Some(row.id)).and_then(|layer| layer["effects"].as_array());
            for (index, effect) in effects.into_iter().flatten().enumerate() {
                let py = ey + (index + 1) as f32 * row_height;
                if py < body.min[1] || py + row_height > body.max[1] {
                    continue;
                }
                let fields = property_layout(&list.styles(), area, py, row_height, 0, 1);
                property_branch(list, fields.watch);
                if motion_ui::widgets::Checkbox::draw(
                    list,
                    atlas,
                    fields.watch,
                    "",
                    effect["enabled"] != false,
                    input,
                    !row.locked && state["observing"] != true,
                    false,
                    None,
                )
                .clicked
                {
                    if let Some(id) = effect["id"].as_u64() {
                        out.push(Action::Edit(Command::Effect {
                            object: row.id,
                            action: motion_core::EffectAction::Enable {
                                effect: id,
                                enabled: effect["enabled"] == false,
                            },
                        }));
                    }
                }
                let definition = panels.effect_controls.definition(effect);
                let title = definition
                    .as_ref()
                    .and_then(|definition| {
                        definition[if locale == Locale::Zh {
                            "name"
                        } else {
                            "english_name"
                        }]
                        .as_str()
                    })
                    .unwrap_or_else(|| effect["effect"].as_str().unwrap_or("Effect"));
                label(list, atlas, fields.caption, title, false);
                let edit = ui::button_styled(
                    list,
                    atlas,
                    fields.number,
                    if locale == Locale::Zh {
                        "编辑"
                    } else {
                        "Edit"
                    },
                    input,
                    true,
                    &list.styles(),
                    "button.icon",
                )
                .clicked;
                if clicked(input, fields.caption) || edit {
                    panels.select_only(Some(row.id));
                    out.push(Action::ShowPanel(Panel::EffectControls.dock()));
                }
                let mut frames = vec![];
                for parameter in effect["params"]
                    .as_object()
                    .into_iter()
                    .flat_map(|params| params.values())
                {
                    for track in [&parameter["track"], &parameter["curve"]] {
                        frames.extend(
                            track["keys"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|key| key["frame"].as_f64())
                                .map(|frame| frame + f64::from(row.mask_offset)),
                        );
                    }
                }
                frames.sort_by(f64::total_cmp);
                frames.dedup();
                for frame in frames {
                    let x = x_of(frame);
                    if x < track.min[0] || x >= track.max[0] {
                        continue;
                    }
                    let radius = list.styles().metric("graph.key_radius", 5.);
                    paint::diamond(
                        list,
                        [x, py + row_height * 0.5],
                        list.styles().metric("graph.handle_radius", 4.),
                        colors.KEYFRAME,
                    );
                    if clicked(input, Rect::new(x - radius, py, radius * 2., row_height)) {
                        panels.select_only(Some(row.id));
                        out.push(Action::Seek(frame.clamp(0., total - 1.)));
                    }
                }
            }
        }
    }
}

pub(super) fn render_time_ruler(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    ruler: Rect,
    panels: &mut Panels,
    state: &Value,
    input: &Input,
    ppf: f32,
    offset: f32,
    seek_candidates: &[f64],
    out: &mut Vec<Action>,
) -> Option<f64> {
    let colors = list.styles().colors();
    let track = ruler;
    let total = last_frame(state) + 1.;
    let x_of = |frame: f64| ruler.min[0] + (frame as f32 - offset) * ppf;
    let frame_of = |x: f32| {
        ((x - ruler.min[0]) / ppf + offset)
            .round()
            .clamp(0., total as f32 - 1.) as f64
    };
    if input.focus_lost || input.keys.iter().any(|(key, _)| *key == Key::Escape) {
        panels.timeline.scrubbing = false;
        panels.timeline.last_seek = None;
    }
    let fps = state["project"]["fps"].as_u64().unwrap_or(30).max(1) as f32;
    let step = super::timeline_layout::ruler_step(ppf, fps);
    let mut f = (offset / step).ceil() * step;
    while x_of(f as f64) < track.max[0] {
        let x = x_of(f as f64);
        paint::rect(
            list,
            Rect::new(x, ruler.min[1] + 1., 1.0, 5.0),
            colors.TEXT_MUTED,
        );
        label(
            list,
            atlas,
            Rect::new(x + 3.0, ruler.min[1], 52.0, 14.0),
            &format!("{:.0}s", f / fps),
            true,
        );
        f += step;
    }
    let marker_owned =
        super::audio::render_ruler(list, atlas, ruler, ppf, offset, panels, state, input, out);
    if clicked(input, ruler) && !marker_owned {
        panels.timeline.scrubbing = true;
        panels.timeline.scrub_ruler = ScrubRuler::Timeline;
        panels.timeline.last_seek = None;
    }
    let owns_scrub =
        panels.timeline.scrubbing && panels.timeline.scrub_ruler == ScrubRuler::Timeline;
    let scrub_frame = if owns_scrub {
        let raw = frame_of(input.mouse[0]);
        let snap = if input.modifiers.alt {
            None
        } else {
            motion_ui::snap::nearest(
                seek_candidates
                    .iter()
                    .copied()
                    .filter(|v| *v >= 0. && *v <= total - 1.),
                raw,
                ppf as f64,
                8.,
            )
        };
        panels.timeline.snap_guide = snap.map(|s| s.target);
        Some(snap.map_or(raw, |s| s.value))
    } else {
        None
    };
    if let Some(frame) = scrub_frame
        .filter(|_| clicked(input, ruler) || input.dragging() || input.released.is_some())
    {
        if panels.timeline.last_seek != Some(frame) {
            panels.timeline.last_seek = Some(frame);
            out.push(Action::Seek(frame));
        }
    }
    if owns_scrub && input.released.is_some() {
        panels.timeline.scrubbing = false;
    }
    scrub_frame
}
