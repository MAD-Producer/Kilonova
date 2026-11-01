use crate::{ensure, Error, Project, Result};
use glam::Mat4;

/// Reparenting changes the inherited basis; authored tracks are never baked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParentMode {
    /// Preserve the sampled world transform at the command's composition frame.
    PreserveWorld,
    /// Retain authored local values, inheriting the new parent's full transform.
    PreserveLocal,
}

pub fn validate(p: &Project) -> Result<()> {
    for object in std::iter::once(0).chain(p.layers.iter().map(|l| l.id)) {
        let mut current = object;
        for depth in 0..=p.layers.len() + 1 {
            let parent = if current == 0 {
                p.camera.parent.as_ref()
            } else {
                p.layers
                    .iter()
                    .find(|l| l.id == current)
                    .ok_or(Error::Missing(current))?
                    .parent
                    .as_ref()
            };
            let Some(link) = parent else { break };
            ensure(
                link.bind.iter().flatten().all(|v| v.is_finite()),
                "invalid parent bind matrix",
            )?;
            let matrix = Mat4::from_cols_array_2d(&link.bind);
            ensure(
                matrix.x_axis.w.abs() < 1e-5
                    && matrix.y_axis.w.abs() < 1e-5
                    && matrix.z_axis.w.abs() < 1e-5
                    && (matrix.w_axis.w - 1.0).abs() < 1e-5,
                "parent bind must be affine",
            )?;
            let Some(parent) = link.object else { break };
            ensure(
                !p.layers
                    .iter()
                    .any(|l| l.id == parent && matches!(l.content, crate::Content::Audio { .. })),
                "audio cannot be a spatial parent",
            )?;
            if parent == 0 {
                ensure(p.camera.created, "parent camera does not exist")?;
            } else {
                ensure(
                    p.layers.iter().any(|l| l.id == parent),
                    "parent object does not exist",
                )?;
            }
            ensure(
                parent != object && depth <= p.layers.len(),
                "parent hierarchy contains a cycle",
            )?;
            current = parent;
        }
    }
    Ok(())
}

pub fn matrices(
    p: &Project,
    frame: f64,
    world: &mut Vec<Mat4>,
    states: &mut Vec<u8>,
) -> Result<()> {
    matrices_with_cameras(p, frame, world, states, &crate::AuthoredCameraSampler, &mut Vec::new())
}
/// Resolve source cameras before their spatial children. The existing state
/// machine also detects cycles involving an imported camera's authored parent.
pub fn matrices_with_cameras(
    p: &Project, frame: f64, world: &mut Vec<Mat4>, states: &mut Vec<u8>,
    sampler: &dyn crate::CameraSampler, cameras: &mut Vec<Option<crate::CameraSample>>,
) -> Result<()> {
    world.resize(p.layers.len() + 1, Mat4::IDENTITY);
    states.resize(p.layers.len() + 1, 0);
    states.fill(0);
    cameras.resize(p.layers.len() + 1, None);
    cameras.fill(None);
    for i in 0..world.len() {
        node(p, frame, i, world, states, sampler, Some(cameras))?;
    }
    Ok(())
}
/// Render roots and their ancestry are strict. An unused tracking camera can
/// legitimately be outside its solve range after a cut; retain its exact cause
/// for a later explicit view selection instead of failing an unrelated shot.
/// Successful inactive samples remain available to fork_view without resampling.
pub fn matrices_for_scene(
    p: &Project,
    frame: f64,
    world: &mut Vec<Mat4>,
    states: &mut Vec<u8>,
    sampler: &dyn crate::CameraSampler,
    cameras: &mut Vec<Option<crate::CameraSample>>,
    errors: &mut Vec<Option<std::sync::Arc<Error>>>,
) -> Result<()> {
    world.resize(p.layers.len() + 1, Mat4::IDENTITY);
    states.resize(world.len(), 0);
    states.fill(0);
    cameras.resize(world.len(), None);
    cameras.fill(None);
    errors.resize_with(world.len(), || None);
    errors.fill(None);
    // Preserve the existing strict spatial-object sampling contract, including
    // cameras used as parents of those objects.
    for (i, _) in p.layers.iter().enumerate().filter(|(_, l)| l.camera_data().is_none()) {
        node(p, frame, i, world, states, sampler, Some(cameras))?;
    }
    let active = p.active_camera_layer(frame)
        .and_then(|l| p.layers.iter().position(|v| v.id == l.id))
        .unwrap_or(p.layers.len());
    node(p, frame, active, world, states, sampler, Some(cameras))?;
    for i in 0..world.len() {
        if states[i] == 2 { continue; }
        if let Err(cause) = node(p, frame, i, world, states, sampler, Some(cameras)) {
            if !matches!(&cause, Error::TrackingCamera { .. }) { return Err(cause); }
            errors[i] = Some(std::sync::Arc::new(cause));
            for state in states.iter_mut().filter(|s| **s == 1) { *state = 0; }
        }
    }
    Ok(())
}
/// Sample just one ancestry chain; particle births do not need unrelated nodes
/// or the composition camera unless it is an explicit spatial parent.
pub(crate) fn matrix_for(p: &Project, frame: f64, index: usize, world: &mut Vec<Mat4>, states: &mut Vec<u8>) -> Result<Mat4> {
    world.resize(p.layers.len() + 1, Mat4::IDENTITY);
    states.resize(p.layers.len() + 1, 0);
    states.fill(0);
    node(p, frame, index, world, states, &crate::AuthoredCameraSampler, None)
}
fn node(
    p: &Project,
    frame: f64,
    index: usize,
    world: &mut [Mat4],
    states: &mut [u8],
    sampler: &dyn crate::CameraSampler,
    mut cameras: Option<&mut [Option<crate::CameraSample>]>,
) -> Result<Mat4> {
    if states[index] == 2 {
        return Ok(world[index]);
    }
    ensure(states[index] == 0, "parent hierarchy contains a cycle")?;
    states[index] = 1;
    let link = if index == p.layers.len() {
        p.camera.parent.as_ref()
    } else {
        p.layers[index].parent.as_ref()
    };
    let prefix = if let Some(link) = link {
        let parent = if let Some(object) = link.object {
            let index = if object == 0 {
                p.layers.len()
            } else {
                p.layers
                    .iter()
                    .position(|l| l.id == object)
                    .ok_or(Error::Missing(object))?
            };
            node(p, frame, index, world, states, sampler, cameras.as_deref_mut())?
        } else {
            Mat4::IDENTITY
        };
        parent * Mat4::from_cols_array_2d(&link.bind)
    } else {
        Mat4::IDENTITY
    };
    let camera_layer = p.layers.get(index).filter(|l| l.camera_data().is_some());
    let value = if index == p.layers.len() || camera_layer.is_some() {
        let implicit;
        let (camera, frame) = if let Some(layer) = camera_layer {
            (layer.camera_data().ok_or_else(|| Error::Invalid("camera data missing".into()))?, layer.local_frame(frame))
        } else if p.camera.created {
            (&p.camera, frame)
        } else {
            implicit = crate::Camera::new(p.width, p.height);
            (&implicit, frame)
        };
        if let Some(binding) = &camera.imported {
            let asset = p.model_assets.iter().find(|asset| asset.id == binding.asset)
                .ok_or_else(|| Error::Invalid(format!("imported_camera source={} phase=binding: managed source missing", binding.asset)))?;
            sampler.validate_source(asset).map_err(|cause| Error::Invalid(format!(
                "imported_camera source={} scene={} node={} phase=binding frame={frame}: {cause}",
                binding.asset, binding.scene, binding.node)))?;
        }
        let sampled = if let Some(binding)=&camera.tracking {
            binding.sample(p,camera,camera_layer.map_or(0,|l|l.id),frame,prefix)?
        }else{
            sampler.sample(camera, frame, p.fps, [p.width, p.height], prefix)?
        };
        if let Some(cameras) = cameras { cameras[index] = Some(sampled); }
        sampled.world
    } else {
        let layer = &p.layers[index];
        let prefix = if layer.three_d {
            prefix
        } else {
            crate::geometry::flat_matrix(prefix)
        };
        prefix * crate::geometry::pivot_matrix(layer, frame, p)?
    };
    ensure(value.is_finite(), "invalid parent transform")?;
    world[index] = value;
    states[index] = 2;
    Ok(value)
}

pub fn prefix(p: &Project, object: u64, frame: f64) -> Result<Mat4> {
    prefix_with_cameras(p, object, frame, &crate::AuthoredCameraSampler)
}
/// Resolve only the requested ancestry. Unrelated source cameras must not make
/// an ordinary layer uneditable; a source camera in this ancestry still requires
/// the caller's resolver and propagates its original failure.
pub fn prefix_with_cameras(p: &Project, object: u64, frame: f64,
    sampler: &dyn crate::CameraSampler) -> Result<Mat4> {
    let link = if object == 0 {
        p.camera.parent.as_ref()
    } else {
        p.layers
            .iter()
            .find(|l| l.id == object)
            .ok_or(Error::Missing(object))?
            .parent
            .as_ref()
    };
    Ok(if let Some(link) = link {
        let parent = if let Some(object) = link.object {
            world_matrix_with_cameras(p, object, frame, sampler)?
        } else {
            Mat4::IDENTITY
        };
        parent * Mat4::from_cols_array_2d(&link.bind)
    } else {
        Mat4::IDENTITY
    })
}

/// Sample one spatial object's ancestry without evaluating unrelated nodes.
pub fn world_matrix_with_cameras(p: &Project, object: u64, frame: f64,
    sampler: &dyn crate::CameraSampler) -> Result<Mat4> {
    let index = if object == 0 { p.layers.len() } else {
        p.layers.iter().position(|l| l.id == object).ok_or(Error::Missing(object))?
    };
    let mut world = vec![Mat4::IDENTITY; p.layers.len() + 1];
    let mut states = vec![0; p.layers.len() + 1];
    node(p, frame, index, &mut world, &mut states, sampler, None)
}
