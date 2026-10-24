//! Authored paths cross the worker boundary as immutable typed metadata.
use super::{PathSource, Stamp};
use motion_core::{PathClipboard, PathSelection};
use motion_host::session::Session;
use std::sync::Arc;

pub(crate) struct PathRead {
    pub stamp: Stamp,
    pub frame: u32,
    pub clipboard: Option<Arc<PathClipboard>>,
    pub superseded: bool,
}
pub(crate) fn read_paths(
    session: &Session,
    source: PathSource,
    selections: Vec<PathSelection>,
) -> motion_host::Result<PathRead> {
    let stamp = Stamp::of(session);
    if !source.matches(&stamp) {
        return Ok(PathRead {
            stamp,
            frame: session.frame.floor() as u32,
            clipboard: None,
            superseded: true,
        });
    }
    let clipboard = if selections.is_empty() {
        None
    } else {
        Some(Arc::new(PathClipboard::capture(
            session.engine.project(),
            &selections,
            session.frame.floor() as u32,
        )?))
    };
    Ok(PathRead {
        stamp,
        frame: session.frame.floor() as u32,
        clipboard,
        superseded: false,
    })
}
