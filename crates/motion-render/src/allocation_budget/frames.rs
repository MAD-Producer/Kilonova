//! Per-frame windows of the shared admitted-capacity gauge. These windows are
//! independent of the lifetime high-water mark and include overlapping work.
use super::{
    invalid, AllocationBudget, AllocationCategory, AllocationCategoryFrameInfo, State, CATEGORIES,
    CATEGORY_COUNT,
};
use crate::RenderError;
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Instant,
};

pub const MAX_ACTIVE_ALLOCATION_FRAMES: usize = 64;
pub const MAX_RECENT_ALLOCATION_FRAMES: usize = 64;

/// Bounded context, never user text or a retained request payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameOperation {
    Present,
    CaptureUi,
    ViewerSnapshot,
    NodeCapture,
    Render,
    Export,
    ExportJob,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationFrameOutcome {
    Rendered,
    Skipped,
    Failed,
    Cancelled,
    Panicked,
    Abandoned,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationFrameInfo {
    pub id: u64,
    pub request: u64,
    pub operation: FrameOperation,
    /// Authored composition frame, including fractional PNG captures; not a GPU timestamp.
    pub source_frame: Option<f64>,
    pub budget_at_start_bytes: u64,
    pub start_admitted_bytes: u64,
    pub peak_admitted_bytes: u64,
    /// Independent category intervals. Their maxima must not be summed into a
    /// simultaneous frame peak; only peak_admitted_bytes has that meaning.
    pub categories: Vec<AllocationCategoryFrameInfo>,
    /// None while CPU work or a registered queue completion remains outstanding.
    pub end_admitted_bytes: Option<u64>,
    pub elapsed_us: u64,
    pub cpu_elapsed_us: Option<u64>,
    pub outcome: Option<AllocationFrameOutcome>,
    pub submissions: u64,
    pub pending_submissions: u64,
    /// A callback was dropped or submission unwound without confirmed completion.
    pub unconfirmed_submissions: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationFramePeaks {
    pub version: u32,
    pub scope: &'static str,
    pub complete_coverage: bool,
    pub is_physical_vram: bool,
    pub admitted_bytes: u64,
    pub max_active_frames: usize,
    pub max_recent_frames: usize,
    /// Includes refused observation windows, never refused native allocations.
    pub omitted_frames: u64,
    pub tracking_fault: bool,
    pub unconfirmed_submissions: u64,
    pub active: Vec<AllocationFrameInfo>,
    /// Ordered by observed completion, not request order.
    pub recent: Vec<AllocationFrameInfo>,
}

struct Active {
    info: AllocationFrameInfo,
    began: Instant,
}

#[derive(Default)]
pub(super) struct Frames {
    next: u64,
    active: BTreeMap<u64, Active>,
    recent: VecDeque<AllocationFrameInfo>,
    omitted: u64,
    fault: bool,
    unconfirmed: u64,
}

fn micros(began: Instant) -> u64 {
    began.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

impl Frames {
    pub(super) fn observe(&mut self, used: u64, category: AllocationCategory, category_used: u64) {
        for frame in self.active.values_mut() {
            frame.info.peak_admitted_bytes = frame.info.peak_admitted_bytes.max(used);
            let bucket = &mut frame.info.categories[category as usize];
            bucket.peak_admitted_bytes = bucket.peak_admitted_bytes.max(category_used);
        }
    }
    fn retire(&mut self, id: u64, used: u64, categories: &[u64; CATEGORY_COUNT]) {
        let Some(frame) = self.active.get(&id) else {
            self.fault = true;
            return;
        };
        if frame.info.outcome.is_none() || frame.info.pending_submissions != 0 {
            return;
        }
        let Some(mut frame) = self.active.remove(&id) else {
            self.fault = true;
            return;
        };
        frame.info.end_admitted_bytes = Some(used);
        for category in &mut frame.info.categories {
            category.end_admitted_bytes = Some(categories[category.category as usize]);
        }
        frame.info.elapsed_us = micros(frame.began);
        if self.recent.len() == MAX_RECENT_ALLOCATION_FRAMES {
            self.recent.pop_front();
        }
        self.recent.push_back(frame.info);
    }
}

impl AllocationBudget {
    pub(crate) fn gpu_command_contexts(&self) -> Vec<crate::gpu_commands::CommandContext> {
        let state = cleanup_lock(&self.0);
        state.frames.active.values().filter(|frame| frame.info.outcome.is_none()).map(|frame| {
            crate::gpu_commands::CommandContext {
                allocation_frame: frame.info.id, request: Some(frame.info.request),
                operation: frame.info.operation, source_frame: frame.info.source_frame,
                exposure_request: None, exposure_poll: None,
            }
        }).collect()
    }
    /// Start before entering the frame handler. Saturation skips observation
    /// explicitly without refusing or changing the actual frame operation.
    pub fn begin_frame(
        &self,
        request: u64,
        operation: FrameOperation,
    ) -> Result<Option<AllocationFrame>, RenderError> {
        self.begin_frame_inner(request, operation, None)
    }

    /// Observe an actual composition frame without retaining project payloads.
    pub fn begin_frame_at(
        &self,
        request: u64,
        operation: FrameOperation,
        source_frame: f64,
    ) -> Result<Option<AllocationFrame>, RenderError> {
        if !source_frame.is_finite() {
            return Err(invalid(
                "allocation observation source frame must be finite",
            ));
        }
        self.begin_frame_inner(request, operation, Some(source_frame))
    }

    fn begin_frame_inner(
        &self,
        request: u64,
        operation: FrameOperation,
        source_frame: Option<f64>,
    ) -> Result<Option<AllocationFrame>, RenderError> {
        let mut state = self.lock()?;
        if state.frames.active.len() == MAX_ACTIVE_ALLOCATION_FRAMES {
            state.frames.omitted = state.frames.omitted.saturating_add(1);
            return Ok(None);
        }
        let id = state
            .frames
            .next
            .checked_add(1)
            .ok_or_else(|| invalid("render allocation frame identity exhausted"))?;
        let info = AllocationFrameInfo {
            id,
            request,
            operation,
            source_frame,
            budget_at_start_bytes: state.limit,
            start_admitted_bytes: state.used,
            peak_admitted_bytes: state.used,
            categories: CATEGORIES
                .into_iter()
                .map(|category| AllocationCategoryFrameInfo {
                    category,
                    start_admitted_bytes: state.categories[category as usize],
                    peak_admitted_bytes: state.categories[category as usize],
                    end_admitted_bytes: None,
                })
                .collect(),
            end_admitted_bytes: None,
            elapsed_us: 0,
            cpu_elapsed_us: None,
            outcome: None,
            submissions: 0,
            pending_submissions: 0,
            unconfirmed_submissions: 0,
        };
        state.frames.next = id;
        state.frames.active.insert(
            id,
            Active {
                info,
                began: Instant::now(),
            },
        );
        Ok(Some(AllocationFrame {
            state: self.0.clone(),
            id,
        }))
    }

    /// Read without entering the serial worker or polling a GPU. Current,
    /// overlapping and recently completed windows are bounded independently.
    pub fn frame_peaks(&self) -> Result<AllocationFramePeaks, RenderError> {
        let state = self.lock()?;
        Ok(AllocationFramePeaks {
            version: 2,
            scope: "session_enrolled_native_capacity",
            complete_coverage: false,
            is_physical_vram: false,
            admitted_bytes: state.used,
            max_active_frames: MAX_ACTIVE_ALLOCATION_FRAMES,
            max_recent_frames: MAX_RECENT_ALLOCATION_FRAMES,
            omitted_frames: state.frames.omitted,
            tracking_fault: state.frames.fault,
            unconfirmed_submissions: state.frames.unconfirmed,
            active: state
                .frames
                .active
                .values()
                .map(|frame| {
                    let mut info = frame.info.clone();
                    info.elapsed_us = micros(frame.began);
                    info
                })
                .collect(),
            recent: state.frames.recent.iter().cloned().collect(),
        })
    }

    pub(crate) fn frame_submission(&self) -> FrameSubmission {
        let mut state = cleanup_lock(&self.0);
        let ids = state
            .frames
            .active
            .iter_mut()
            .filter_map(|(id, frame)| {
                // A later unrelated submission must not extend a CPU-ended frame.
                if frame.info.outcome.is_some() {
                    return None;
                }
                frame.info.submissions = frame.info.submissions.saturating_add(1);
                frame.info.pending_submissions = frame.info.pending_submissions.saturating_add(1);
                Some(*id)
            })
            .collect();
        FrameSubmission {
            state: self.0.clone(),
            ids,
            confirmed: false,
        }
    }
}

// RAII cleanup cannot return an error. Preserve known records after poison,
// expose the fault, and leave normal queries/admission failing closed. No logger
// is invoked: completion can run under a host Session lock during device.poll.
fn cleanup_lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    match state.lock() {
        Ok(state) => state,
        Err(error) => {
            let mut state = error.into_inner();
            state.frames.fault = true;
            state
        }
    }
}

/// Finishing the CPU scope does not finish outstanding registered GPU work.
/// An unfinished scope dropped during unwind is marked Panicked, otherwise
/// Abandoned. This owns metadata only; it does not prolong resource leases.
pub struct AllocationFrame {
    state: Arc<Mutex<State>>,
    id: u64,
}
impl AllocationFrame {
    pub fn finish(self, outcome: AllocationFrameOutcome) {
        self.end(outcome);
    }
    fn end(&self, outcome: AllocationFrameOutcome) {
        let mut state = cleanup_lock(&self.state);
        if let Some(frame) = state.frames.active.get_mut(&self.id) {
            if frame.info.outcome.is_some() {
                return;
            }
            frame.info.outcome = Some(outcome);
            frame.info.cpu_elapsed_us = Some(micros(frame.began));
            let used = state.used;
            let categories = state.categories;
            state.frames.retire(self.id, used, &categories);
        }
    }
}
impl Drop for AllocationFrame {
    fn drop(&mut self) {
        self.end(if std::thread::panicking() {
            AllocationFrameOutcome::Panicked
        } else {
            AllocationFrameOutcome::Abandoned
        });
    }
}

pub(crate) struct FrameSubmission {
    state: Arc<Mutex<State>>,
    ids: Vec<u64>,
    confirmed: bool,
}
impl FrameSubmission {
    pub(crate) fn is_tracked(&self) -> bool {
        !self.ids.is_empty()
    }
    pub(crate) fn complete(mut self) {
        self.confirmed = true;
    }
}
impl Drop for FrameSubmission {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        let mut state = cleanup_lock(&self.state);
        if !self.confirmed {
            state.frames.unconfirmed = state.frames.unconfirmed.saturating_add(1);
        }
        let used = state.used;
        let categories = state.categories;
        for id in &self.ids {
            let Some(frame) = state.frames.active.get_mut(id) else {
                state.frames.fault = true;
                continue;
            };
            let Some(pending) = frame.info.pending_submissions.checked_sub(1) else {
                state.frames.fault = true;
                continue;
            };
            frame.info.pending_submissions = pending;
            if !self.confirmed {
                frame.info.unconfirmed_submissions =
                    frame.info.unconfirmed_submissions.saturating_add(1);
            }
            state.frames.retire(*id, used, &categories);
        }
    }
}

#[cfg(test)]
mod tests;
