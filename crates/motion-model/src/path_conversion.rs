//! Copy authored geometry without changing the source or baking animation.
//! Paint, mask compositing, effects and generated outlines are not geometry.
use crate::{
    ensure,
    masks::{LayerMask, MAX_MASKS},
    vector::{
        groups::{GroupItem, VectorGroup},
        FillRule, VectorContent, VectorPath, VectorSource,
    },
    Content, Error, ExpressionTarget, Layer, PathTarget, Project, Result, Track, MAX_LAYERS,
};
use glam::{Mat3, Vec2};

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

fn basis(group: &VectorGroup, item: u64, parent: Mat3) -> Result<Option<Mat3>> {
    // Reject time-varying bases: sampling would discard continuous animation.
    let t = &group.transform;
    let animated = t.position.is_animated()
        || t.anchor.is_animated()
        || t.scale.is_animated()
        || t.rotation.is_animated()
        || t.skew.is_animated()
        || t.skew_axis.is_animated();
    for entry in &group.items {
        let found = match entry {
            GroupItem::Geometry { id, position, .. } if *id == item => {
                ensure(
                    !position.is_animated(),
                    "cannot copy animated geometry position losslessly",
                )?;
                Some(Mat3::from_translation(Vec2::from_array(
                    position.sample(f64::NAN)?,
                )))
            }
            GroupItem::Group { group } => basis(group, item, Mat3::IDENTITY)?,
            _ => None,
        };
        if let Some(found) = found {
            ensure(
                !animated,
                "cannot copy a path through an animated group transform losslessly",
            )?;
            return Ok(Some(parent * t.sample_matrix(f64::NAN)? * found));
        }
    }
    Ok(None)
}

fn transform(path: &mut VectorPath, matrix: Mat3) -> Result<()> {
    ensure(matrix.is_finite(), "path conversion basis is not finite")?;
    for node in &mut path.nodes {
        ensure(node.geometry.axes.is_none() && node.geometry.keys.iter().all(|k| k.spatial.is_none()),
            "path conversion requires authored geometry tracks without spatial or separated dimensions")?;
        let map = |v: &mut [f32; 6]| {
            let p = matrix.transform_point2(Vec2::new(v[0], v[1]));
            let a = matrix.transform_vector2(Vec2::new(v[2], v[3]));
            let b = matrix.transform_vector2(Vec2::new(v[4], v[5]));
            *v = [p.x, p.y, a.x, a.y, b.x, b.y];
        };
        map(&mut node.geometry.value);
        for key in &mut node.geometry.keys {
            map(&mut key.value);
        }
    }
    Ok(())
}

/// Returns the receiving layer ID. The caller owns the project transaction.
/// Both directions preserve closedness, geometry keys and layer-local time.
pub fn copy_as(project: &mut Project, target: PathTarget, path_id: u64) -> Result<u64> {
    let object = target.object();
    let result = (|| {
        let index = project
            .layers
            .iter()
            .position(|l| l.id == object)
            .ok_or_else(|| invalid(format!("path source layer missing: object={object}")))?;
        let source = &project.layers[index];
        ensure(!source.locked, "path source layer is locked")?;
        let mut path = target.read(source, path_id)?.clone();
        match target {
            PathTarget::Vector { item, .. } => {
                ensure(source.masks.len() < MAX_MASKS, "mask limit exceeded")?;
                let Content::Vector { vector } = &source.content else {
                    return Err(invalid("path source is not a vector layer"));
                };
                let local = match item {
                    None => Mat3::IDENTITY,
                    Some(item) => {
                        let VectorSource::Group { group } = &vector.source else {
                            return Err(invalid("geometry item requires a vector group"));
                        };
                        basis(group, item, Mat3::IDENTITY)?
                            .ok_or_else(|| invalid(format!("geometry item missing: item={item}")))?
                    }
                };
                transform(
                    &mut path,
                    Mat3::from_translation(Vec2::from_array(source.size) * 0.5) * local,
                )?;
                // Mask IDs are local to this layer; path/node IDs are local to their container.
                let id = source
                    .masks
                    .iter()
                    .map(|m| m.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| invalid("mask ID space exhausted"))?;
                path.id = id;
                let mask = LayerMask::new(id, path);
                mask.validate()?;
                project.layers[index].masks.push(mask);
                Ok(object)
            }
            PathTarget::Mask { .. } => {
                ensure(project.layers.len() < MAX_LAYERS, "layer limit exceeded")?;
                ensure(!project.expressions.iter().any(|e| e.enabled &&
                    matches!(e.target, ExpressionTarget::Property { object: o, .. } if o == object)),
                    "cannot copy a mask to a shape layer with transform expressions losslessly")?;
                transform(
                    &mut path,
                    Mat3::from_translation(-Vec2::from_array(source.size) * 0.5),
                )?;
                let id = project
                    .layers
                    .iter()
                    .map(|l| l.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or_else(|| invalid("layer ID space exhausted"))?;
                path.id = 1;
                let mut layer = Layer::solid(id, "Shape path", source.size, [0.; 3], [1.; 4]);
                layer.content = Content::Vector {
                    vector: VectorContent {
                        source: VectorSource::Paths { paths: vec![path] },
                        fill: Some(Track::constant([1.; 4])),
                        fill_rule: FillRule::NonZero,
                        stroke: None,
                        trim: None,
                    },
                };
                if let Content::Vector { vector } = &layer.content {
                    vector.validate()?;
                }
                layer.transform = source.transform.clone();
                layer.parent = source.parent.clone();
                layer.three_d = source.three_d;
                layer.rotation_order = source.rotation_order;
                layer.sampling = source.sampling.clone();
                layer.timeline = source.timeline.clone();
                layer.subframe_range = source.subframe_range;
                project.layers.insert(index + 1, layer);
                Ok(id)
            }
        }
    })();
    result.map_err(|source| Error::PathOperation {
        operation: "copy_as",
        object,
        path: path_id,
        node: None,
        source: Box::new(source),
    })
}
