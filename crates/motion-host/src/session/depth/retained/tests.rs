use super::super::Source;
use super::*;
use crate::depth::{Costs, Key};
use motion_model::depth::*;

fn entry(asset: u64) -> Entry {
    let descriptor = DepthDescriptor {
        size: [2, 1],
        source: DepthSource {
            asset,
            revision: 3,
            pts: DepthTime {
                ticks: 0,
                timescale: 1,
            },
        },
        geometry: DepthGeometry {
            source_size: [2, 1],
            crop: [0, 0, 2, 1],
            quarter_turns: 0,
            mirror_x: false,
        },
        model: DepthModel {
            id: "owned-retention".into(),
            revision: "1".into(),
            sha256: [3; 32],
            processing: "owned".into(),
        },
        measure: DepthMeasure::Relative {
            near: NearDirection::Increasing,
        },
        scale: 1.,
        offset: 0.,
        valid_range: None,
    };
    let key = Key {
        source: descriptor.source.clone(),
        geometry: descriptor.geometry.clone(),
        model: descriptor.model.clone(),
    };
    Entry {
        binding: Binding {
            request: asset,
            key: Identity::Sample(key),
            document: 1,
            revision: 3,
            frame: 0.,
            composition: "main".into(),
            root: "owned".into(),
            decoded_budget: 4096,
            source: Source::Image(motion_core::Asset {
                id: asset,
                path: format!("assets/{asset}.png"),
                width: 2,
                height: 1,
            }),
        },
        estimate: Arc::new(Estimate {
            resource: DepthResource::new(
                descriptor,
                vec![1., f32::NAN],
                Some(vec![1, 0]),
                Some(vec![0.25, 0.5]),
            )
            .unwrap(),
            provider: "owned".into(),
            costs: Costs::default(),
        }),
        image: None,
        video_end_us: None,
        input: PreparedDepthInput {
            rgb_size: [2, 1],
            rgb_sha256: [asset as u8; 32],
        },
    }
}

fn video_entry(start: u64, object: u64) -> Entry {
    let mut sample = entry(74);
    let mut descriptor = sample.estimate.resource.descriptor().clone();
    descriptor.source.pts = DepthTime { ticks: start as i64, timescale: 1_000_000 };
    sample.binding.key = Identity::Video(crate::depth::VideoKey {
        object, asset: 74, revision: 3, target_us: start,
        geometry: descriptor.geometry.clone(), model: descriptor.model.clone(),
    });
    sample.binding.source = Source::Video { object, asset: motion_core::VideoAsset {
        id: 74, path: "assets/owned.mp4".into(), bytes: 5, mime: "video/avc".into(), track: 0,
        width: 2, height: 1, rotation: 0, display_width: 2, display_height: 1,
        video_start_us: 0, video_end_us: 150_000, duration_us: 150_000,
        frame_count: 3, variable_frame_rate: true, nominal_frame_rate: 30.,
        color_standard: 1, color_range: 2, audio_asset: None,
    }};
    sample.estimate = Arc::new(Estimate {
        resource: DepthResource::new(descriptor, vec![1., f32::NAN], Some(vec![1, 0]), Some(vec![0.25, 0.5])).unwrap(),
        provider: "owned".into(), costs: Costs::default(),
    });
    sample.image = Some(motion_render::DepthImage::new([2,1], vec![255;8]).unwrap());
    sample.video_end_us = Some(start + 10_000);
    sample
}

#[test]
fn temporal_capacity_evicts_oldest_video_and_preserves_pinned_static_and_other_instances() {
    let mut result = Results::default();
    result.admit(entry(1), 70, 3).unwrap(); // 18 bytes pinned, video entries are 26 each.
    let first = video_entry(10_000, 2);
    let first_weak = Arc::downgrade(&first.estimate);
    result.admit_video(first, 70, 3, |_| false).unwrap();
    let other = video_entry(10_000, 3);
    let other_weak = Arc::downgrade(&other.estimate);
    result.admit_video(other, 70, 3, |_| false).unwrap();
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes), (3, 54, 16));
    result.admit_video(video_entry(40_000, 2), 70, 3, |_| false).unwrap();
    assert!(first_weak.upgrade().is_none());
    assert!(other_weak.upgrade().is_some());
    assert_eq!(result.entries[0].estimate.resource.descriptor().source.asset, 1);
    assert_eq!(result.entries[1].video_interval(), Some((10_000, 20_000)));
    assert_eq!(result.entries[2].video_interval(), Some((40_000, 50_000)));
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes, result.rejections), (3, 54, 16, 0));
    result.clear();
    assert!(other_weak.upgrade().is_none());
}

#[test]
fn temporal_oversize_and_pinned_capacity_refusals_do_not_evict_working_samples() {
    let mut result = Results::default();
    result.admit(entry(1), 100, 4).unwrap();
    result.admit_video(video_entry(10_000, 2), 100, 4, |_| false).unwrap();
    let identities: Vec<_> = result.entries.iter().map(|e| e.estimate.resource.cache_identity()).collect();
    for (bytes, slots) in [(43,4), (100,1)] {
        assert!(matches!(result.admit_video(video_entry(40_000, 2), bytes, slots, |_| false), Err(Error::RetentionCapacity { .. })));
        assert_eq!(result.entries.iter().map(|e| e.estimate.resource.cache_identity()).collect::<Vec<_>>(), identities);
        assert_eq!((result.bytes, result.image_bytes), (36,8));
    }
    assert_eq!(result.rejections, 2);
}

#[test]
fn temporal_cache_refuses_to_evict_another_objects_current_render_sample() {
    let mut result = Results::default();
    result.admit_video(video_entry(10_000, 1), 52, 2, |_| false).unwrap();
    result.admit_video(video_entry(40_000, 2), 52, 2, |_| false).unwrap();
    let identities: Vec<_> = result.entries.iter().map(|e| e.estimate.resource.cache_identity()).collect();
    assert!(matches!(result.admit_video(video_entry(110_000, 3), 52, 2, |_| true),
        Err(Error::RetentionCapacity { required_bytes: 78, required_samples: 3, .. })));
    assert_eq!(result.entries.iter().map(|e| e.estimate.resource.cache_identity()).collect::<Vec<_>>(), identities);
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes, result.rejections), (2,36,16,1));
}

#[test]
fn retained_byte_refusal_preserves_other_sources_and_release_recovers_capacity() {
    let mut results = Results::default();
    let a = entry(1);
    let a_weak = Arc::downgrade(&a.estimate);
    results.admit(a, 36, 8).unwrap();
    let b = entry(2);
    let b_weak = Arc::downgrade(&b.estimate);
    results.admit(b, 36, 8).unwrap();
    assert_eq!(
        results.bytes, 36,
        "both scalar/validity/confidence planes count"
    );
    let c = entry(3);
    let c_weak = Arc::downgrade(&c.estimate);
    let cause = results.admit(c, 36, 8).unwrap_err();
    assert!(matches!(
        cause,
        Error::RetentionCapacity {
            request: 3,
            asset: 3,
            required_bytes: 54,
            byte_limit: 36,
            required_samples: 3,
            sample_limit: 8
        }
    ));
    assert!(cause.to_string().contains("phase=retain"));
    assert_eq!(results.entries.len(), 2);
    assert_eq!(results.bytes, 36);
    assert_eq!(results.rejections, 1);
    assert!(a_weak.upgrade().is_some() && b_weak.upgrade().is_some());
    assert!(
        c_weak.upgrade().is_none(),
        "refused resources cannot become hidden owners"
    );
    results.remove_source(1);
    assert!(a_weak.upgrade().is_none());
    assert_eq!(results.bytes, 18);
    results.admit(entry(3), 36, 8).unwrap();
    assert_eq!(results.bytes, 36);
    results.clear();
    assert!(b_weak.upgrade().is_none());
    assert!(results.entries.is_empty());
    assert_eq!(results.bytes, 0);
    assert_eq!(
        results.rejections, 1,
        "clearing ownership does not erase lifetime failures"
    );
}

#[test]
fn replacement_releases_old_result_and_sample_count_refusal_is_atomic() {
    let mut results = Results::default();
    let original = entry(1);
    let old = Arc::downgrade(&original.estimate);
    results.admit(original, 1024, 1).unwrap();
    let replacement = entry(1);
    let token = replacement.estimate.resource.cache_identity();
    results.admit(replacement, 1024, 1).unwrap();
    assert!(old.upgrade().is_none());
    assert_eq!(results.entries.len(), 1);
    assert_eq!(results.bytes, 18);
    let cause = results.admit(entry(2), 1024, 1).unwrap_err();
    assert!(matches!(
        cause,
        Error::RetentionCapacity {
            required_samples: 2,
            sample_limit: 1,
            ..
        }
    ));
    assert_eq!(results.entries[0].estimate.resource.cache_identity(), token);
    assert_eq!(results.bytes, 18);
}

#[test]
fn result_admission_failure_counter_saturates() {
    let mut results = Results::default();
    results.rejections = u64::MAX;
    assert!(results.admit(entry(1), 0, 0).is_err());
    assert_eq!(results.rejections, u64::MAX);
    assert_eq!(results.bytes, 0);
    assert!(results.entries.is_empty());
}


#[test]
fn original_image_capacity_is_charged_with_planes_and_released_atomically() {
    let mut result = Results::default();
    let mut first = entry(1);
    let mut bytes = Vec::with_capacity(64);
    bytes.extend_from_slice(&[10,20,30,255,40,50,60,255]);
    first.image = Some(motion_render::DepthImage::new([2,1], bytes).unwrap());
    result.admit(first, 82, 2).unwrap();
    assert_eq!((result.bytes, result.image_bytes), (18,64));
    let cause = result.admit(entry(2), 82, 2).unwrap_err();
    assert!(matches!(cause, Error::RetentionCapacity { required_bytes: 100, .. }));
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes), (1,18,64));
    // Replacing the source releases its previous original-image allocation.
    result.admit(entry(1), 82, 2).unwrap();
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes), (1,18,0));
    result.clear();
    assert_eq!((result.bytes, result.image_bytes), (0,0));
    assert_eq!(result.rejections, 1);
}

#[test]
fn invalidating_a_subset_releases_both_owned_images_and_scalar_allocations() {
    let mut result = Results::default();
    for asset in [1,2] {
        let mut sample = entry(asset);
        sample.image = Some(motion_render::DepthImage::new([2,1], vec![255;8]).unwrap());
        result.admit(sample, 52, 2).unwrap();
    }
    let old = Arc::downgrade(&result.entries[0].estimate);
    let current = Arc::downgrade(&result.entries[1].estimate);
    result.retain(|entry| entry.binding.request == 2);
    assert!(old.upgrade().is_none());
    assert!(current.upgrade().is_some());
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes), (1,18,8));
    result.retain(|_| false);
    assert!(current.upgrade().is_none());
    assert_eq!((result.entries.len(), result.bytes, result.image_bytes), (0,0,0));
}
