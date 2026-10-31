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
    layer_targets: [Vec<f64>; 2],
    integer_fallback: bool,
}
impl Controls {
    // Retain the guide-only contract for its regression tests. Production
    // transforms now use the complete geometry context below.
    #[cfg(test)]
    pub(in crate::panels) fn snapping(
        &mut self,
        state: &Value,
        mapping: Mapping,
        canvas: [f64; 2],
        styles: &motion_ui::style::StyleSheet,
    ) -> Option<Snapping> {
        self.snapping_context(state, mapping, canvas, styles, false, None)
    }
    pub(in crate::panels) fn transform_snapping(
        &mut self,
        state: &Value,
        mapping: Mapping,
        canvas: [f64; 2],
        selected: Option<u64>,
        styles: &motion_ui::style::StyleSheet,
    ) -> Option<Snapping> {
        self.snapping_context(state, mapping, canvas, styles, true, selected)
    }
    fn snapping_context(
        &mut self,
        state: &Value,
        mapping: Mapping,
        canvas: [f64; 2],
        styles: &motion_ui::style::StyleSheet,
        automatic: bool,
        selected: Option<u64>,
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
        if !automatic && grid.is_none() && (!self.visibility.guides || guides.is_empty()) {
            return None;
        }
        Some(Snapping {
            mapping,
            canvas,
            guides,
            use_guides: self.visibility.guides,
            grid,
            tolerance: styles.metric("viewer.overlay.snap.distance", 6.) as f64,
            layer_targets: if automatic {
                observe!("transform.snap_targets", layer_targets(state, selected),
                    "operation=transform_snapping phase=build selected={selected:?} revision={} frame={}",
                    state["revision"], state["frame"]).ok()?
            } else {
                Default::default()
            },
            integer_fallback: automatic,
        })
    }
}
impl Snapping {
    pub(in crate::panels) fn tolerance(&self) -> f32 {
        self.tolerance as f32
    }
    pub(in crate::panels) fn targets(&self, point: [f32; 2]) -> [Option<f32>; 2] {
        let local = self.mapping.local(point);
        let structural: [Option<f64>; 2] = std::array::from_fn(|index| {
            let axis = if index == 0 {
                GuideAxis::X
            } else {
                GuideAxis::Y
            };
            let guide = nearest_axis(
                local[index],
                axis,
                if self.use_guides { &self.guides } else { &[] },
                self.canvas,
                self.mapping,
                self.tolerance,
                self.grid,
                None,
            );
            motion_ui::snap::nearest(
                guide
                    .into_iter()
                    .chain(self.layer_targets[index].iter().copied()),
                local[index],
                self.mapping.scale,
                self.tolerance,
            )
            .map(|snap| snap.value)
        });
        // A fallback on the other screen axis must not steal an explicit
        // reference from a proportion-constrained corner scale either.
        let has_reference = structural.iter().any(Option::is_some);
        std::array::from_fn(|index| {
            let position = structural[index].or_else(|| {
                (self.integer_fallback && !has_reference)
                    .then(|| {
                        motion_ui::snap::nearest(
                            [local[index].round()],
                            local[index],
                            self.mapping.scale,
                            self.tolerance,
                        )
                    })
                    .flatten()
                    .map(|snap| snap.value)
            })?;
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

fn layer_targets(state: &Value, selected: Option<u64>) -> Result<[Vec<f64>; 2], String> {
    let mut targets = [vec![], vec![]];
    let layers = state["projectedLayers"]
        .as_array()
        .ok_or("projected layer list is missing or malformed")?;
    if layers.len() > motion_core::MAX_LAYERS {
        return Err("projected layer list exceeds model limit".into());
    }
    for layer in layers {
        let id = layer["id"]
            .as_u64()
            .ok_or("projected layer ID is malformed")?;
        if Some(id) == selected {
            continue;
        }
        // The renderer bounds its selection hull to 64 points before using a
        // conservative rectangle. Do not silently truncate malformed records.
        let corners = layer["corners"]
            .as_array()
            .filter(|corners| corners.len() <= 64)
            .ok_or_else(|| {
                format!("layer={id} projected polygon is malformed or exceeds 64 points")
            })?;
        if corners.iter().any(|corner| {
            corner.as_array().is_none_or(|point| {
                point.len() != 2
                    || point
                        .iter()
                        .any(|v| v.as_f64().is_none_or(|v| !v.is_finite()))
            })
        }) {
            return Err(format!(
                "layer={id} projected point is not a finite coordinate pair"
            ));
        }
        for i in 0..2 {
            let mut bounds: Option<(f64, f64)> = None;
            for corner in corners {
                let Some(value) = corner[i].as_f64().filter(|v| v.is_finite()) else {
                    bounds = None;
                    break;
                };
                bounds = Some(bounds.map_or((value, value), |(min, max)| {
                    (min.min(value), max.max(value))
                }));
            }
            if let Some((min, max)) = bounds {
                targets[i].extend([min, (min + max) * 0.5, max]);
            }
        }
    }
    Ok(targets)
}

#[cfg(test)]
mod tests;
