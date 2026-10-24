//! Typed metadata transport: keyframe payloads never pass through the bounded
//! JSON edit request. The engine owns capture and stamps the exact document it read.
use super::Stamp;
use motion_core::{KeyframeClipboard, KeyframeSelection};
use motion_host::session::Session;
use std::sync::Arc;

pub(crate) struct KeyframeRead {
    pub stamp: Stamp,
    pub frame: u32,
    pub clipboard: Option<Arc<KeyframeClipboard>>,
}

pub(crate) fn read_keyframes(
    session: &Session,
    selections: Vec<KeyframeSelection>,
) -> motion_host::Result<KeyframeRead> {
    let clipboard = if selections.is_empty() {
        None
    } else {
        Some(Arc::new(KeyframeClipboard::capture(
            session.engine.project(),
            &selections,
        )?))
    };
    Ok(KeyframeRead {
        stamp: Stamp::of(session),
        frame: session.frame.floor() as u32,
        clipboard,
    })
}
