//! Authored paths cross the worker boundary as immutable typed metadata.
use super::{PathSource, Stamp};
use motion_core::{PathClipboard, PathSelection};
use motion_host::session::Session;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(crate) enum PathOperation {
    FirstVertex {
        target: motion_core::PathTarget,
        path: u64,
        node: u64,
    },
    CopyAs {
        target: motion_core::PathTarget,
        path: u64,
    },
}
pub(crate) fn edit_path(
    session: &mut Session,
    source: PathSource,
    operation: PathOperation,
) -> motion_host::Result<serde_json::Value> {
    if !source.matches(&Stamp::of(session)) {
        let mut state = session.snapshot()?;
        state["path_operation_status"] = "superseded".into();
        return Ok(state);
    }
    let command = match operation {
        PathOperation::FirstVertex { target, path, node } => match target {
            motion_core::PathTarget::Vector { object, item } => motion_core::Command::Vector {
                object,
                action: motion_core::vector::VectorAction::SetFirstVertex { item, path, node },
            },
            motion_core::PathTarget::Mask { object, mask } => motion_core::Command::Mask {
                object,
                action: motion_core::masks::MaskAction::SetFirstVertex { mask, path, node },
            },
        },
        PathOperation::CopyAs { target, path } => motion_core::Command::CopyPathAs { target, path },
    };
    let mut state = session.apply_command_batch(vec![command])?;
    state["path_operation_status"] = "applied".into();
    Ok(state)
}

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
