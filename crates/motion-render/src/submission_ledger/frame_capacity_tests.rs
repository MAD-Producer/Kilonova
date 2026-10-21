use super::*;
use crate::{FrameTextureCapacity, Renderer, TextureCategory, TextureState};

#[test]
fn replaced_target_is_counted_once_across_owners_queues_and_completions() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let old = renderer.render_target(4, 4).unwrap();
    let mut submitted = renderer.texture_resource_ledger();
    old.collect_texture_resources(&mut submitted, "old", TextureState::PreviewFront);
    let preview = SubmissionLedger::default();
    let editor = SubmissionLedger::default();
    preview.set_enabled(true);
    editor.set_enabled(true);
    let first = preview.begin(submitted.clone());
    let second = editor.begin(submitted.clone());
    let replacement = renderer.render_target(4, 4).unwrap();
    let mut current = renderer.texture_resource_ledger();
    replacement.collect_texture_resources(&mut current, "new", TextureState::PreviewCandidate);
    let owned_bytes = current.total_known_bytes();
    drop(old);
    let snapshots = [preview.snapshot(&current), editor.snapshot(&current)];
    let combined = FrameTextureCapacity::observe(&current, &snapshots);
    assert_eq!(combined.known_owned_bytes, owned_bytes);
    assert_eq!(combined.known_retired_bytes, 64);
    assert_eq!(combined.known_combined_bytes, owned_bytes + 64);
    assert_eq!(combined.unknown_capacity_count, 0);
    assert_eq!(combined.active_renderers, 2);
    assert!(combined.all_active_renderers_tracked);
    assert_eq!(combined.pending_untracked, 0);
    // Adding per-renderer totals would double-count this same retired target.
    assert_eq!(snapshots.iter().map(|s| s.info.known_retired_bytes).sum::<u64>(), 128);
    SubmissionLedger::complete(&preview.state, first);
    let snapshots = [preview.snapshot(&current), editor.snapshot(&current)];
    assert_eq!(FrameTextureCapacity::observe(&current, &snapshots).known_retired_bytes, 64);
    SubmissionLedger::complete(&editor.state, second);
    let snapshots = [preview.snapshot(&current), editor.snapshot(&current)];
    let recovered = FrameTextureCapacity::observe(&current, &snapshots);
    assert_eq!(recovered.known_retired_bytes, 0);
    assert_eq!(recovered.known_combined_bytes, owned_bytes);
    // Metadata is immutable and stays usable after subsequent completions.
    assert_eq!(combined.known_combined_bytes, owned_bytes + 64);
    assert_eq!(submitted.total_known_bytes(), renderer.texture_resource_ledger().total_known_bytes() + 64);
    renderer.check_health().unwrap();
}

#[test]
fn disabled_tracking_and_bounded_snapshot_gaps_remain_explicit() {
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let target = renderer.render_target(4, 4).unwrap();
    let mut resources = TextureResourceLedger::default();
    target.collect_texture_resources(&mut resources, "target", TextureState::Retained);
    let tracker = SubmissionLedger::default();
    tracker.set_enabled(true);
    let tickets = (0..MAX_SUBMISSION_SNAPSHOTS + 3)
        .map(|_| tracker.begin(resources.clone())).collect::<Vec<_>>();
    tracker.set_enabled(false);
    let current = TextureResourceLedger::default();
    let snapshot = tracker.snapshot(&current);
    let combined = FrameTextureCapacity::observe(&current, [&snapshot]);
    assert!(!combined.all_active_renderers_tracked);
    assert_eq!(combined.pending_untracked, 3);
    assert_eq!(combined.known_retired_bytes, 64);
    assert_eq!(snapshot.pending_resources.resources().count(), 1);
    assert_eq!(snapshot.info.skipped_snapshots, 3);
    for ticket in tickets { SubmissionLedger::complete(&tracker.state, ticket); }
    let snapshot = tracker.snapshot(&current);
    let combined = FrameTextureCapacity::observe(&current, [&snapshot]);
    assert_eq!(combined.known_combined_bytes, 0);
    assert_eq!(combined.pending_untracked, 0);
    assert_eq!(combined.active_renderers, 1);
}

#[test]
fn unknown_depth_capacity_is_deduplicated_without_inventing_bytes() {
    use crate::resource_ledger::DeviceTextureExt;
    let renderer = pollster::block_on(Renderer::headless()).unwrap();
    let depth = renderer.device.create_owned_texture(&wgpu::TextureDescriptor {
        label: Some("unknown-capacity-depth"),
        size: wgpu::Extent3d { width: 4, height: 4, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth24Plus,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut owned = TextureResourceLedger::default();
    owned.retained(&depth, TextureCategory::SystemTextures, "depth".into());
    assert_eq!(owned.unknown_capacity_count(), 1);
    let tracker = SubmissionLedger::default();
    tracker.set_enabled(true);
    let ticket = tracker.begin(owned.clone());
    let snapshot = tracker.snapshot(&owned);
    let combined = FrameTextureCapacity::observe(&owned, [&snapshot]);
    assert_eq!(combined.known_owned_bytes, 0);
    assert_eq!(combined.known_combined_bytes, 0);
    assert_eq!(combined.unknown_capacity_count, 1);
    SubmissionLedger::complete(&tracker.state, ticket);
    renderer.check_health().unwrap();
}

#[test]
fn no_renderer_is_not_reported_as_complete_tracking() {
    let capacity = FrameTextureCapacity::observe(&Default::default(), []);
    assert_eq!(capacity.active_renderers, 0);
    assert!(!capacity.all_active_renderers_tracked);
    assert_eq!(capacity.known_combined_bytes, 0);
}
