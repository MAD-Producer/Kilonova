//! A compensated pivot draft owns both spatial identities until release.
use super::super::property_authoring::SpatialDraft;
use super::*;
#[derive(Clone, Debug, Default)]
pub(in crate::panels) struct AnchorKeysDraft {
    anchor: Option<SpatialDraft>,
    position: Option<SpatialDraft>,
}
impl AnchorKeysDraft {
    pub(in crate::panels) fn read(state: &Value, object: u64) -> motion_core::Result<Self> {
        // A static anchor intentionally uses the existing global operation.
        if !anchor_animated(state, object) {
            return Ok(Self::default());
        }
        Ok(Self {
            anchor: SpatialDraft::read(state, object, Property::Anchor, None)?,
            position: SpatialDraft::read(state, object, Property::Position, None)?,
        })
    }
    pub(in crate::panels) fn current(&self, state: &Value) -> motion_core::Result<bool> {
        Ok(SpatialDraft::current(&self.anchor, state)?
            && SpatialDraft::current(&self.position, state)?)
    }
    pub(in crate::panels) fn command(&self, mut command: Command) -> Command {
        if let Command::AnchorAt { keys, .. } = &mut command {
            let binding = motion_core::AnchorKeyBinding {
                anchor: self.anchor.as_ref().map(SpatialDraft::key),
                position: self.position.as_ref().map(SpatialDraft::key),
            };
            *keys = (binding.anchor.is_some() || binding.position.is_some()).then_some(binding);
        }
        command
    }
}
