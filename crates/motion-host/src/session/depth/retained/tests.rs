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
        input: PreparedDepthInput {
            rgb_size: [2, 1],
            rgb_sha256: [asset as u8; 32],
        },
    }
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
