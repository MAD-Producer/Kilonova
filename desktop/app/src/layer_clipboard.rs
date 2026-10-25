//! Application clipboard metadata and owned, immutable resource snapshots.
//! File IO runs on the clipboard worker; only the final transaction owns the
//! session. A submitted paste also transfers ownership of staged files to it.
mod storage;
mod keyframes;
mod paths;
pub(crate) mod controller;
#[cfg(test)]
mod tests;
pub(crate) mod worker;

use motion_core::LayerClipboard;
use motion_host::session::Session;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub(crate) use storage::{Frozen, Prepared};
pub(crate) use keyframes::{read_keyframes, KeyframeRead, KeyframeSource};
pub(crate) use keyframes::KeyframeSource as PathSource;
pub(crate) use paths::{read_paths, PathRead};
pub(crate) use paths::{edit_path, PathOperation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub generation: u64,
    pub root: PathBuf,
    pub composition: String,
    pub revision: u64,
}
impl Stamp {
    fn of(session: &Session) -> Self {
        Self {
            generation: session.document_generation,
            root: session.root.clone(),
            composition: session.engine.project().composition_id.clone(),
            revision: session.engine.revision(),
        }
    }
    pub(crate) fn same_document(&self, other: &Self) -> bool {
        self.generation == other.generation
            && self.root == other.root
            && self.composition == other.composition
    }
}

pub(crate) struct Read {
    pub stamp: Stamp,
    pub frame: u32,
    pub clipboard: Option<LayerClipboard>,
    pub project: Option<motion_core::Project>,
}
pub(crate) fn read(session: &Session, objects: Vec<u64>) -> motion_host::Result<Read> {
    if motion_core::serialized_bytes(session.engine.project()) > 16 * 1024 * 1024 {
        return Err("clipboard document metadata exceeds 16 MiB".into());
    }
    Ok(Read {
        stamp: Stamp::of(session),
        frame: session.frame.floor() as u32,
        project: objects.is_empty().then(|| session.engine.project().clone()),
        clipboard: if objects.is_empty() {
            None
        } else {
            Some(LayerClipboard::capture(session.engine.project(), objects)?)
        },
    })
}

pub(crate) enum Edit {
    CutPaths {
        stamp: Stamp,
        clipboard: Arc<motion_core::PathClipboard>,
        cancel: Arc<AtomicBool>,
    },
    PastePaths {
        stamp: Stamp,
        clipboard: Arc<motion_core::PathClipboard>,
        target: motion_core::PathDestination,
        at_frame: Option<u32>,
        cancel: Arc<AtomicBool>,
    },
    CutKeyframes {
        stamp: Stamp,
        clipboard: Arc<motion_core::KeyframeClipboard>,
        cancel: Arc<AtomicBool>,
    },
    PasteKeyframes {
        stamp: Stamp,
        clipboard: Arc<motion_core::KeyframeClipboard>,
        targets: Vec<motion_core::KeyframeTarget>,
        at_frame: Option<u32>,
        cancel: Arc<AtomicBool>,
    },
    Cut {
        stamp: Stamp,
        objects: Vec<u64>,
        frame: u32,
        cancel: Arc<AtomicBool>,
    },
    Paste {
        prepared: Prepared,
        at_frame: Option<u32>,
        cancel: Arc<AtomicBool>,
    },
}
pub(crate) fn commit(session: &mut Session, edit: Edit) -> motion_host::Result<Value> {
    let (stamp, cancel) = match &edit {
        Edit::CutPaths { stamp, cancel, .. }
        | Edit::PastePaths { stamp, cancel, .. }
        | Edit::CutKeyframes { stamp, cancel, .. }
        | Edit::PasteKeyframes { stamp, cancel, .. } => (stamp, cancel),
        Edit::Cut { stamp, cancel, .. } => (stamp, cancel),
        Edit::Paste {
            prepared, cancel, ..
        } => (&prepared.target, cancel),
    };
    if cancel.load(Ordering::Acquire) || stamp != &Stamp::of(session) {
        log::debug!(
            "editor clipboard commit superseded: generation={} revision={} composition={}",
            stamp.generation,
            stamp.revision,
            stamp.composition
        );
        return Ok(serde_json::json!({"clipboard_status":"superseded"}));
    }
    let result = match edit {
        Edit::CutPaths { clipboard, .. } => session.apply_command_batch(vec![
            motion_core::Command::RemovePaths { clipboard: Box::new((*clipboard).clone()) },
        ]),
        Edit::PastePaths { clipboard, target, at_frame, .. } => session.apply_command_batch(vec![
            motion_core::Command::PastePaths { clipboard: Box::new((*clipboard).clone()), target, at_frame },
        ]),
        Edit::CutKeyframes { clipboard, .. } => session.apply_command_batch(vec![
            motion_core::Command::RemoveKeyframes { clipboard: Box::new((*clipboard).clone()) },
        ]),
        Edit::PasteKeyframes { clipboard, targets, at_frame, .. } => session.apply_command_batch(vec![
            motion_core::Command::PasteKeyframes { clipboard: Box::new((*clipboard).clone()), targets, at_frame },
        ]),
        Edit::Cut { objects, frame, .. } => {
            session.apply_command_batch(vec![motion_core::Command::RemoveLayers { objects, frame }])
        }
        Edit::Paste {
            prepared, at_frame, ..
        } => paste_with(session, prepared, at_frame, |s, commands| {
            s.apply_command_batch(commands)
        }),
    };
    result.map(|mut state| {
        state["clipboard_status"] = "applied".into();
        state
    })
}

fn paste_with(
    session: &mut Session,
    prepared: Prepared,
    at_frame: Option<u32>,
    apply: impl FnOnce(&mut Session, Vec<motion_core::Command>) -> motion_host::Result<Value>,
) -> motion_host::Result<Value> {
    let before = session.engine.revision();
    let Prepared {
        clipboard,
        target,
        origin,
        files,
    } = prepared;
    let keep_external_references = target.same_document(&origin);
    if keep_external_references {
        validate_external_references(&clipboard, session.engine.project())?;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply(
            session,
            vec![motion_core::Command::PasteLayers {
                clipboard: Box::new(clipboard),
                at_frame,
                keep_external_references,
            }],
        )
    }));
    // Snapshot/sample may fail or panic after commit. Preserve referenced
    // sources before propagating the original failure to the engine boundary.
    if session.engine.revision() != before {
        for file in files {
            file.keep();
        }
    }
    match result {
        Ok(result) => result,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

fn validate_external_references(
    clipboard: &LayerClipboard,
    target: &motion_core::Project,
) -> motion_host::Result<()> {
    let view = clipboard.resolve()?;
    for layer in view.layers {
        let references = layer
            .parent
            .iter()
            .chain(layer.camera_data().into_iter().flat_map(|c| &c.parent))
            .filter_map(|p| p.object)
            .chain(layer.track_matte.iter().map(|m| m.source))
            .chain(
                layer
                    .effects
                    .iter()
                    .filter_map(|e| e.image_input.and_then(motion_core::EffectImageInput::layer)),
            );
        for id in references.filter(|id| !clipboard.objects.contains(id)) {
            let unchanged = if id == 0 {
                clipboard.source.camera == target.camera
            } else {
                clipboard.source.layers.iter().find(|l| l.id == id)
                    == target.layers.iter().find(|l| l.id == id)
            };
            if !unchanged {
                return Err(format!("clipboard external layer changed after copying; copy the dependency together: object={} dependency={id}", layer.id).into());
            }
        }
    }
    Ok(())
}
