//! Grips address authored geometry, never trim/repeater output ordinals.
use super::*;
use glam::{Mat3, Vec2};
use motion_core::vector::groups::{GroupItem, ParameterValue, VectorGroup};

fn leaf(
    vector: &VectorContent,
    size: [f32; 2],
    position: [f32; 2],
    basis: Mat3,
    item: Option<u64>,
    frame: f64,
    out: &mut Vec<Value>,
) {
    let matrix = (basis * Mat3::from_translation(Vec2::from_array(position))).to_cols_array();
    match &vector.source {
        VectorSource::Paths { paths } => {
            for path in paths {
                let nodes: Vec<_> = path
                    .nodes
                    .iter()
                    .map(|n| {
                        serde_json::json!({
                "id":n.id,"geometry":n.geometry.sample(frame),"animated":n.geometry.is_animated()})
                    })
                    .collect();
                out.push(serde_json::json!({"id":path.id,"closed":path.closed,"nodes":nodes,"item":item,"basis":matrix}));
            }
        }
        VectorSource::Shape { .. } => {
            let nodes:Vec<_>=[[-1.,-1.],[1.,-1.],[1.,1.],[-1.,1.]].into_iter().enumerate()
                .map(|(i,p)|serde_json::json!({"id":i+1,"geometry":[p[0]*size[0]*0.5,p[1]*size[1]*0.5,0.,0.,0.,0.]})).collect();
            out.push(
                serde_json::json!({"id":1,"closed":true,"nodes":nodes,"item":item,"basis":matrix,
                "bounds_size":size,"bounds_position":position}),
            );
        }
        VectorSource::Group { .. } => unreachable!("validated geometry cannot contain a group"),
    }
}
fn visit(
    group: &VectorGroup,
    frame: f64,
    parent: Mat3,
    out: &mut Vec<Value>,
) -> motion_core::Result<()> {
    let basis = parent * group.transform.sample_matrix(frame)?;
    for item in &group.items {
        match item {
            GroupItem::Group { group: g } => visit(g, frame, basis, out)?,
            GroupItem::Geometry {
                id,
                vector,
                size,
                position,
                ..
            } => leaf(
                vector,
                size.sample(frame),
                position.sample(frame),
                basis,
                Some(*id),
                frame,
                out,
            ),
            _ => {} // Paints and generated copies do not own editable nodes.
        }
    }
    Ok(())
}
pub(super) fn paths(layer: &Layer, frame: f64) -> motion_core::Result<Vec<Value>> {
    let mut out = vec![];
    if let Content::Vector { vector } = &layer.content {
        let local = layer.local_frame(frame);
        if let VectorSource::Group { group: g } = &vector.source {
            visit(g, local, Mat3::IDENTITY, &mut out)?;
        } else {
            leaf(
                vector,
                layer.size,
                [0.; 2],
                Mat3::IDENTITY,
                None,
                local,
                &mut out,
            );
        }
    }
    Ok(out)
}
pub(super) fn projection(base: &Projection, path: &Value) -> Option<Projection> {
    let Some(a) = path["basis"].as_array() else {
        return Some(base.clone());
    };
    let mut m = [0.; 9];
    if a.len() != 9 {
        return None;
    }
    for (x, v) in m.iter_mut().zip(a) {
        *x = v.as_f64()?;
    }
    // Projection uses upward Y; vector geometry uses downward Y.
    let local = DMat4::from_cols_array(&[
        m[0], -m[1], 0., 0., -m[3], m[4], 0., 0., 0., 0., 1., 0., m[6], -m[7], 0., 1.,
    ]);
    let matrix = base.matrix * local;
    if !matrix.is_finite() || matrix.determinant().abs() < 1e-15 {
        return None;
    }
    Some(Projection {
        matrix,
        inverse: matrix.inverse(),
        image: base.image,
    })
}

#[derive(Clone)]
pub(super) struct Bounds {
    pub size: [f32; 2],
    pub position: [f32; 2],
    pub corner: [f32; 2],
}
impl Bounds {
    pub fn from_path(path: &Value, node: u64) -> Option<Self> {
        let size = serde_json::from_value(path["bounds_size"].clone()).ok()?;
        let position = serde_json::from_value(path["bounds_position"].clone()).ok()?;
        let corner = match node {
            1 => [-1., -1.],
            2 => [1., -1.],
            3 => [1., 1.],
            4 => [-1., 1.],
            _ => return None,
        };
        Some(Self {
            size,
            position,
            corner,
        })
    }
    pub fn edit(
        &self,
        object: u64,
        item: Option<u64>,
        frame: u32,
        delta: [f32; 2],
    ) -> Option<Vec<Action>> {
        let size: [f32; 2] =
            std::array::from_fn(|i| (self.size[i] + self.corner[i] * delta[i]).clamp(0., 32768.));
        let position: [f32; 2] = std::array::from_fn(|i| {
            self.position[i] + self.corner[i] * (size[i] - self.size[i]) * 0.5
        });
        if position.iter().any(|v| !v.is_finite() || v.abs() > 32768.) {
            return None;
        }
        Some(vec![Action::EditBatch(
            [("size", size), ("position", position)]
                .into_iter()
                .map(|(parameter, value)| Command::Vector {
                    object,
                    action: VectorAction::SetGroupParameter {
                        item: item.unwrap_or(2),
                        parameter: parameter.into(),
                        frame,
                        value: ParameterValue::Vector(value),
                        animated: None,
                    },
                })
                .collect(),
        )])
    }
}
