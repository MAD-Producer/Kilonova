//! A path key is a pose of all vertices, stored in the existing node tracks.
use super::*;

pub(super) const CHANNEL: u8 = 3;
static REJECTED_EDITS: motion_render::observability::FailureCounter = motion_render::observability::FailureCounter::new();
pub(super) const CHANNELS: [u8; 4] = [CHANNEL, 0, 1, 2];

pub(super) fn frames(mask: &LayerMask) -> Vec<i32> {
    let mut frames: Vec<_> = mask
        .path
        .nodes
        .iter()
        .flat_map(|n| n.geometry.keys.iter().map(|k| k.frame))
        .collect();
    frames.sort_unstable();
    frames.dedup();
    frames
}
pub(super) fn values(mask: &LayerMask, frame: f64) -> Vec<f32> {
    mask.path
        .nodes
        .iter()
        .flat_map(|n| n.geometry.sample(frame))
        .collect()
}

/// Build one validated path replacement, so a pose operation is atomic and one undo.
/// `action.property` is ignored: this helper always targets the complete path.
pub(super) fn edit(
    mask: &LayerMask,
    offset: i32,
    action: MaskAction,
) -> motion_core::Result<LayerMask> {
    let local = |f: u32| {
        i32::try_from(i64::from(f) - i64::from(offset))
            .map_err(|_| motion_core::Error::Invalid("mask local frame overflow".into()))
    };
    let mut result = mask.clone();
    let frames = frames(mask);
    let has = |frame| frames.binary_search(&frame).is_ok();
    match &action {
        MaskAction::MoveKey { from, to, .. } | MaskAction::CopyKey { from, to, .. } => {
            let (from, to) = (local(*from)?, local(*to)?);
            if !has(from) || (from != to && has(to)) {
                return Err(motion_core::Error::Invalid(
                    "path key missing or destination occupied".into(),
                ));
            }
        }
        MaskAction::DeleteKey { frame, .. } | MaskAction::Curve { frame, .. } => {
            if !has(local(*frame)?) {
                return Err(motion_core::Error::Invalid("path key missing".into()));
            }
        }
        MaskAction::Set { value, .. } if value.len() != mask.path.nodes.len() * 6 => {
            return Err(motion_core::Error::Invalid(
                "path pose dimension mismatch".into(),
            ));
        }
        _ => {}
    }
    for (index, node) in result.path.nodes.iter_mut().enumerate() {
        let t = &mut node.geometry;
        let contains = |f: i32| t.keys.iter().any(|k| k.frame == f);
        match &action {
            MaskAction::Animate { frame, enabled, .. } => {
                t.set_animated(local(*frame)?, *enabled)?
            }
            MaskAction::Set {
                frame,
                value,
                animated,
                ..
            } => {
                if let Some(enabled) = animated {
                    t.set_animated(local(*frame)?, *enabled)?;
                }
                t.set_at(
                    local(*frame)?,
                    value[index * 6..(index + 1) * 6].try_into().unwrap(),
                )?;
            }
            MaskAction::DeleteKey { frame, .. } if contains(local(*frame)?) => {
                t.delete_key(local(*frame)?)?
            }
            MaskAction::MoveKey { from, to, .. } if contains(local(*from)?) => {
                t.move_key(local(*from)?, local(*to)?)?
            }
            MaskAction::CopyKey { from, to, .. } if contains(local(*from)?) => {
                t.copy_key(local(*from)?, local(*to)?)?
            }
            MaskAction::Curve { frame, easing, .. } => {
                let frame = local(*frame)?;
                if contains(frame) && t.keys.iter().any(|k| k.frame > frame) {
                    t.set_curve(frame, *easing)?;
                }
            }
            MaskAction::DeleteKey { .. }
            | MaskAction::MoveKey { .. }
            | MaskAction::CopyKey { .. } => {}
            _ => {
                return Err(motion_core::Error::Invalid(
                    "unsupported path key operation".into(),
                ))
            }
        }
    }
    result.validate()?;
    Ok(result)
}

pub(super) fn command(
    object: u64,
    mask: &LayerMask,
    offset: i32,
    action: MaskAction,
) -> Option<Action> {
    match edit(mask, offset, action) {
        Ok(mask) => {
            REJECTED_EDITS.success();
            Some(Action::Edit(Command::Mask {
                object,
                action: MaskAction::Path {
                    mask: mask.id,
                    path: mask.path,
                },
            }))
        }
        Err(error) => {
            if REJECTED_EDITS.failure("mask_path_edit", format_args!("object={object} mask={} reason={error}", mask.id)) == 1 {
                log::warn!("mask path edit rejected: object={object} mask={} reason={error}", mask.id);
            }
            None
        }
    }
}

pub(super) fn node_command(
    object: u64,
    mask: &LayerMask,
    offset: i32,
    node: u64,
    frame: u32,
    geometry: [f32; 6],
) -> Option<Action> {
    if !mask_keys::animated(mask, CHANNEL) {
        return Some(Action::Edit(Command::Mask {
            object,
            action: MaskAction::Set {
                mask: mask.id,
                property: MaskProperty::Node(node),
                frame,
                value: geometry.to_vec(),
                animated: None,
            },
        }));
    }
    let index = mask.path.nodes.iter().position(|n| n.id == node)?;
    let mut pose = values(mask, f64::from(frame) - f64::from(offset));
    pose[index * 6..(index + 1) * 6].copy_from_slice(&geometry);
    command(
        object,
        mask,
        offset,
        MaskAction::Set {
            mask: mask.id,
            property: MaskProperty::Node(node),
            frame,
            value: pose,
            animated: Some(true),
        },
    )
}
