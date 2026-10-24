//! Bounded observer-only history. Project data and its undo stack are never touched.
use crate::session::{Result, Session};
use motion_core::{ObservationView, Observer};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Orbit,
    Pan,
    Dolly,
}
#[derive(Clone, Debug)]
pub enum Gesture {
    Begin {
        id: u64,
    },
    Update {
        id: u64,
        mode: Mode,
        delta: [f32; 2],
        viewport: [u32; 2],
    },
    End {
        id: u64,
        commit: bool,
    },
    Undo,
    Redo,
}
#[derive(Default)]
pub(crate) struct History {
    context: Option<(std::path::PathBuf, String, u64, u64, ObservationView, bool)>,
    pending: Option<(u64, Observer)>,
    undo: VecDeque<Observer>,
    redo: VecDeque<Observer>,
}
impl History {
    fn valid(&self, s: &Session) -> bool {
        self.context
            .as_ref()
            .is_some_and(|(root, comp, revision, frame, view, observing)| {
                root == &s.root
                    && comp == &s.engine.project().composition_id
                    && *revision == s.engine.revision()
                    && *frame == s.frame.to_bits()
                    && *view == s.observer.view
                    && *observing == s.observing
            })
    }
    pub(crate) fn can_undo(&self, s: &Session) -> bool {
        self.valid(s) && !self.undo.is_empty()
    }
    pub(crate) fn can_redo(&self, s: &Session) -> bool {
        self.valid(s) && !self.redo.is_empty()
    }
}
fn key(o: &Observer) -> [f32; 6] {
    [
        o.camera.azimuth.value,
        o.camera.elevation.value,
        o.camera.radius.value,
        o.camera.target.value[0],
        o.camera.target.value[1],
        o.camera.target.value[2],
    ]
}
fn retain(stack: &mut VecDeque<Observer>, value: Observer) {
    const LIMIT: usize = 64;
    if stack.len() == LIMIT {
        stack.pop_front();
    }
    stack.push_back(value);
}
// Re-sampling here is rollback cleanup, never a retry of the rejected request.
fn install(s: &mut Session, observer: Observer) -> Result<()> {
    let before = std::mem::replace(&mut s.observer, observer);
    if let Err(primary) = s.sample() {
        s.observer = before;
        return match s.sample() {
            Ok(()) => Err(primary),
            Err(secondary) => Err(crate::Error::Cleanup {
                primary: Box::new(primary),
                secondary: Box::new(secondary),
            }),
        };
    }
    s.view_revision = s.view_revision.wrapping_add(1);
    Ok(())
}
pub fn apply(id: i64, action: Gesture) -> Result<serde_json::Value> {
    let context = match &action {
        Gesture::Begin { id } => format!("observation gesture phase=begin token={id}"),
        Gesture::Update { id, mode, .. } => {
            format!("observation gesture phase=update token={id} mode={mode:?}")
        }
        Gesture::End { id, commit } => {
            format!("observation gesture phase=end token={id} commit={commit}")
        }
        Gesture::Undo => "observation gesture phase=undo".into(),
        Gesture::Redo => "observation gesture phase=redo".into(),
    };
    crate::session::with_session(id, |s| {
        let context = (
            s.root.clone(),
            s.engine.project().composition_id.clone(),
            s.engine.revision(),
            s.frame.to_bits(),
            s.observer.view,
            s.observing,
        );
        if s.observer_history.context.as_ref() != Some(&context) {
            // A seek or authored edit cancels the active candidate in the same
            // composition. A project/view replacement owns its new observer.
            let same_view = s.observer_history.context.as_ref().is_some_and(|old| {
                old.0 == context.0 && old.1 == context.1 && old.4 == context.4 && old.5 == context.5
            });
            if same_view {
                if let Some((_, before)) = s.observer_history.pending.as_ref() {
                    install(s, before.clone())?;
                }
            }
            s.observer_history = History {
                context: Some(context),
                ..Default::default()
            };
        }
        match action {
            Gesture::Begin { id } => {
                if !s.observing || id == 0 {
                    return Err("camera gesture requires an observation view and nonzero id".into());
                }
                if s.observer_history.pending.is_some() {
                    return Err("observation gesture already active".into());
                }
                s.observer_history.pending = Some((id, s.observer.clone()));
            }
            Gesture::Update {
                id,
                mode,
                delta,
                viewport,
            } => {
                let (current, before) = s
                    .observer_history
                    .pending
                    .as_ref()
                    .ok_or("observation gesture is not active")?;
                if *current != id || !s.observing {
                    return Err("observation gesture owner changed".into());
                }
                if viewport.into_iter().any(|v| v == 0 || v > 65536)
                    || delta
                        .into_iter()
                        .any(|v| !v.is_finite() || v.abs() > 1_000_000.)
                {
                    return Err("invalid observation gesture coordinates".into());
                }
                let mut next = before.clone();
                match mode {
                    Mode::Orbit => {
                        if next.view != ObservationView::Free {
                            return Err("orbit requires a free observation view".into());
                        }
                        next.orbit(delta[0] * 0.2, delta[1] * 0.2)?;
                    }
                    Mode::Dolly => next.zoom((-delta[1] / 240.).clamp(-20., 20.).exp())?,
                    Mode::Pan => {
                        // Use the gesture's original projection on every update, so clamping,
                        // coalescing and cancelling cannot accumulate drift.
                        let pose =
                            before.pose(s.engine.project().width, s.engine.project().height)?;
                        let saved = std::mem::replace(&mut s.scene.camera, pose);
                        let delta =
                            s.scene
                                .screen_translation(before.camera.target.value, delta, viewport);
                        s.scene.camera = saved;
                        let delta = delta?;
                        next.camera.target.value =
                            std::array::from_fn(|i| before.camera.target.value[i] - delta[i]);
                    }
                }
                install(s, next)?;
            }
            Gesture::End { id, commit } => {
                // A view/project change may already have discarded its ephemeral transaction.
                if let Some((current, before)) = s.observer_history.pending.as_ref() {
                    if *current != id {
                        return Err("observation gesture owner changed".into());
                    }
                    let before = before.clone();
                    if commit {
                        if key(&before) != key(&s.observer) {
                            retain(&mut s.observer_history.undo, before);
                            s.observer_history.redo.clear();
                        }
                    } else {
                        install(s, before)?;
                    }
                    s.observer_history.pending = None;
                }
            }
            Gesture::Undo | Gesture::Redo => {
                if s.observer_history.pending.is_some() {
                    return Err("finish observation gesture before undo".into());
                }
                let redo = matches!(action, Gesture::Redo);
                let stack = if redo {
                    &s.observer_history.redo
                } else {
                    &s.observer_history.undo
                };
                // Empty view history is a normal no-op, independent of project history.
                if let Some(next) = stack.back().cloned() {
                    let before = s.observer.clone();
                    install(s, next)?;
                    if redo {
                        s.observer_history.redo.pop_back();
                        retain(&mut s.observer_history.undo, before);
                    } else {
                        s.observer_history.undo.pop_back();
                        retain(&mut s.observer_history.redo, before);
                    }
                }
            }
        }
        s.snapshot()
    })
    .map_err(|e| crate::Error::with_context(context, e))
}
