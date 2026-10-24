use super::*;
use motion_core::{PathDestination, PathSelection, PathTarget};
use std::collections::BTreeMap;

impl Controls {
    pub(in crate::panels) fn path_clipboard_selection(
        &self,
        state: &Value,
        object: Option<u64>,
    ) -> Option<super::super::path_clipboard::Selection> {
        if self.editing() || self.tool == Tool::Select {
            return None;
        }
        let object = object.filter(|id| *id != 0)?;
        let layer = state["project"]["layers"]
            .as_array()?
            .iter()
            .find(|l| l["id"] == object)?;
        let mask = self.tool == Tool::Mask;
        let target = if mask {
            PathDestination::NewMasks { object }
        } else {
            PathDestination::Vector {
                object,
                item: self.selected_item,
            }
        };
        let mut paths: BTreeMap<(Option<u64>, Option<u64>, u64), Vec<u64>> = BTreeMap::new();
        if self.point_selection.current(state, object, mask) {
            for point in &self.point_selection.points {
                paths
                    .entry((point.mask, point.item, point.path))
                    .or_default()
                    .push(point.node);
            }
        }
        let selections = paths
            .into_iter()
            .map(|((mask, item, path), nodes)| PathSelection {
                target: if let Some(mask) = mask {
                    PathTarget::Mask { object, mask }
                } else {
                    PathTarget::Vector { object, item }
                },
                path,
                nodes: Some(nodes),
            })
            .collect();
        Some(super::super::path_clipboard::Selection::new(
            state,
            selections,
            target,
            layer["locked"] != true,
        ))
    }
    pub(crate) fn clipboard_cut_completed(&mut self) {
        self.point_selection.clear();
        self.selected = None;
    }
}
