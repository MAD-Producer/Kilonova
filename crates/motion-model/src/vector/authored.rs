//! Changes to authored topology never sample or rebuild animated geometry.
use super::{
    groups::{GroupItem, VectorGroup},
    VectorContent, VectorPath, VectorSource,
};
use crate::{ensure, Error, Result};

fn group_geometry(group: &mut VectorGroup, item: u64) -> Option<&mut VectorContent> {
    for entry in &mut group.items {
        match entry {
            GroupItem::Geometry { id, vector, .. } if *id == item => return Some(vector),
            GroupItem::Group { group } => {
                if let Some(vector) = group_geometry(group, item) {
                    return Some(vector);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn first_vertex(
    vector: &mut VectorContent,
    item: Option<u64>,
    path: u64,
    node: u64,
) -> Result<()> {
    let vector = if let Some(item) = item {
        ensure(item != 0, "first vertex requires a nonzero geometry item")?;
        let VectorSource::Group { group } = &mut vector.source else {
            return Err(Error::Invalid(
                "first vertex geometry item requires a vector group".into(),
            ));
        };
        group_geometry(group, item).ok_or_else(|| {
            Error::Invalid(format!("first vertex geometry item missing: item={item}"))
        })?
    } else {
        vector
    };
    let VectorSource::Paths { paths } = &mut vector.source else {
        return Err(Error::Invalid(
            "first vertex requires an authored path; convert parametric geometry first".into(),
        ));
    };
    let path = paths
        .iter_mut()
        .find(|p| p.id == path)
        .ok_or_else(|| Error::Invalid(format!("first vertex path missing: path={path}")))?;
    path.set_first_vertex(node)
}

pub(super) fn set_tangents(
    vector: &mut VectorContent, item: Option<u64>, path: u64, mode: super::TangentMode,
) -> Result<()> {
    let vector = if let Some(item) = item {
        ensure(item != 0, "path tangents require a nonzero geometry item")?;
        let VectorSource::Group { group } = &mut vector.source else {
            return Err(Error::Invalid("path tangents geometry item requires a vector group".into()));
        };
        group_geometry(group, item).ok_or_else(|| Error::Invalid(format!("path tangents geometry item missing: item={item}")))?
    } else { vector };
    let VectorSource::Paths { paths } = &mut vector.source else {
        return Err(Error::Invalid("path tangents require an authored path; convert parametric geometry first".into()));
    };
    let target = paths.iter_mut().find(|p| p.id == path)
        .ok_or_else(|| Error::Invalid(format!("path tangents path missing: path={path}")))?;
    target.tangents = mode;
    Ok(())
}

impl VectorPath {
    /// Change only the seam, across all authored animation keys at once.
    pub fn set_first_vertex(&mut self, node: u64) -> Result<()> {
        ensure(
            self.closed && self.nodes.len() >= 3,
            "first vertex requires a closed path with at least three nodes",
        )?;
        let index = self
            .nodes
            .iter()
            .position(|n| n.id == node)
            .ok_or_else(|| Error::Invalid(format!("first vertex node missing: node={node}")))?;
        self.nodes.rotate_left(index);
        Ok(())
    }
}
