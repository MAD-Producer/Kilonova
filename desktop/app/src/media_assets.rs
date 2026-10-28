//! Immutable identity attached to an asset action; UI rows never own a session.
use motion_core::media_assets::{AssetRef, AssetUsage};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Source {
    pub root: String,
    pub generation: u64,
    pub revision: u64,
    pub target: AssetRef,
}
impl Source {
    pub(crate) fn read(state: &Value, target: AssetRef) -> Result<Self, String> {
        Ok(Self {
            root: state["root"]
                .as_str()
                .ok_or("asset action: missing project root")?
                .into(),
            generation: state["document_generation"]
                .as_u64()
                .ok_or("asset action: missing document generation")?,
            revision: state["revision"]
                .as_u64()
                .ok_or("asset action: missing revision")?,
            target,
        })
    }
    pub(crate) fn current(&self, state: &Value) -> bool {
        Self::read(state, self.target).as_ref() == Ok(self)
    }
}

#[derive(Debug)]
pub(crate) enum Intent {
    Place,
    RefreshThumbnail,
    Preview { owner: Option<motion_ui::dock::Panel> },
    Replace,
    RelinkMissing,
    RevealFile,
    Remove,
    RevealUsage(AssetUsage),
}
