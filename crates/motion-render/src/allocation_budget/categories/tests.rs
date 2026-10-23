use super::*;
use crate::{AllocationBudget, AllocationFrameOutcome as Outcome, FrameOperation};

#[test]
fn independent_category_peaks_are_not_a_simultaneous_total_and_refusal_is_atomic() {
    let budget = AllocationBudget::new(16).unwrap();
    let frame = budget
        .begin_frame(1, FrameOperation::Render)
        .unwrap()
        .unwrap();
    let image = budget
        .reserve_in(8, AllocationCategory::StaticImages)
        .unwrap();
    drop(image);
    let video = budget.reserve_in(12, AllocationCategory::Video).unwrap();
    let before = budget.info().unwrap();
    assert!(budget.reserve_in(8, AllocationCategory::Masks).is_err());
    assert_eq!(budget.info().unwrap(), before);
    assert_eq!(
        before
            .categories
            .iter()
            .map(|c| c.admitted_bytes)
            .sum::<u64>(),
        12
    );
    assert_eq!(
        before
            .categories
            .iter()
            .map(|c| c.peak_admitted_bytes)
            .sum::<u64>(),
        20
    );
    assert_eq!(before.peak_admitted_bytes, 12);
    drop(video);
    frame.finish(Outcome::Rendered);
    let snapshot = budget.frame_peaks().unwrap();
    let frame = &snapshot.recent[0];
    assert_eq!(frame.peak_admitted_bytes, 12);
    assert_eq!(
        frame
            .categories
            .iter()
            .map(|c| c.peak_admitted_bytes)
            .sum::<u64>(),
        20
    );
    assert!(frame
        .categories
        .iter()
        .all(|c| c.end_admitted_bytes == Some(0)));
    let next = budget
        .begin_frame(2, FrameOperation::Render)
        .unwrap()
        .unwrap();
    next.finish(Outcome::Skipped);
    assert!(budget.frame_peaks().unwrap().recent[1]
        .categories
        .iter()
        .all(|c| c.peak_admitted_bytes == 0));
    let wire = serde_json::to_value(budget.frame_peaks().unwrap()).unwrap();
    assert_eq!(
        wire["recent"][0]["categories"][0]["category"],
        "static_images"
    );
}

#[test]
fn alias_and_queue_retention_share_one_category_until_all_frame_callbacks_retire() {
    let budget = AllocationBudget::new(16).unwrap();
    let image = budget
        .reserve_in(8, AllocationCategory::StaticImages)
        .unwrap();
    let first = budget
        .begin_frame(1, FrameOperation::Present)
        .unwrap()
        .unwrap();
    let alias = image.clone();
    let retained = image.retention();
    let uniform = budget
        .reserve_in(4, AllocationCategory::UniformBuffers)
        .unwrap();
    let second = budget
        .begin_frame(2, FrameOperation::CaptureUi)
        .unwrap()
        .unwrap();
    let submission = budget.frame_submission();
    drop((image, alias));
    first.finish(Outcome::Rendered);
    second.finish(Outcome::Rendered);
    let pending = budget.frame_peaks().unwrap();
    assert_eq!(pending.active.len(), 2);
    assert!(pending
        .active
        .iter()
        .flat_map(|f| &f.categories)
        .all(|c| c.end_admitted_bytes.is_none()));
    assert_eq!(budget.info().unwrap().admitted_bytes, 12);
    drop((uniform, retained));
    submission.complete();
    let completed = budget.frame_peaks().unwrap();
    assert!(completed.active.is_empty());
    assert_eq!(completed.recent.len(), 2);
    for frame in &completed.recent {
        assert_eq!(
            frame.categories[AllocationCategory::StaticImages as usize].peak_admitted_bytes,
            8
        );
        assert_eq!(
            frame.categories[AllocationCategory::UniformBuffers as usize].peak_admitted_bytes,
            4
        );
        assert!(frame
            .categories
            .iter()
            .all(|c| c.end_admitted_bytes == Some(0)));
    }
    assert_eq!(
        budget
            .info()
            .unwrap()
            .categories
            .iter()
            .map(|c| c.admitted_bytes)
            .sum::<u64>(),
        0
    );
}

#[test]
fn bucket_indices_and_descriptor_roles_are_bounded_and_category_drift_fails_closed() {
    for (index, category) in CATEGORIES.into_iter().enumerate() {
        assert_eq!(category as usize, index);
    }
    assert_eq!(
        AllocationCategory::buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST),
        AllocationCategory::ReadbackBuffers
    );
    assert_eq!(
        AllocationCategory::buffer(wgpu::BufferUsages::MAP_WRITE | wgpu::BufferUsages::COPY_SRC),
        AllocationCategory::UploadBuffers
    );
    assert_eq!(
        AllocationCategory::buffer(wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST),
        AllocationCategory::UniformBuffers
    );
    let budget = AllocationBudget::new(8).unwrap();
    let image = budget
        .reserve_in(8, AllocationCategory::StaticImages)
        .unwrap();
    budget.0.lock().unwrap().categories[AllocationCategory::StaticImages as usize] = 0;
    drop(image);
    assert!(budget.info().is_err());
    assert!(budget.reserve_in(1, AllocationCategory::Masks).is_err());
    let state = budget.0.lock().unwrap();
    assert!(state.release_fault_seen);
    assert_eq!(
        state.used, 8,
        "do not silently erase known total capacity after category underflow"
    );
}
