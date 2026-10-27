//! One application clipboard and one pending operation, shared by all windows.
//! Polling never waits for the engine or filesystem. Cancellation retains the
//! pending slot until its worker replies, including an IO call that blocks.
use super::{
    worker::{Job, Output, Work},
    Edit, Frozen, KeyframeRead, Read,
};
use crate::engine::{Command, Engine};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
        Arc,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Intent {
    Copy,
    Cut,
    Paste { in_place: bool },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    Copied,
    Applied,
    Cancelled,
    Superseded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Domain {
    Curves,
    Layers,
    Keyframes,
    Paths,
}
pub(crate) struct Completion {
    pub domain: Domain,
    pub intent: Intent,
    pub status: Status,
    pub state: Option<Value>,
}
enum Stage {
    ReadingCurves(Receiver<Result<super::curves::Read, String>>, Vec<motion_core::KeyframeSelection>),
    CurveIO(super::curves::Job, super::Stamp, Vec<motion_core::KeyframeSelection>),
    ReadingPaths(Receiver<Result<super::PathRead, String>>, Option<motion_core::PathDestination>),
    Reading(Receiver<Result<Read, String>>, PathBuf),
    ReadingKeyframes(
        Receiver<Result<KeyframeRead, String>>,
        Vec<motion_core::KeyframeTarget>,
    ),
    Working(Job, Option<u32>),
    Ready(Edit),
    Committing(Receiver<Result<Value, String>>),
}
struct Pending {
    domain: Domain,
    intent: Intent,
    cancel: Arc<AtomicBool>,
    stage: Stage,
}
#[derive(Default)]
pub(crate) struct Clipboard {
    curves: super::curves::Service,
    frozen: Option<Arc<Frozen>>,
    keyframes: Option<Arc<motion_core::KeyframeClipboard>>,
    paths: Option<Arc<motion_core::PathClipboard>>,
    pending: Option<Pending>,
}
impl Drop for Clipboard {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl Clipboard {
    pub(crate) fn available(&self) -> bool {
        self.frozen.is_some()
    }
    pub(crate) fn keyframes_available(&self) -> bool {
        self.keyframes.is_some()
    }
    pub(crate) fn paths_available(&self) -> bool { self.paths.is_some() }
    pub(crate) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(crate) fn committing(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| matches!(p.stage, Stage::Committing(_)))
    }
    pub(crate) fn phase(&self) -> Option<&'static str> {
        self.pending.as_ref().map(|pending| {
            if pending.cancel.load(Ordering::Acquire) {
                return "cancelling";
            }
            match &pending.stage {
                Stage::ReadingCurves(..) => "reading",
                Stage::CurveIO(job, ..) => job.status().1,
                Stage::Reading(..) | Stage::ReadingKeyframes(..) | Stage::ReadingPaths(..) => "reading",
                Stage::Working(job, _) => job.status().1,
                Stage::Ready(_) => "ready",
                Stage::Committing(_) => "committing",
            }
        })
    }
    pub(crate) fn cancel(&mut self) {
        if let Some(pending) = &self.pending {
            pending.cancel.store(true, Ordering::Release);
        }
    }
    pub(crate) fn start_curves(
        &mut self, engine: &Engine, intent: Intent, source: super::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
    ) -> Result<bool, String> {
        if self.busy() || selections.is_empty() || intent == Intent::Cut { return Ok(false); }
        let selection = if intent == Intent::Copy {
            if selections.len() != 1 || selections[0].frames.len() != 1 {
                return Err("copy easing requires one source interval".into());
            }
            Some(selections[0].clone())
        } else { None };
        let receive = engine.request(|reply| Command::CurveClipboardRead { source, selection, reply })?;
        self.pending = Some(Pending { domain: Domain::Curves, intent,
            cancel: Arc::new(AtomicBool::new(false)), stage: Stage::ReadingCurves(receive, selections) });
        Ok(true)
    }
    pub(crate) fn start(
        &mut self,
        engine: &Engine,
        intent: Intent,
        objects: Vec<u64>,
        cache: PathBuf,
    ) -> Result<bool, String> {
        if self.busy()
            || match intent {
                Intent::Copy | Intent::Cut => objects.is_empty(),
                Intent::Paste { .. } => !self.available(),
            }
        {
            return Ok(false);
        }
        let objects = if matches!(intent, Intent::Paste { .. }) {
            vec![]
        } else {
            objects
        };
        let receive = engine.request(|reply| Command::ClipboardRead { objects, reply })?;
        self.pending = Some(Pending {
            domain: Domain::Layers,
            intent,
            cancel: Arc::new(AtomicBool::new(false)),
            stage: Stage::Reading(receive, cache),
        });
        Ok(true)
    }
    /// Property selection is resolved by its owning editor. An empty/stale
    /// selection never falls back to copying layers. Both caches share the same
    /// pending slot, cancellation and shell commit barrier.
    pub(crate) fn start_keyframes(
        &mut self,
        engine: &Engine,
        intent: Intent,
        source: super::KeyframeSource,
        selections: Vec<motion_core::KeyframeSelection>,
        targets: Vec<motion_core::KeyframeTarget>,
    ) -> Result<bool, String> {
        if self.busy()
            || match intent {
                Intent::Copy | Intent::Cut => selections.is_empty(),
                Intent::Paste { .. } => !self.keyframes_available() || targets.is_empty(),
            }
        {
            return Ok(false);
        }
        let selections = if matches!(intent, Intent::Paste { .. }) {
            vec![]
        } else {
            selections
        };
        let receive =
            engine.request(|reply| Command::KeyframeClipboardRead { source, selections, reply })?;
        self.pending = Some(Pending {
            domain: Domain::Keyframes,
            intent,
            cancel: Arc::new(AtomicBool::new(false)),
            stage: Stage::ReadingKeyframes(receive, targets),
        });
        Ok(true)
    }
    pub(crate) fn start_paths(
        &mut self, engine: &Engine, intent: Intent, source: super::PathSource,
        selections: Vec<motion_core::PathSelection>, target: Option<motion_core::PathDestination>,
    ) -> Result<bool, String> {
        if self.busy() || match intent {
            Intent::Copy | Intent::Cut => selections.is_empty(),
            Intent::Paste { .. } => !self.paths_available() || target.is_none(),
        } { return Ok(false); }
        let selections = if matches!(intent, Intent::Paste { .. }) { vec![] } else { selections };
        let receive = engine.request(|reply| Command::PathClipboardRead { source, selections, reply })?;
        self.pending = Some(Pending { domain: Domain::Paths, intent, cancel: Arc::new(AtomicBool::new(false)),
            stage: Stage::ReadingPaths(receive, target) });
        Ok(true)
    }
    /// Only submit a transaction when the shell has drained prior edits and
    /// project transitions. While committing, it must wait before dispatching
    /// another edit, so an old acknowledgement cannot replace newer UI state.
    pub(crate) fn poll(
        &mut self,
        engine: &Engine,
        allow_commit: bool,
    ) -> Option<Result<Completion, String>> {
        let pending = self.pending.take()?;
        let domain = pending.domain;
        let intent = pending.intent;
        let cancel = pending.cancel;
        let cancelled = || cancel.load(Ordering::Acquire);
        let done = |status, state| {
            Some(Ok(Completion {
                domain,
                intent,
                status,
                state,
            }))
        };
        let stage = match pending.stage {
            Stage::ReadingCurves(receive, selections) => {
                let Some(result) = engine.poll_reply(&receive, "curve_clipboard_read") else {
                    self.pending = Some(Pending { domain, intent, cancel, stage: Stage::ReadingCurves(receive, selections) });
                    return None;
                };
                let source = match result { Ok(source) => source, Err(cause) => return Some(Err(cause)) };
                if cancelled() { return done(Status::Cancelled, None); }
                if source.superseded { return done(Status::Superseded, None); }
                let work = if intent == Intent::Copy {
                    let Some(clipboard) = source.clipboard else {
                        log::error!("curve clipboard capture failed phase=reading reason=engine returned no easing");
                        return Some(Err("curve clipboard capture returned no easing".into()));
                    };
                    super::curves::Work::Copy(clipboard)
                } else { super::curves::Work::Paste };
                log::info!("curve clipboard source phase=validated generation={} revision={} composition={} objects={:?} channels={}",
                    source.stamp.generation, source.stamp.revision,
                    source.stamp.composition.chars().take(128).collect::<String>(),
                    selections.iter().take(8).map(|s| s.target.object()).collect::<Vec<_>>(), selections.len());
                match self.curves.submit(work, cancel.clone()) {
                    Ok(job) => Stage::CurveIO(job, source.stamp, selections),
                    Err(cause) => return Some(Err(cause)),
                }
            }
            Stage::CurveIO(job, stamp, selections) => {
                let Some(result) = job.poll() else {
                    self.pending = Some(Pending { domain, intent, cancel, stage: Stage::CurveIO(job, stamp, selections) });
                    return None;
                };
                let result = match result { Ok(value) => value, Err(cause) => return Some(Err(cause)) };
                // A native write that already completed remains a successful
                // copy even if cancellation arrived while the OS owned it.
                match result {
                    super::curves::Output::Copied => return done(Status::Copied, None),
                    super::curves::Output::Cancelled => return done(Status::Cancelled, None),
                    super::curves::Output::Paste(clipboard) => {
                        if cancelled() { return done(Status::Cancelled, None); }
                        Stage::Ready(Edit::PasteEasing { stamp, clipboard, selections, cancel: cancel.clone() })
                    }
                }
            }
            Stage::ReadingPaths(receive, target) => {
                let Some(result) = engine.poll_reply(&receive, "path_clipboard_read") else {
                    self.pending = Some(Pending { domain, intent, cancel, stage: Stage::ReadingPaths(receive, target) });
                    return None;
                };
                let source = match result { Ok(source) => source, Err(error) => return Some(Err(error)) };
                if cancelled() { return done(Status::Cancelled, None); }
                if source.superseded { return done(Status::Superseded, None); }
                match intent {
                    Intent::Copy | Intent::Cut => {
                        let Some(clipboard) = source.clipboard else {
                            log::error!("path clipboard capture failed phase=reading reason=engine returned no paths");
                            return Some(Err("path clipboard capture returned no paths".into()));
                        };
                        self.paths = Some(clipboard.clone());
                        if intent == Intent::Copy { return done(Status::Copied, None); }
                        Stage::Ready(Edit::CutPaths { stamp: source.stamp, clipboard, cancel: cancel.clone() })
                    }
                    Intent::Paste { in_place } => {
                        let Some(clipboard) = self.paths.clone() else {
                            log::error!("path clipboard paste failed phase=reading reason=application cache lost during pending paste");
                            return Some(Err("path clipboard cache lost during paste".into()));
                        };
                        let Some(target) = target else {
                            log::error!("path clipboard paste failed phase=reading reason=destination lost during pending paste");
                            return Some(Err("path clipboard destination lost during paste".into()));
                        };
                        Stage::Ready(Edit::PastePaths { stamp: source.stamp, clipboard, target,
                            at_frame: (!in_place).then_some(source.frame), cancel: cancel.clone() })
                    }
                }
            }
            Stage::ReadingKeyframes(receive, targets) => {
                let Some(result) = engine.poll_reply(&receive, "keyframe_clipboard_read") else {
                    self.pending = Some(Pending {
                        domain,
                        intent,
                        cancel,
                        stage: Stage::ReadingKeyframes(receive, targets),
                    });
                    return None;
                };
                let source = match result {
                    Ok(source) => source,
                    Err(error) => return Some(Err(error)),
                };
                if cancelled() {
                    return done(Status::Cancelled, None);
                }
                if source.superseded {
                    return done(Status::Superseded, None);
                }
                match intent {
                    Intent::Copy | Intent::Cut => {
                        let Some(clipboard) = source.clipboard else {
                            log::error!("keyframe clipboard capture failed phase=reading reason=engine returned no keys");
                            return Some(Err("keyframe clipboard capture returned no keys".into()));
                        };
                        self.keyframes = Some(clipboard.clone());
                        if intent == Intent::Copy {
                            return done(Status::Copied, None);
                        }
                        Stage::Ready(Edit::CutKeyframes {
                            stamp: source.stamp,
                            clipboard,
                            cancel: cancel.clone(),
                        })
                    }
                    Intent::Paste { in_place } => {
                        let Some(clipboard) = self.keyframes.clone() else {
                            log::error!("keyframe clipboard paste failed phase=reading reason=application cache lost during pending paste");
                            return Some(Err("keyframe clipboard cache lost during paste".into()));
                        };
                        Stage::Ready(Edit::PasteKeyframes {
                            stamp: source.stamp,
                            clipboard,
                            targets,
                            at_frame: (!in_place).then_some(source.frame),
                            cancel: cancel.clone(),
                        })
                    }
                }
            }
            Stage::Reading(receive, cache) => {
                let Some(result) = engine.poll_reply(&receive, "clipboard_read") else {
                    self.pending = Some(Pending {
                        domain,
                        intent,
                        cancel,
                        stage: Stage::Reading(receive, cache),
                    });
                    return None;
                };
                let source = match result {
                    Ok(source) => source,
                    Err(error) => return Some(Err(error)),
                };
                if cancelled() {
                    return done(Status::Cancelled, None);
                }
                let at_frame = match intent {
                    Intent::Paste { in_place: false } => Some(source.frame),
                    _ => None,
                };
                let work = match intent {
                    Intent::Copy | Intent::Cut => Work::Capture { source, cache },
                    Intent::Paste { .. } => {
                        let Some(frozen) = &self.frozen else {
                            log::error!("layer clipboard prepare failed phase=reading reason=application cache lost during pending paste");
                            return Some(Err(
                                "application clipboard cache lost during paste".into()
                            ));
                        };
                        Work::Prepare {
                            source: frozen.clone(),
                            target: source,
                        }
                    }
                };
                let job = match Job::start(work) {
                    Ok(job) => job,
                    Err(error) => return Some(Err(error)),
                };
                // Keep one cancellation flag across worker and engine phases.
                let cancel = job.cancel.clone();
                self.pending = Some(Pending {
                    domain,
                    intent,
                    cancel,
                    stage: Stage::Working(job, at_frame),
                });
                return None;
            }
            Stage::Working(mut job, at_frame) => {
                let Some(result) = job.poll() else {
                    self.pending = Some(Pending {
                        domain,
                        intent,
                        cancel,
                        stage: Stage::Working(job, at_frame),
                    });
                    return None;
                };
                let output = match result {
                    Ok(output) => output,
                    Err(error) if error == "clipboard operation cancelled" => {
                        return done(Status::Cancelled, None)
                    }
                    Err(error) => return Some(Err(error)),
                };
                if cancelled() {
                    return done(Status::Cancelled, None);
                }
                match output {
                    Output::Captured(source) => {
                        self.frozen = Some(source.clone());
                        if intent == Intent::Copy {
                            return done(Status::Copied, None);
                        }
                        Stage::Ready(Edit::Cut {
                            stamp: source.origin.clone(),
                            objects: source.objects.clone(),
                            frame: source.frame,
                            cancel: cancel.clone(),
                        })
                    }
                    Output::Prepared(prepared) => Stage::Ready(Edit::Paste {
                        prepared,
                        at_frame,
                        cancel: cancel.clone(),
                    }),
                }
            }
            Stage::Ready(edit) => {
                if cancelled() {
                    return done(Status::Cancelled, None);
                }
                if allow_commit {
                    match engine.request(|reply| Command::ClipboardCommit { edit, reply }) {
                        Ok(receive) => Stage::Committing(receive),
                        Err(error) => return Some(Err(error)),
                    }
                } else {
                    Stage::Ready(edit)
                }
            }
            Stage::Committing(receive) => {
                let Some(result) = engine.poll_reply(&receive, "clipboard_commit") else {
                    self.pending = Some(Pending {
                        domain,
                        intent,
                        cancel,
                        stage: Stage::Committing(receive),
                    });
                    return None;
                };
                return match result {
                    Ok(state) if state["clipboard_status"] == "superseded" => done(
                        if cancelled() {
                            Status::Cancelled
                        } else {
                            Status::Superseded
                        },
                        None,
                    ),
                    // A cancellation arriving after the atomic transaction has
                    // committed cannot turn its successful result into a no-op.
                    Ok(state) => done(Status::Applied, Some(state)),
                    Err(error) => Some(Err(error)),
                };
            }
        };
        self.pending = Some(Pending {
            domain,
            intent,
            cancel,
            stage,
        });
        None
    }
}
