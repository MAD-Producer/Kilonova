//! Typed metadata transport: keyframe payloads never pass through the bounded
//! JSON edit request. The engine owns capture and stamps the exact document it read.
use super::Stamp;
use motion_core::{KeyframeClipboard, KeyframeSelection};
use motion_host::session::Session;
use std::sync::Arc;

/// Source identity captured by the UI, before an action enters either queue.
/// Missing identity cannot authorize a read, including reused object IDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KeyframeSource {
    root: std::path::PathBuf,
    composition: String,
    generation: Option<u64>,
    revision: Option<u64>,
}
impl KeyframeSource {
    pub(crate) fn current(&self, session: &Session) -> bool {
        self.matches(&Stamp::of(session))
    }
    pub(crate) fn of(state: &serde_json::Value) -> Self {
        Self {
            root: state["root"].as_str().unwrap_or("").into(),
            composition: state["composition"]
                .as_str()
                .or_else(|| state["project"]["composition_id"].as_str())
                .unwrap_or("")
                .into(),
            generation: state["document_generation"].as_u64(),
            revision: state["revision"].as_u64(),
        }
    }
    pub(crate) fn matches(&self, stamp: &Stamp) -> bool {
        self.root == stamp.root
            && self.composition == stamp.composition
            && self.generation == Some(stamp.generation)
            && self.revision == Some(stamp.revision)
    }
}

pub(crate) struct KeyframeRead {
    pub stamp: Stamp,
    pub frame: u32,
    pub clipboard: Option<Arc<KeyframeClipboard>>,
    pub superseded: bool,
}

pub(crate) fn read_keyframes(
    session: &Session,
    source: KeyframeSource,
    selections: Vec<KeyframeSelection>,
) -> motion_host::Result<KeyframeRead> {
    let stamp = Stamp::of(session);
    if !source.matches(&stamp) {
        return Ok(KeyframeRead {
            stamp,
            frame: session.frame.floor() as u32,
            clipboard: None,
            superseded: true,
        });
    }
    let clipboard = if selections.is_empty() {
        None
    } else {
        Some(Arc::new(KeyframeClipboard::capture(
            session.engine.project(),
            &selections,
        )?))
    };
    Ok(KeyframeRead {
        stamp,
        frame: session.frame.floor() as u32,
        clipboard,
        superseded: false,
    })
}
