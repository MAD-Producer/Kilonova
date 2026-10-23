use motion_core::ExpressionTarget;
use std::hash::{Hash, Hasher};

pub(super) fn expression_rows(targets: &[ExpressionTarget], hash: &mut impl Hasher) {
    targets.len().hash(hash);
    for target in targets {
        std::mem::discriminant(target).hash(hash);
        match target {
            ExpressionTarget::Property {
                object,
                property,
                axis,
            } => {
                object.hash(hash);
                std::mem::discriminant(property).hash(hash);
                axis.map(motion_core::Axis::index).hash(hash);
            }
            ExpressionTarget::Effect {
                object,
                effect,
                param,
            } => {
                object.hash(hash);
                effect.hash(hash);
                param.hash(hash);
            }
        }
    }
}
