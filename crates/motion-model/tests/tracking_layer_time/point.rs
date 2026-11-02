use super::*;
use motion_model::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

#[test]
fn mapped_point_and_authored_corrections_share_fractional_local_time() {
    for reverse_binding in [false, true] {
        let mut original = fixture();
        original.layers[0]
            .transform
            .position
            .upsert(0, [100., 80., 9.], Ease::Linear)
            .unwrap();
        original.layers[0]
            .transform
            .position
            .upsert(4, [108., 84., 13.], Ease::Linear)
            .unwrap();
        if reverse_binding {
            let binding = original.layers[0].tracking_point.as_mut().unwrap();
            binding.source_anchor_pts = 4000;
            binding.speed_numerator = -1;
        }
        for direction in [TimeDirection::Forward, TimeDirection::Reverse] {
            let mut mapped = original.clone();
            mapped.layers[0].timeline = Some(LayerTimeline {
                in_frame: 10,
                out_frame: 110,
                offset_frame: 10,
            });
            mapped.layers[0].time_mapping = Some(LayerTimeMapping {
                clock: LayerClock::new(
                    TimeStretch::new(3, 2).unwrap(),
                    direction,
                    0.,
                    if direction == TimeDirection::Reverse {
                        3.
                    } else {
                        0.
                    },
                )
                .unwrap(),
                remap: None,
            });
            mapped.validate().unwrap();
            for (composition, forward, reverse) in [
                (10., 0., 3.),
                (10.75, 0.5, 2.5),
                (11.5, 1., 2.),
                (14.5, 3., 0.),
            ] {
                let local = if direction == TimeDirection::Reverse {
                    reverse
                } else {
                    forward
                };
                assert_eq!(sampled(&mapped, composition), sampled(&original, local),
                    "binding_reverse={reverse_binding} layer_direction={direction:?} C={composition}");
            }
            assert_eq!(
                mapped.layers[0].tracking_point,
                original.layers[0].tracking_point
            );
            assert_eq!(mapped.layers[0].transform, original.layers[0].transform);
            assert!(mapped.tracking_assets[0]
                .data
                .shares_storage(&original.tracking_assets[0].data));
            let reopened: Project =
                serde_json::from_slice(&serde_json::to_vec(&mapped).unwrap()).unwrap();
            assert_eq!(reopened, mapped);
        }
    }
}
