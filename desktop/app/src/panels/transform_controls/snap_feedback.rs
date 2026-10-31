//! A transient reached-target overlay; never serialized or added to history.
use super::*;

impl Drag {
    pub(super) fn paint_snap_feedback(&self, list: &mut PaintList, clip: Rect) {
        let Some((point, targets)) = self.snap_feedback.filter(|_| self.begun) else {
            return;
        };
        let styles = list.styles();
        let color = styles.color("viewer.guide");
        let width = styles.metric("viewer.overlay.line.width", 1.);
        let radius = styles.metric("viewer.overlay.snap.marker.radius", 3.);
        for (axis, target) in targets.into_iter().enumerate() {
            let Some(target) = target else {
                continue;
            };
            if !target.is_finite() || target < clip.min[axis] || target > clip.max[axis] {
                continue;
            }
            let mut start = clip.min;
            let mut end = clip.max;
            start[axis] = target;
            end[axis] = target;
            paint::line_with_width(list, vec![start, end], color, width);
        }
        if clip.contains(point) {
            paint::diamond(list, point, radius, color);
        }
    }
}

#[cfg(test)]
mod tests;
