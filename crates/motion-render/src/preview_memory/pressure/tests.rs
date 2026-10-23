use super::*;
use crate::preview_memory::PreviewMemoryPolicy;

fn reading(usage: u64) -> MemoryPressureSnapshot {
    let mut value = MemoryPressureSnapshot::without_renderer();
    value.backend = "Vulkan".into();
    value.reading = MemoryPressureReading::Available {
        source: MemoryPressureSource::VulkanDeviceLocalHeaps,
        pools: vec![MemoryPressurePool {
            index: 3,
            budget_bytes: 100,
            usage_bytes: usage,
        }],
    };
    value
}
fn poll(policy: &mut PreviewMemoryPolicy, second: u64, usage: u64) -> PreviewPressureUpdate {
    policy.observe_pressure(Duration::from_secs(second), || reading(usage))
}

#[test]
fn repeated_frame_polling_cannot_recount_one_native_probe_cache_entry() {
    let mut policy = PreviewMemoryPolicy::default();
    let first = poll(&mut policy, 0, 95);
    assert_eq!(first.action, PreviewPressureAction::EvictIdle);
    assert!(first.sampled && first.first_high && !first.persistent_high);
    for millisecond in 1..1_000 {
        let next = policy.observe_pressure(Duration::from_millis(millisecond), || {
            panic!("a healthy per-frame poll must not query native memory again")
        });
        assert!(!next.sampled);
        assert_eq!(next.action, PreviewPressureAction::None);
    }
    let boundary = policy.observe_pressure(Duration::from_nanos(999_999_999), || {
        panic!("do not round across the native cache deadline")
    });
    assert!(!boundary.sampled);
    assert_eq!(policy.quality(true).level, 0);
    let next = poll(&mut policy, 1, 95);
    assert_eq!(
        next.action,
        PreviewPressureAction::Quality {
            previous: 0,
            level: 1
        }
    );
    assert_eq!(policy.quality(true).level, 1);
}

#[test]
fn eviction_precedes_strictly_bounded_quality_steps_and_sixty_is_reported_once() {
    let mut policy = PreviewMemoryPolicy::default();
    let mut actions = Vec::new();
    let mut first = 0;
    let mut errors = 0;
    for second in 0..120 {
        let update = poll(&mut policy, second, 95);
        first += usize::from(update.first_high);
        errors += usize::from(update.persistent_high);
        if update.action != PreviewPressureAction::None {
            actions.push((second, update.action));
        }
    }
    assert_eq!((first, errors), (1, 1));
    assert_eq!(
        actions,
        [
            (0, PreviewPressureAction::EvictIdle),
            (
                1,
                PreviewPressureAction::Quality {
                    previous: 0,
                    level: 1
                }
            ),
            (
                6,
                PreviewPressureAction::Quality {
                    previous: 1,
                    level: 2
                }
            ),
            (
                11,
                PreviewPressureAction::Quality {
                    previous: 2,
                    level: 3
                }
            ),
        ]
    );
    assert_eq!(policy.quality(true).target_divisor, 8);
    assert_eq!(
        policy
            .pressure_info(Duration::from_secs(119))
            .consecutive_high_samples,
        60
    );
    assert_eq!(
        policy.refusal_count, 0,
        "advisory pressure is not an allocation rejection"
    );
    assert!(policy.last_rejection.is_none());
    assert!(!policy.blocked());
    assert_eq!(
        poll(&mut policy, 120, 50).action,
        PreviewPressureAction::None
    );
    let restarted = poll(&mut policy, 121, 95);
    assert!(restarted.first_high && !restarted.persistent_high);
    assert_eq!(restarted.action, PreviewPressureAction::EvictIdle);
}

#[test]
fn recovery_requires_low_samples_completed_frames_and_separate_thirty_second_cooldowns() {
    let mut policy = PreviewMemoryPolicy::default();
    for second in 0..=6 {
        poll(&mut policy, second, 95);
    }
    assert_eq!(policy.quality(true).level, 2);
    for _ in 0..120 {
        policy.succeeded();
    }
    for second in 7..36 {
        assert_eq!(
            poll(&mut policy, second, 50).action,
            PreviewPressureAction::None
        );
    }
    assert_eq!(
        poll(&mut policy, 36, 50).action,
        PreviewPressureAction::Quality {
            previous: 2,
            level: 1
        }
    );
    for second in 37..=66 {
        assert_eq!(
            poll(&mut policy, second, 50).action,
            PreviewPressureAction::None
        );
    }
    for _ in 0..119 {
        policy.succeeded();
    }
    assert_eq!(
        poll(&mut policy, 67, 50).action,
        PreviewPressureAction::None
    );
    policy.succeeded();
    assert_eq!(
        poll(&mut policy, 68, 50).action,
        PreviewPressureAction::Quality {
            previous: 1,
            level: 0
        }
    );
    assert_eq!(policy.quality(true).image_edge, None);
    assert_eq!(policy.quality(true).video_edge, None);
    assert_eq!(policy.quality(true).target_divisor, 1);
}

#[test]
fn unknown_and_intermediate_readings_hold_quality_and_reset_consecutive_episodes() {
    let mut policy = PreviewMemoryPolicy::default();
    poll(&mut policy, 0, 95);
    poll(&mut policy, 1, 95);
    let unknown = policy.observe_pressure(
        Duration::from_secs(2),
        MemoryPressureSnapshot::without_renderer,
    );
    assert!(unknown.sampled && !unknown.first_high && !unknown.persistent_high);
    assert_eq!(unknown.action, PreviewPressureAction::None);
    let info = policy.pressure_info(Duration::from_secs(2));
    assert_eq!(info.status, PreviewPressureStatus::Unknown);
    assert_eq!(
        info.unavailable,
        Some(MemoryPressureUnavailable::NoActiveRenderer)
    );
    assert!(info.worst_pool.is_none());
    assert_eq!(info.consecutive_high_samples, 0);
    for _ in 0..120 {
        policy.succeeded();
    }
    poll(&mut policy, 100, 80);
    assert_eq!(policy.quality(true).level, 1);
    assert_eq!(
        policy
            .pressure_info(Duration::from_secs(100))
            .recovery_samples,
        0
    );
    let high = poll(&mut policy, 101, 95);
    assert!(high.first_high);
    assert_eq!(high.action, PreviewPressureAction::EvictIdle);
    assert_eq!(policy.quality(true).level, 1);
}

#[test]
fn independent_pool_ratios_do_not_sum_free_vram_or_overflow_large_native_values() {
    let mut policy = PreviewMemoryPolicy::default();
    let mut sample = reading(50);
    sample.reading = MemoryPressureReading::Available {
        source: MemoryPressureSource::VulkanDeviceLocalHeaps,
        pools: vec![
            MemoryPressurePool {
                index: 1,
                budget_bytes: u64::MAX,
                usage_bytes: u64::MAX - 1,
            },
            MemoryPressurePool {
                index: 2,
                budget_bytes: 10,
                usage_bytes: 1,
            },
        ],
    };
    assert_eq!(
        policy.observe_pressure(Duration::ZERO, || sample).action,
        PreviewPressureAction::EvictIdle
    );
    assert_eq!(
        policy
            .pressure_info(Duration::ZERO)
            .worst_pool
            .unwrap()
            .index,
        1
    );
    let mut invalid = reading(5);
    if let MemoryPressureReading::Available { pools, .. } = &mut invalid.reading {
        pools[0].budget_bytes = 0;
    }
    let update = policy.observe_pressure(Duration::from_secs(1), || invalid);
    assert_eq!(update.action, PreviewPressureAction::None);
    assert_eq!(
        policy.pressure_info(Duration::from_secs(1)).unavailable,
        Some(MemoryPressureUnavailable::InvalidNativeResult)
    );
    assert_eq!(policy.quality(true).level, 0);
    assert_eq!(policy.refusal_count, 0);
}

#[test]
fn typed_refusals_start_above_the_native_floor_and_keep_existing_bounded_recovery() {
    let mut policy = PreviewMemoryPolicy::default();
    for second in 0..=6 {
        poll(&mut policy, second, 95);
    }
    assert_eq!(policy.quality(true).level, 2);
    let rejection = crate::ResourceRejection::Budget {
        scope: crate::ResourceBudgetScope::SourceTextures,
        required_bytes: 129 << 20,
        budget_bytes: 128 << 20,
    };
    assert!(policy.begin(6_000));
    assert!(policy.refused(rejection, 6_000));
    assert_eq!(policy.quality(true).level, 3);
    assert!(!policy.refused(rejection, 6_000));
    assert_eq!(policy.refusal_count, 2);
    assert_eq!(policy.last_rejection, Some(rejection));
    assert!(policy.blocked());
    assert!(!policy.begin(35_999));
    assert!(policy.begin(36_000));
    assert_eq!(policy.quality(true).level, 3);
    for _ in 0..120 {
        policy.succeeded();
    }
    assert!(policy.begin(36_000));
    assert_eq!(policy.quality(true).level, 2);
    assert!(policy.refused(rejection, 36_000));
    assert_eq!(
        policy.quality(true).level,
        3,
        "failed recovery restores the known working quality"
    );
    assert_eq!(policy.last_rejection, Some(rejection));
}
