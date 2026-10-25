use crate::{ensure, Error, Project, Result};
use glam::Mat4;
use motion_model::hierarchy::ParentMode;
pub(crate) use motion_model::hierarchy::{matrices, prefix, validate};

/// The local policy clears the old bind without rewriting or sampling tracks.
/// Restore this one link if hierarchy validation fails, including direct calls.
pub fn reparent_with_mode(
    p: &mut Project,
    object: u64,
    parent: Option<u64>,
    frame: u32,
    mode: ParentMode,
) -> Result<()> {
    let result = (|| {
        let index = if object == 0 {
            ensure(p.camera.created, "camera does not exist")?;
            None
        } else {
            let index = p
                .layers
                .iter()
                .position(|layer| layer.id == object)
                .ok_or(Error::Missing(object))?;
            ensure(!p.layers[index].locked, "object is locked")?;
            ensure(
                !matches!(p.layers[index].content, crate::Content::Audio { .. }),
                "audio has no spatial parent",
            )?;
            Some(index)
        };
        let old = match index {
            Some(index) => p.layers[index].parent.clone(),
            None => p.camera.parent.clone(),
        };
        let result = match mode {
            ParentMode::PreserveWorld => reparent(p, object, parent, frame),
            ParentMode::PreserveLocal => {
                let link = parent.map(|object| crate::ParentLink {
                    object: Some(object),
                    bind: Mat4::IDENTITY.to_cols_array_2d(),
                });
                match index {
                    Some(index) => p.layers[index].parent = link,
                    None => p.camera.parent = link,
                }
                validate(p)
            }
        };
        if result.is_err() {
            match index {
                Some(index) => p.layers[index].parent = old,
                None => p.camera.parent = old,
            }
        }
        result
    })();
    result.map_err(|source| Error::ParentOperation {
        object,
        parent,
        frame,
        mode,
        source: Box::new(source),
    })
}

pub fn reparent(p: &mut Project, object: u64, parent: Option<u64>, frame: u32) -> Result<()> {
    if object == 0 {
        ensure(p.camera.created, "camera does not exist")?;
    } else {
        ensure(!p.layer_mut(object)?.locked, "object is locked")?;
    }
    let old = prefix(p, object, frame as f64)?;
    let new_link = if let Some(parent) = parent {
        ensure(parent != object, "cannot parent an object to itself")?;
        let mut world = Vec::new();
        let mut states = Vec::new();
        matrices(p, frame as f64, &mut world, &mut states)?;
        let i = if parent == 0 {
            ensure(p.camera.created, "camera does not exist")?;
            p.layers.len()
        } else {
            p.layers
                .iter()
                .position(|l| l.id == parent)
                .ok_or(Error::Missing(parent))?
        };
        ensure(
            world[i].determinant().abs() > 1e-8,
            "cannot bind to a zero-scale parent",
        )?;
        Some(crate::ParentLink {
            object: Some(parent),
            bind: (world[i].inverse() * old).to_cols_array_2d(),
        })
    } else {
        // Keep an unparented object's world offset without rewriting keyframes.
        if old.abs_diff_eq(Mat4::IDENTITY, 1e-6) {
            None
        } else {
            Some(crate::ParentLink {
                object: None,
                bind: old.to_cols_array_2d(),
            })
        }
    };
    if object == 0 {
        p.camera.parent = new_link
    } else {
        p.layer_mut(object)?.parent = new_link;
    }
    validate(p)
}
