//! Owned per-cell input for the normal Present/CaptureUi worker protocol.
//! Observer scalars belong to the frame, never to the session's global camera.
use super::UiFrame;
use motion_host::session::viewer::View;
use motion_render::{multiview::ViewPlacement, ViewCamera};

pub(crate) enum Camera {
    Composition,
    Camera(u64),
    Observer(motion_core::Observer),
}
pub(crate) struct Cell {
    pub camera: Camera,
    pub placement: ViewPlacement,
}
pub(crate) struct Plan {
    pub cells: Vec<Cell>,
}
impl Plan {
    pub(super) fn borrow(&self) -> Vec<View<'_>> {
        self.cells
            .iter()
            .map(|cell| View {
                camera: match &cell.camera {
                    Camera::Composition => ViewCamera::Composition,
                    Camera::Camera(id) => ViewCamera::Camera(*id),
                    Camera::Observer(observer) => ViewCamera::Observer(observer),
                },
                placement: cell.placement,
            })
            .collect()
    }
}

pub(super) fn validate(ui: &UiFrame) -> Result<(), String> {
    let Some(plan) = &ui.views else {
        return Ok(());
    };
    if !matches!(plan.cells.len(), 1 | 2 | 4) {
        return Err("multiview phase=frame: expected 1, 2, or 4 cells".into());
    }
    let outer = ui
        .composition
        .ok_or("multiview phase=frame: composition viewport missing")?;
    if !outer.iter().all(|v| v.is_finite())
        || outer[0] < 0.
        || outer[1] < 0.
        || outer[2] <= 0.
        || outer[3] <= 0.
    {
        return Err("multiview phase=frame: invalid composition viewport".into());
    }
    if ui.composition_image.is_some() {
        return Err("multiview phase=frame: use each cell's canvas image".into());
    }
    // Activation is gated until inspection is explicitly routed per cell.
    // Reject unsupported settings; silently displaying the wrong channel or
    // another window's comparison would make the frame look successful.
    if ui.inspection.channel != motion_render::ViewChannel::Color
        || ui.inspection.region.is_some()
        || ui.inspection.comparison_split.is_some()
    {
        return Err("multiview phase=frame: per-cell inspection is not connected yet".into());
    }
    for (index, cell) in plan.cells.iter().enumerate() {
        let [x, y, w, h] = cell.placement.viewport;
        if ![x, y, w, h].iter().all(|v| v.is_finite())
            || w <= 0.
            || h <= 0.
            || x < outer[0]
            || y < outer[1]
            || x + w > outer[0] + outer[2]
            || y + h > outer[1] + outer[3]
        {
            return Err(format!(
                "multiview view={index} phase=frame: cell exceeds composition viewport"
            ));
        }
    }
    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
mod native;
