//! Conservative scheduling hints, never a license to alias pixels across frames.
//!
//! Analyze a validated project once per document revision. Unknown temporal
//! inputs keep the full configured window. Recognized constant compositions
//! keep the nearest two lookahead frames in playback, away from clip boundaries.
use super::Direction;
use motion_core::{Camera, Content, Project, Transform};

#[derive(Clone, Debug)]
pub struct StaticIntervals {
    boundaries: Vec<f64>,
}

impl StaticIntervals {
    pub fn from_project(project: &Project) -> Option<Self> {
        if !project.expressions.is_empty() || !constant_camera(&project.camera) {
            return None;
        }
        let mut boundaries = vec![0., f64::from(project.frames)];
        for layer in &project.layers {
            // Include hidden, matte and parent sources conservatively. They may
            // affect visible output without producing their own color pixels.
            if !constant_transform(&layer.transform)
                || !layer.effects.is_empty()
                || layer.masks.iter().any(|mask| mask.animated())
            {
                return None;
            }
            let constant = match &layer.content {
                Content::Solid { .. }
                | Content::Image { .. }
                | Content::Text { .. }
                | Content::Null
                | Content::Adjustment
                | Content::Audio { .. } => true,
                Content::Vector { vector } => !vector.animated(),
                Content::Camera { camera } => constant_camera(camera),
                // Do not infer time independence from missing authored keys:
                // model clips, nested compositions and video own other clocks.
                Content::Model { .. } | Content::Composition { .. } | Content::Video { .. } => {
                    false
                }
            };
            if !constant {
                return None;
            }
            let interval = layer.interval(project.frames);
            for edge in [interval.in_frame, interval.out_frame] {
                if !edge.is_finite() {
                    return None;
                }
                if edge > 0. && edge < f64::from(project.frames) {
                    boundaries.push(edge);
                }
            }
        }
        boundaries.sort_by(f64::total_cmp);
        boundaries.dedup();
        Some(Self { boundaries })
    }

    /// Half-open composition interval with no recognized visual time variation.
    pub fn at(&self, frame: f64) -> Option<[f64; 2]> {
        if !frame.is_finite() || frame < 0. {
            return None;
        }
        let next = self.boundaries.partition_point(|edge| *edge <= frame);
        Some([
            *self.boundaries.get(next.checked_sub(1)?)?,
            *self.boundaries.get(next)?,
        ])
    }

    /// Keep the full window near a clip cut so its first changed image can warm.
    /// Every admitted frame still has its own exact key and native rendering.
    pub fn window(&self, frame: f64, direction: Direction, configured: usize) -> usize {
        if configured <= 2 {
            return configured;
        }
        let Some([start, end]) = self.at(frame) else {
            return configured;
        };
        let farthest = frame
            + match direction {
                Direction::Forward => configured as f64,
                Direction::Backward => -(configured as f64),
            };
        if farthest >= start && farthest < end {
            2
        } else {
            configured
        }
    }
}

fn constant_transform(value: &Transform) -> bool {
    !value.position.is_animated()
        && !value.rotation.is_animated()
        && !value.scale.is_animated()
        && !value.opacity.is_animated()
        && !value.anchor.is_animated()
}

fn constant_camera(value: &Camera) -> bool {
    value.imported.is_none()
        && value.depth_of_field.is_none()
        && !value.position.is_animated()
        && !value.target.is_animated()
        && !value.roll.is_animated()
        && !value.fov.is_animated()
        && !value.radius.is_animated()
        && !value.azimuth.is_animated()
        && !value.elevation.is_animated()
}

#[cfg(test)]
mod tests;
