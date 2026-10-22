//! Boundary alignment is planned from one evaluated scene and committed as one edit.
use crate::session::Result;
use motion_core::{Axis, Command, Content, ExpressionTarget, Project, Property};
use motion_effects::Registry;
use motion_render::Scene;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Left,
    HorizontalCenter,
    Right,
    Top,
    VerticalCenter,
    Bottom,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[default]
    Composition,
    Selection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    HorizontalCenters,
    VerticalCenters,
    HorizontalGaps,
    VerticalGaps,
}
impl Distribution {
    fn axis(self) -> usize {
        usize::from(matches!(self, Self::VerticalCenters | Self::VerticalGaps))
    }
    fn gaps(self) -> bool {
        matches!(self, Self::HorizontalGaps | Self::VerticalGaps)
    }
}
impl Edge {
    fn axis(self) -> usize {
        match self {
            Self::Left | Self::HorizontalCenter | Self::Right => 0,
            _ => 1,
        }
    }
    fn coordinate(self, bounds: [[f32; 2]; 2]) -> f32 {
        let axis = self.axis();
        match self {
            Self::Left | Self::Top => bounds[0][axis],
            Self::Right | Self::Bottom => bounds[1][axis],
            _ => (bounds[0][axis] + bounds[1][axis]) * 0.5,
        }
    }
}

pub fn align(id: i64, objects: Vec<u64>, edge: Edge, target: Target) -> Result<serde_json::Value> {
    execute(id, |p, scene, registry, frame| {
        plan(p, scene, registry, frame, &objects, edge, target)
    })
}

pub fn distribute(
    id: i64,
    objects: Vec<u64>,
    distribution: Distribution,
) -> Result<serde_json::Value> {
    execute(id, |p, scene, registry, frame| {
        distribution_plan(p, scene, registry, frame, &objects, distribution)
    })
}

fn execute(
    id: i64,
    planner: impl FnOnce(&Project, &Scene, &Registry, f64) -> Result<Vec<Command>>,
) -> Result<serde_json::Value> {
    crate::session::with_session(id, |s| {
        // Property edits address integer frames. Evaluate and display that same
        // frame after a fractional playback stop; rejected plans keep the clock.
        let previous_frame = s.frame;
        let previous_scene = if s.frame.fract() != 0. {
            Some(s.scene.clone())
        } else {
            None
        };
        s.frame = s.frame.floor();
        let result = (|| {
            s.sample()?;
            let commands = planner(s.engine.project(), &s.scene, &s.effects.registry, s.frame)?;
            if commands.is_empty() {
                return s.snapshot();
            }
            s.apply_command_batch(commands)
        })();
        if result.is_err() {
            s.frame = previous_frame;
            if let Some(scene) = previous_scene {
                s.scene = scene;
            }
        }
        result
    })
}

fn bounds(scene: &Scene, registry: &Registry, object: u64) -> Result<[[f32; 2]; 2]> {
    let layer = scene
        .layers
        .iter()
        .find(|l| l.id == object)
        .ok_or_else(|| format!("align layer {object}: no active drawable bounds"))?;
    if layer.screen_overlay {
        return Err(format!(
            "align layer {object}: screen overlay is unsupported"
        ).into());
    }
    let polygon = motion_render::selection_geometry::polygon(layer, scene, registry)
        .ok_or_else(|| format!("align layer {object}: projected bounds unavailable"))?;
    let mut result = [[f32::INFINITY; 2], [f32::NEG_INFINITY; 2]];
    for point in polygon {
        for axis in 0..2 {
            if !point[axis].is_finite() {
                return Err(format!("align layer {object}: nonfinite bounds").into());
            }
            result[0][axis] = result[0][axis].min(point[axis]);
            result[1][axis] = result[1][axis].max(point[axis]);
        }
    }
    if result.iter().flatten().any(|v| !v.is_finite()) {
        return Err(format!("align layer {object}: empty bounds").into());
    }
    Ok(result)
}

fn plan(
    project: &Project,
    scene: &Scene,
    registry: &Registry,
    frame: f64,
    objects: &[u64],
    edge: Edge,
    target: Target,
) -> Result<Vec<Command>> {
    let rectangles = selected_bounds(project, scene, registry, frame, objects)?;
    if target == Target::Selection && objects.len() < 2 {
        return Err("align: selection target requires two layers".into());
    }
    let mut union = [[f32::INFINITY; 2], [f32::NEG_INFINITY; 2]];
    for rectangle in rectangles.values() {
        for axis in 0..2 {
            union[0][axis] = union[0][axis].min(rectangle[0][axis]);
            union[1][axis] = union[1][axis].max(rectangle[1][axis]);
        }
    }
    let destination = edge.coordinate(if target == Target::Selection {
        union
    } else {
        [[0.; 2], [project.width as f32, project.height as f32]]
    });
    let deltas = objects
        .iter()
        .map(|&object| {
            let mut delta = [0.; 3];
            delta[edge.axis()] = destination - edge.coordinate(rectangles[&object]);
            (object, delta)
        })
        .collect();
    position_commands(project, frame, objects, &deltas)
}

fn selected_bounds(
    project: &Project,
    scene: &Scene,
    registry: &Registry,
    frame: f64,
    objects: &[u64],
) -> Result<HashMap<u64, [[f32; 2]; 2]>> {
    if objects.is_empty()
        || objects.len() > motion_core::MAX_LAYERS
        || !frame.is_finite()
        || frame < 0.
    {
        return Err("align: invalid selection or frame".into());
    }
    let unique: HashSet<_> = objects.iter().copied().collect();
    if unique.len() != objects.len() {
        return Err("align: duplicate selected layer".into());
    }
    let mut rectangles = HashMap::new();
    for &object in objects {
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or_else(|| format!("align layer {object}: layer does not exist"))?;
        if layer.locked
            || layer.three_d
            || !layer.visible
            || !layer.active(frame, project.frames)
            || matches!(
                layer.content,
                Content::Null
                    | Content::Camera { .. }
                    | Content::Audio { .. }
                    | Content::Adjustment
            )
        {
            return Err(format!(
                "align layer {object}: requires an unlocked, visible, active 2D drawable layer"
            ).into());
        }
        if project.expressions.iter().any(|e|e.enabled && matches!(e.target,ExpressionTarget::Property{object:id,property:Property::Position,..} if id==object)) {
            return Err(format!("align layer {object}: position is controlled by an expression").into());
        }
        let rectangle = bounds(scene, registry, object)?;
        rectangles.insert(object, rectangle);
    }
    Ok(rectangles)
}

fn distribution_plan(
    project: &Project,
    scene: &Scene,
    registry: &Registry,
    frame: f64,
    objects: &[u64],
    distribution: Distribution,
) -> Result<Vec<Command>> {
    if objects.len() < 3 {
        return Err("distribute: at least three layers are required".into());
    }
    let rectangles = selected_bounds(project, scene, registry, frame, objects)
        .map_err(|e| crate::Error::with_context("distribute", e))?;
    let axis = distribution.axis();
    let coordinate = |id: u64| {
        let r = rectangles[&id];
        if distribution.gaps() {
            f64::from(r[0][axis])
        } else {
            (f64::from(r[0][axis]) + f64::from(r[1][axis])) * 0.5
        }
    };
    let width = |id: u64| f64::from(rectangles[&id][1][axis]) - f64::from(rectangles[&id][0][axis]);
    let mut ordered = objects.to_vec();
    ordered.sort_by(|a, b| coordinate(*a).total_cmp(&coordinate(*b)).then(a.cmp(b)));
    let first = ordered[0];
    let last = ordered[ordered.len() - 1];
    let intervals = (ordered.len() - 1) as f64;
    let step = if distribution.gaps() {
        (f64::from(rectangles[&last][0][axis])
            - f64::from(rectangles[&first][1][axis])
            - ordered[1..ordered.len() - 1]
                .iter()
                .map(|&id| width(id))
                .sum::<f64>())
            / intervals
    } else {
        (coordinate(last) - coordinate(first)) / intervals
    };
    let mut cursor = if distribution.gaps() {
        coordinate(first) + width(first) + step
    } else {
        coordinate(first) + step
    };
    let mut deltas = HashMap::new();
    deltas.insert(first, [0.; 3]);
    deltas.insert(last, [0.; 3]);
    let mut previous = (first, coordinate(first));
    for &id in &ordered[1..ordered.len() - 1] {
        if distribution.gaps()
            && cursor
                .total_cmp(&previous.1)
                .then(id.cmp(&previous.0))
                .is_lt()
        {
            return Err(format!("distribute: fixed endpoints require reversed edge order between layers {} and {id}",previous.0).into());
        }
        let mut delta = [0.; 3];
        delta[axis] = (cursor - coordinate(id)) as f32;
        // Projected bounds are f32. Avoid writing another key/history entry
        // when re-evaluation differs only by projection rounding.
        let magnitude = rectangles[&id]
            .iter()
            .map(|p| p[axis].abs())
            .fold(1., f32::max);
        if delta[axis].abs() <= magnitude * f32::EPSILON * 4. {
            delta[axis] = 0.;
        }
        deltas.insert(id, delta);
        previous = (id, cursor);
        cursor += step + if distribution.gaps() { width(id) } else { 0. };
    }
    if distribution.gaps()
        && coordinate(last)
            .total_cmp(&previous.1)
            .then(last.cmp(&previous.0))
            .is_lt()
    {
        return Err(format!(
            "distribute: fixed endpoints require reversed edge order between layers {} and {last}",
            previous.0
        ).into());
    }
    position_commands(project, frame, objects, &deltas)
        .map_err(|cause| crate::Error::with_context("distribute", cause))
}

fn position_commands(
    project: &Project,
    frame: f64,
    objects: &[u64],
    deltas: &HashMap<u64, [f32; 3]>,
) -> Result<Vec<Command>> {
    let mut commands = vec![];
    for &object in objects {
        let layer = project
            .layers
            .iter()
            .find(|l| l.id == object)
            .ok_or("align: selected layer disappeared")?;
        let mut delta = deltas[&object];
        // A selected child inherits its selected ancestor's planned displacement.
        // Remove that displacement before converting through the unchanged parent prefix.
        let mut parent = layer.parent.as_ref().and_then(|l| l.object);
        let mut visited = HashSet::new();
        while let Some(id) = parent {
            if !visited.insert(id) {
                return Err(format!("align layer {object}: parent cycle").into());
            }
            if let Some(inherited) = deltas.get(&id) {
                for axis in 0..3 {
                    delta[axis] -= inherited[axis];
                }
                break;
            }
            parent = if id == 0 {
                project.camera.parent.as_ref().and_then(|l| l.object)
            } else {
                project
                    .layers
                    .iter()
                    .find(|l| l.id == id)
                    .and_then(|l| l.parent.as_ref())
                    .and_then(|l| l.object)
            };
        }
        if delta.iter().all(|v| v.abs() < 1e-5) {
            continue;
        }
        let local = motion_core::scene_prefix_delta(project, object, frame, delta)
            .map_err(|e| crate::Error::with_context(format!("align layer {object}"), e))?;
        let before = layer.transform.position.sample(layer.local_frame(frame))?;
        let value = std::array::from_fn(|axis| before[axis] + local[axis]);
        if value
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 10_000_000.)
        {
            return Err(format!("align layer {object}: position is out of range").into());
        }
        if layer.transform.position.axes.is_some() {
            for (axis, component) in [Axis::X, Axis::Y].into_iter().enumerate() {
                if local[axis].abs() > 1e-5 {
                    commands.push(Command::SetComponent {
                        object,
                        property: Property::Position,
                        axis: component,
                        frame: frame.floor() as u32,
                        value: value[axis],
                    });
                }
            }
        } else {
            commands.push(Command::SetVector {
                object,
                property: Property::Position,
                frame: frame.floor() as u32,
                value,
            });
        }
    }
    Ok(commands)
}

#[cfg(test)]
mod distribution_tests;
#[cfg(test)]
mod tests;
