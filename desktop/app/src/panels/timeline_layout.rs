//! A frame's row geometry is fixed before any disclosure or selection input.
//! Labels, numeric fields, time lanes, culling and hit tests consume this plan.
use super::{path_controls, timeline::Row, types::TimelineState, Locale};
use motion_ui::{
    paint::{self, PaintList},
    style::{box_style, State, States},
    Input, Rect,
};
use std::ops::Range;

pub(super) struct LayerRows {
    pub expanded: bool,
    pub transform_open: bool,
    pub properties: Vec<usize>,
    pub transform: Range<usize>,
    pub anchor: Option<usize>,
    pub audio: Range<usize>,
    pub geometry: Range<usize>,
    pub effects: Range<usize>,
    pub geometry_open: bool,
    pub effects_open: bool,
    pub masks: Range<usize>,
    pub vectors: Range<usize>,
    pub mask_rows: path_controls::timeline::Rows,
    pub vector_rows: path_controls::groups::timeline::Rows,
}

pub(super) fn ruler_step(ppf: f32, fps: f32) -> f32 {
    (60. / (ppf * fps)).ceil().max(1.) * fps
}

/// One row rectangle spans both columns, including its hover hit region.
pub(super) fn paint_rows(
    list: &mut PaintList,
    body: Rect,
    top: f32,
    height: f32,
    count: usize,
    selected: bool,
    input: &Input,
) {
    let styles = list.styles();
    let first = ((body.min[1] - top) / height).floor().max(0.) as usize;
    let last = (((body.max[1] - top) / height).ceil().max(0.) as usize).min(count);
    for index in first..last {
        let rect = Rect::new(
            body.min[0],
            top + index as f32 * height,
            body.width(),
            height,
        );
        let mut states = States::default();
        if body.contains(input.mouse) && rect.contains(input.mouse) {
            states = states.with(State::Hovered);
        }
        if index == 0 && selected {
            states = states.with(State::Selected);
        }
        let class = if index % 2 == 0 {
            "timeline.row"
        } else {
            "timeline.row.alternate"
        };
        if let Some(style) = styles.resolve(class, states) {
            let mut row = list.child();
            box_style(&mut row, rect, style);
            let thickness = styles.metric("tree.separator.width", 0.5);
            paint::rect(
                &mut row,
                Rect::new(
                    rect.min[0],
                    rect.max[1] - thickness,
                    rect.width(),
                    thickness,
                ),
                styles
                    .color("border")
                    .with_alpha(styles.metric("tree.separator.opacity", 0.75)),
            );
            row.clip_to(body);
            list.append(row);
        }
    }
}

/// The ruler owns time ticks; the body only paints the label/lane boundary.
pub(super) fn paint_track_boundary(list: &mut PaintList, track: Rect, body: Rect) {
    let styles = list.styles();
    paint::rect(
        list,
        Rect::new(
            track.min[0],
            body.min[1],
            styles.metric("border", 1.),
            body.height(),
        ),
        styles.color("border"),
    );
}
impl LayerRows {
    pub fn new(row: &Row, t: &TimelineState, frame: f64, locale: Locale) -> Self {
        let expanded = row.id != 0 && t.expanded.contains(&row.id);
        let transform_open = expanded && row.spatial && !t.collapsed_transforms.contains(&row.id);
        let filter = t.filters.get(&row.id).copied().unwrap_or_default();
        let properties = row
            .properties
            .iter()
            .enumerate()
            .filter(|(_, p)| transform_open && filter.admits(p.property, !p.keys.is_empty()))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let anchor =
            (transform_open && !row.camera && matches!(filter, super::types::PropertyFilter::All)).then_some(2);
        let transform = 1..1
            + usize::from(expanded && row.spatial)
            + usize::from(anchor.is_some())
            + properties.len();
        let audio = transform.end
            ..transform.end
                + if expanded && row.audio > 0 {
                    1 + if t.collapsed_audio.contains(&row.id) {
                        0
                    } else {
                        row.audio
                    }
                } else {
                    0
                };
        let geometry_open = expanded && !t.collapsed_geometry.contains(&row.id);
        let effects_open = expanded && t.expanded_effects.contains(&row.id);
        let mask_rows = if geometry_open {
            path_controls::timeline::Rows::new(row, t)
        } else {
            Default::default()
        };
        let vector_rows = if geometry_open {
            path_controls::groups::timeline::Rows::new(row, t, frame, locale)
        } else {
            Default::default()
        };
        let geometry = audio.end
            ..audio.end
                + if expanded && row.spatial && !row.camera {
                    1 + usize::from(geometry_open)
                } else {
                    0
                };
        let masks = geometry.end..geometry.end + mask_rows.len();
        let vectors = masks.end..masks.end + vector_rows.len();
        let effects = vectors.end
            ..vectors.end
                + if expanded && row.id != 0 && !row.camera {
                    1 + if effects_open { row.effects } else { 0 }
                } else {
                    0
                };
        Self {
            expanded,
            transform_open,
            properties,
            transform,
            anchor,
            audio,
            geometry,
            effects,
            geometry_open,
            effects_open,
            masks,
            vectors,
            mask_rows,
            vector_rows,
        }
    }
    pub fn len(&self) -> usize {
        self.effects.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::{timeline, Panels};
    use motion_core::{
        vector::{
            groups::{GroupItem, GroupTransform, Repeater, VectorGroup},
            VectorContent, VectorPath,
        },
        Content, Layer,
    };
    use motion_ui::{text::TextAtlas, MouseButton};

    fn fixture() -> serde_json::Value {
        let mut layer = Layer::solid(7, "Mixed contents", [100.; 2], [0.; 3], [1.; 4]);
        layer.masks.push(motion_core::masks::LayerMask::new(
            1,
            VectorPath {
                id: 1,
                closed: true,
                nodes: vec![],
            },
        ));
        let mut repeater = Repeater::default();
        repeater.copies.set_animated(0, true).unwrap();
        repeater.copies.set_at(60, 8.).unwrap();
        layer.content = Content::Vector {
            vector: VectorContent {
                source: motion_core::vector::VectorSource::Group {
                    group: Box::new(VectorGroup {
                        id: 1,
                        name: "Group".into(),
                        transform: GroupTransform::default(),
                        items: vec![GroupItem::Repeater {
                            id: 3,
                            name: "Repeater".into(),
                            repeater,
                        }],
                    }),
                },
                fill: None,
                fill_rule: motion_core::vector::FillRule::NonZero,
                stroke: None,
                trim: None,
            },
        };
        serde_json::json!({"root":"rows-fixture", "revision":1, "frame":21, "project":{"frames":120,"fps":30,"layers":[layer]}})
    }
    fn panels() -> Panels {
        let mut panels = Panels::default();
        panels.focused = Some(motion_ui::dock::Panel::Timeline);
        panels.timeline.expanded.insert(7);
        panels.timeline.collapsed_transforms.insert(7);
        panels
            .timeline
            .vector_open
            .extend([(7, 0, 0), (7, 1, 0), (7, 3, 2)]);
        panels
    }
    #[test]
    fn folding_masks_does_not_move_following_shape_rows_until_next_frame() {
        let state = fixture();
        let mut panels = panels();
        let rows = timeline::rows(&state);
        let plan = LayerRows::new(&rows[0], &panels.timeline, 21., Locale::En);
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        let area = Rect::new(0., 0., 1280., 1000.);
        let point = [110., 64. + plan.masks.start as f32 * 22. + 11.];
        let mut actions = vec![];
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &state,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                ..Default::default()
            },
            &mut actions,
        );
        assert!(actions.is_empty(), "folding must not edit project data");
        assert!(panels.timeline.collapsed_masks.contains(&7));
        let rect = panels.path_controls.groups.rects[&(
            motion_ui::dock::Panel::Timeline as u32,
            7,
            3,
            "copies".into(),
            0,
        )]
            .1;
        // Vector header, group, collapsed group transform, repeater, then copies.
        assert_eq!(rect.min[1], 64. + (plan.vectors.start + 4) as f32 * 22.);
        timeline::render(
            &mut PaintList::default(),
            &mut atlas,
            area,
            &mut panels,
            &state,
            &Input::default(),
            &mut actions,
        );
        let next = panels.path_controls.groups.rects[&(
            motion_ui::dock::Panel::Timeline as u32,
            7,
            3,
            "copies".into(),
            0,
        )]
            .1;
        assert_eq!(
            rect.min[1] - next.min[1],
            (plan.masks.len() - 1) as f32 * 22.
        );
    }
    #[test]
    fn shape_key_centers_and_numeric_rows_share_scrolled_geometry_at_both_densities() {
        let state = fixture();
        let mut atlas = TextAtlas::from_system(12.).unwrap();
        for (width, height) in [(720., 22.), (1280., 32.)] {
            let sheet = PaintList::default()
                .styles()
                .with_overrides(
                    serde_json::json!({"tokens":{"sizes":{"row.height":height}}})
                        .to_string()
                        .as_bytes(),
                )
                .unwrap();
            let mut list = PaintList::with_styles(std::sync::Arc::new(sheet));
            let mut panels = panels();
            let rows = timeline::rows(&state);
            let plan = LayerRows::new(&rows[0], &panels.timeline, 21., Locale::En);
            panels.timeline.scroll = 37. + plan.geometry.len() as f32 * height;
            panels.timeline.zoom = 2.;
            panels.timeline.offset = 10.;
            timeline::render(
                &mut list,
                &mut atlas,
                Rect::new(0., 0., width, 500.),
                &mut panels,
                &state,
                &Input::default(),
                &mut vec![],
            );
            let rect = panels.path_controls.groups.rects[&(
                motion_ui::dock::Panel::Timeline as u32,
                7,
                3,
                "copies".into(),
                0,
            )]
                .1;
            let key_y = (rect.min[1] + rect.max[1]) * 0.5;
            let bottom = rect.min[1] + height;
            assert!(list.shapes.iter().any(|shape| matches!(shape,
                paint::Shape::Procedural {area, kind: paint::Pattern::Box { .. }, ..}
                if area.width() >= width && area.height() <= 1. && (area.max[1]-bottom).abs()<0.01)),
                "each keyframe property must retain a horizontal boundary across the time lanes");
            let track_x = super::super::GUTTER.min(width * 0.52);
            let expected_x = track_x
                + (60. - panels.timeline.offset) * (width - track_x - 8.) / 120.
                    * panels.timeline.zoom;
            assert!(list.shapes.iter().any(|shape| matches!(shape,
                paint::Shape::Procedural {area,kind:paint::Pattern::Diamond,..}
                if ((area.min[0]+area.max[0])*0.5-expected_x).abs()<0.01 && ((area.min[1]+area.max[1])*0.5-key_y).abs()<0.01)), "key must stay centered on its numeric row after scrolling/zooming");
        }
    }
}
