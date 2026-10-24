use super::*;
use crate::{
    masks::{LayerMask, MAX_MASKS},
    vector::groups::{GroupItem, VectorGroup},
};
use std::collections::BTreeMap;

macro_rules! access {
    ($vector:ident, $group:ident, $iter:ident, $($mutable:tt)?) => {
        fn $group(group: &$($mutable)? VectorGroup, id: u64) -> Option<&$($mutable)? VectorContent> {
            for entry in group.items.$iter() {
                match entry {
                    GroupItem::Geometry { id: item, vector, .. } if *item == id => return Some(vector),
                    GroupItem::Group { group } => if let Some(vector) = $group(group, id) { return Some(vector); },
                    _ => {}
                }
            }
            None
        }
        pub(super) fn $vector(layer: &$($mutable)? Layer, item: Option<u64>) -> Result<&$($mutable)? VectorContent> {
            let Content::Vector { vector } = &$($mutable)? layer.content else {
                return Err(Error::Invalid("path clipboard destination is not a vector layer".into()));
            };
            if let Some(id) = item {
                let VectorSource::Group { group } = &$($mutable)? vector.source else {
                    return Err(Error::Invalid("path clipboard geometry item requires a vector group".into()));
                };
                $group(group, id).ok_or_else(|| Error::Invalid(format!("path clipboard geometry item missing: item={id}")))
            } else { Ok(vector) }
        }
    }
}
access!(vector, group_vector, iter,);
access!(vector_mut, group_vector_mut, iter_mut, mut);

fn next(id: &mut u64) -> Result<u64> {
    *id = id
        .checked_add(1)
        .ok_or_else(|| Error::Invalid("path clipboard ID overflow".into()))?;
    Ok(*id)
}
fn path_ids<'a>(paths: impl Iterator<Item = &'a VectorPath>) -> (u64, u64) {
    paths.fold((0, 0), |(path, node), p| {
        (
            path.max(p.id),
            node.max(p.nodes.iter().map(|n| n.id).max().unwrap_or(0)),
        )
    })
}
fn authored<'a>(vector: &'a mut VectorContent) -> Result<&'a mut Vec<VectorPath>> {
    match &mut vector.source {
        VectorSource::Paths { paths } => Ok(paths),
        _ => Err(Error::Invalid(
            "select authored geometry before pasting paths".into(),
        )),
    }
}
pub(super) fn paste(
    clipboard: &PathClipboard,
    project: &mut Project,
    target: &PathDestination,
    at_frame: Option<u32>,
) -> Result<()> {
    ensure(
        at_frame.is_none_or(|frame| frame < project.frames),
        "path paste frame outside composition",
    )?;
    let object = target.object();
    let original = layer(project, object)?;
    if original.locked {
        return Err(Error::Locked(object));
    }
    let mut updated = original.clone();
    let offset = original.clip(project.frames).offset_frame;
    let (source_path, source_node) = path_ids(clipboard.entries.iter().map(|e| &e.path));
    let (mut last_path, mut last_node) = match target {
        PathDestination::Vector { item, .. } => {
            let vector = vector(&updated, *item)?;
            let VectorSource::Paths { paths } = &vector.source else {
                return Err(Error::Invalid(
                    "select authored geometry before pasting paths".into(),
                ));
            };
            path_ids(paths.iter())
        }
        _ => path_ids(updated.masks.iter().map(|m| &m.path)),
    };
    last_path = last_path.max(source_path);
    last_node = last_node.max(source_node);
    let mut paths = Vec::new();
    for entry in &clipboard.entries {
        let delta = at_frame.map_or(0, |frame| i64::from(frame) - i64::from(clipboard.frame))
            + i64::from(entry.offset)
            - i64::from(offset);
        for mut path in fragments(entry)? {
            path.id = next(&mut last_path)?;
            for node in &mut path.nodes {
                node.id = next(&mut last_node)?;
                for key in &mut node.geometry.keys {
                    key.frame = i32::try_from(i64::from(key.frame) + delta).map_err(|_| {
                        Error::Invalid("path clipboard local key time overflow".into())
                    })?;
                }
            }
            paths.push(path);
        }
    }
    match target {
        PathDestination::Vector { item, .. } => {
            let vector = vector_mut(&mut updated, *item)?;
            authored(vector)?.extend(paths);
            vector.validate()?;
        }
        PathDestination::Mask { mask, .. } => {
            ensure(
                paths.len() == 1,
                "one mask requires exactly one connected path; create separate masks for fragments",
            )?;
            let mask = updated
                .masks
                .iter_mut()
                .find(|m| m.id == *mask)
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "path clipboard destination mask missing: mask={mask}"
                    ))
                })?;
            mask.path = paths.remove(0);
            mask.validate()?;
        }
        PathDestination::NewMasks { .. } => {
            ensure(
                updated.masks.len() + paths.len() <= MAX_MASKS,
                "path clipboard exceeds mask limit",
            )?;
            let mut last_mask = updated.masks.iter().map(|m| m.id).max().unwrap_or(0);
            for path in paths {
                let mask = LayerMask::new(next(&mut last_mask)?, path);
                mask.validate()?;
                updated.masks.push(mask);
            }
        }
    }
    let destination = project
        .layers
        .iter_mut()
        .find(|l| l.id == object)
        .ok_or(Error::Missing(object))?;
    *destination = updated;
    Ok(())
}
pub(super) fn remove(clipboard: &PathClipboard, project: &mut Project) -> Result<()> {
    let mut updated = BTreeMap::new();
    // Validate every source before deleting anything, even across layers.
    for entry in &clipboard.entries {
        let object = entry.target.object();
        (|| -> Result<()> {
            let source = layer(project, object)?;
            if source.locked {
                return Err(Error::Locked(object));
            }
            ensure(
                source.clip(project.frames).offset_frame == entry.offset
                    && entry.target.read(source, entry.path.id)? == &entry.path,
                "path clipboard cut source has changed",
            )?;
            updated.entry(object).or_insert_with(|| source.clone());
            Ok(())
        })()
        .map_err(|error| failure("cut", object, entry.path.id, error))?;
    }
    for entry in &clipboard.entries {
        (|| -> Result<()> {
            let object = entry.target.object();
            let layer = updated.get_mut(&object).ok_or(Error::Missing(object))?;
            let ids = selected(entry)?;
            let retain = |path: &mut VectorPath| {
                path.nodes.retain(|node| !ids.contains(&node.id));
                path.closed &= path.nodes.len() >= 3;
            };
            match &entry.target {
                PathTarget::Mask { mask, .. } => {
                    let mask = layer
                        .masks
                        .iter_mut()
                        .find(|m| m.id == *mask)
                        .ok_or_else(|| {
                            Error::Invalid(format!("path clipboard cut mask missing: mask={mask}"))
                        })?;
                    retain(&mut mask.path);
                    mask.validate()?;
                }
                PathTarget::Vector { item, .. } => {
                    let vector = vector_mut(layer, *item)?;
                    let paths = authored(vector)?;
                    if entry.nodes.is_none() {
                        paths.retain(|path| path.id != entry.path.id);
                    } else {
                        let path = paths
                            .iter_mut()
                            .find(|p| p.id == entry.path.id)
                            .ok_or_else(|| {
                                Error::Invalid("path clipboard cut path missing".into())
                            })?;
                        retain(path);
                    }
                    vector.validate()?;
                }
            }
            Ok(())
        })()
        .map_err(|error| failure("cut", entry.target.object(), entry.path.id, error))?;
    }
    for destination in &mut project.layers {
        if let Some(layer) = updated.remove(&destination.id) {
            *destination = layer;
        }
    }
    Ok(())
}
