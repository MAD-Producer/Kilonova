//! Shared composition-space targets; all tolerances remain logical screen pixels.
use super::*;

#[derive(Clone)]
pub(in crate::panels) struct Snapping {
    mapping: Mapping,
    canvas: [f64; 2],
    guides: Arc<Vec<Guide>>,
    use_guides: bool,
    grid: Option<f64>,
    tolerance: f64,
}
impl Controls {
    pub(in crate::panels) fn snapping(
        &mut self,
        state: &Value,
        mapping: Mapping,
        canvas: [f64; 2],
        styles: &motion_ui::style::StyleSheet,
    ) -> Option<Snapping> {
        if state["observing"] == true || self.editing() {
            return None;
        }
        let guides = self.snapshot(state)?;
        let grid = if self.visibility.grid {
            grid_step(
                mapping,
                styles.metric("viewer.overlay.grid.spacing", 100.) as f64,
                styles.metric("viewer.overlay.grid.min_spacing", 24.) as f64,
            )
        } else {
            None
        };
        if grid.is_none() && (!self.visibility.guides || guides.is_empty()) {
            return None;
        }
        Some(Snapping {
            mapping,
            canvas,
            guides,
            use_guides: self.visibility.guides,
            grid,
            tolerance: styles.metric("viewer.overlay.snap.distance", 6.) as f64,
        })
    }
}
impl Snapping {
    pub(in crate::panels) fn tolerance(&self) -> f32 {
        self.tolerance as f32
    }
    pub(in crate::panels) fn targets(&self, point: [f32; 2]) -> [Option<f32>; 2] {
        let local = self.mapping.local(point);
        std::array::from_fn(|index| {
            let axis = if index == 0 {
                GuideAxis::X
            } else {
                GuideAxis::Y
            };
            let position = nearest_axis(
                local[index],
                axis,
                if self.use_guides { &self.guides } else { &[] },
                self.canvas,
                self.mapping,
                self.tolerance,
                self.grid,
                None,
            )?;
            let mut target = local;
            target[index] = position;
            Some(self.mapping.screen(target)[index])
        })
    }
    pub(in crate::panels) fn point(&self, point: [f32; 2]) -> [f32; 2] {
        let targets = self.targets(point);
        std::array::from_fn(|i| targets[i].unwrap_or(point[i]))
    }
}
