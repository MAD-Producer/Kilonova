//! Best-effort metadata around independent exports, outside Session locks.
use super::Result;
use motion_render::{AllocationBudget, AllocationFrameOutcome, FrameOperation};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

/// One bounded site across PNG, movie jobs and movie frames. Omission episodes
/// reset on a successful window, faults on a successful begin result. Counters
/// never change admission, return values, cancellation or publication.
pub(super) fn run<T>(
    budget: &AllocationBudget,
    request: u64,
    operation: FrameOperation,
    source_frame: Option<f64>,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    static OMITTED: motion_render::observability::FailureCounter =
        motion_render::observability::FailureCounter::new();
    static FAULT: AtomicBool = AtomicBool::new(false);
    let began = Instant::now();
    let observed = match source_frame {
        Some(frame) => budget.begin_frame_at(request, operation, frame),
        None => budget.begin_frame(request, operation),
    };
    let frame = match observed {
        Ok(frame) => {
            FAULT.store(false, Ordering::Relaxed);
            if frame.is_some() {
                OMITTED.success();
            } else {
                OMITTED.failure("export.frame_allocation.observation", format_args!(
                    "request={request} operation={operation:?} source_frame={source_frame:?} phase=begin elapsed_ms={} active_limit={}",
                    began.elapsed().as_millis(), motion_render::MAX_ACTIVE_ALLOCATION_FRAMES,
                ));
            }
            frame
        }
        Err(error) => {
            if !FAULT.swap(true, Ordering::Relaxed) {
                log::error!("export allocation observation failed request={request} operation={operation:?} source_frame={source_frame:?} phase=begin elapsed_ms={} cause={error}", began.elapsed().as_millis());
            }
            None
        }
    };
    // An unwind drops the scope as Panicked; preserve the original panic.
    let result = work();
    if let Some(frame) = frame {
        frame.finish(match &result {
            Ok(_) => AllocationFrameOutcome::Rendered,
            Err(error)
                if matches!(
                    error.as_str(),
                    "movie export cancelled" | "PNG export cancelled"
                ) =>
            {
                AllocationFrameOutcome::Cancelled
            }
            Err(_) => AllocationFrameOutcome::Failed,
        });
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn result_cancellation_and_unwind_preserve_the_original_operation() {
        let budget =
            AllocationBudget::new(motion_render::FIXED_RENDERER_ALLOCATION_BYTES + 64).unwrap();
        let renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let occupied = budget.info().unwrap().admitted_bytes;
        assert_eq!(
            run(&budget, 1, FrameOperation::Export, Some(1.25), || {
                let _native = renderer
                    .allocation_context()
                    .buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("observed export helper buffer"),
                        contents: &[0; 16],
                        usage: wgpu::BufferUsages::COPY_DST,
                    })
                    .unwrap();
                Ok(17)
            })
            .unwrap(),
            17
        );
        let failure: Result<()> = run(&budget, 2, FrameOperation::ExportJob, None, || {
            Err("primary codec failure".into())
        });
        assert_eq!(failure.unwrap_err(), "primary codec failure");
        let cancelled: Result<()> = run(&budget, 3, FrameOperation::Export, Some(2.), || {
            Err("movie export cancelled".into())
        });
        assert_eq!(cancelled.unwrap_err(), "movie export cancelled");
        assert!(std::panic::catch_unwind(|| {
            let _: Result<()> = run(&budget, 4, FrameOperation::Export, Some(3.), || {
                panic!("controlled export panic")
            });
        })
        .is_err());
        let peaks = budget.frame_peaks().unwrap();
        assert!(peaks.active.is_empty());
        assert_eq!(
            peaks
                .recent
                .iter()
                .map(|w| w.outcome.unwrap())
                .collect::<Vec<_>>(),
            [
                AllocationFrameOutcome::Rendered,
                AllocationFrameOutcome::Failed,
                AllocationFrameOutcome::Cancelled,
                AllocationFrameOutcome::Panicked,
            ]
        );
        assert_eq!(peaks.recent[0].peak_admitted_bytes, occupied + 16);
        assert_eq!(peaks.recent[0].source_frame, Some(1.25));
        assert_eq!(peaks.recent[0].end_admitted_bytes, Some(occupied));
    }

    #[test]
    #[cfg_attr(not(feature = "gpu-tests"), ignore = "requires an actual GPU")]
    fn missing_or_invalid_observation_preserves_primary_failure_and_native_admission() {
        let budget =
            AllocationBudget::new(motion_render::FIXED_RENDERER_ALLOCATION_BYTES + 64).unwrap();
        let renderer = pollster::block_on(
            motion_render::Renderer::headless_with_allocation_budget(budget.clone()),
        )
        .unwrap();
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        let occupied = budget.info().unwrap().admitted_bytes;
        let mut active: Vec<_> = (0..motion_render::MAX_ACTIVE_ALLOCATION_FRAMES)
            .map(|id| {
                budget
                    .begin_frame(id as u64, FrameOperation::Render)
                    .unwrap()
                    .unwrap()
            })
            .collect();
        let failure: Result<()> = run(&budget, 80, FrameOperation::Export, Some(0.), || {
            let _native = renderer
                .allocation_context()
                .buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("saturated observation still admits native capacity"),
                    contents: &[0; 64],
                    usage: wgpu::BufferUsages::COPY_DST,
                })
                .unwrap();
            Err("primary source failure".into())
        });
        assert_eq!(failure.unwrap_err(), "primary source failure");
        assert_eq!(budget.frame_peaks().unwrap().omitted_frames, 1);
        drop(active.pop());
        assert_eq!(
            run(&budget, 81, FrameOperation::Export, Some(0.), || Ok(9)).unwrap(),
            9
        );
        let failure: Result<()> = run(&budget, 82, FrameOperation::Export, Some(f64::NAN), || {
            Err("original cancellation gate failure".into())
        });
        assert_eq!(failure.unwrap_err(), "original cancellation gate failure");
        let peaks = budget.frame_peaks().unwrap();
        assert_eq!(peaks.recent.last().unwrap().request, 81);
        assert_eq!(budget.info().unwrap().admitted_bytes, occupied);
        drop(active);
    }
}
