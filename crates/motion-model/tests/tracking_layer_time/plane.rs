use super::*;
use motion_model::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

fn corners(project: &Project, composition_frame: f64) -> Vec<[f32; 4]> {
    let layer = &project.layers[0];
    let effect = &layer.effects[0];
    let mut sampled = SampledEffect::new(layer.id, effect);
    for (index, parameter) in effect.params.values().enumerate() {
        sampled.values[index] = parameter
            .sample(layer.local_frame(composition_frame))
            .unwrap();
    }
    layer
        .apply_tracking_plane(project, composition_frame, &mut sampled)
        .unwrap();
    sampled.values.to_vec()
}

#[test]
fn mapped_plane_and_authored_corners_share_fractional_local_time() {
    let mut original = plane_fixture();
    let correction = &mut original.layers[0].effects[0]
        .params
        .get_mut("tl")
        .unwrap()
        .track;
    correction.upsert(0, [0.; 4], Ease::Linear).unwrap();
    correction
        .upsert(4, [8., 4., 0., 0.], Ease::Linear)
        .unwrap();
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
            assert_eq!(
                corners(&mapped, composition),
                corners(&original, local),
                "{direction:?} C={composition}"
            );
        }
        assert_eq!(
            mapped.layers[0].tracking_plane,
            original.layers[0].tracking_plane
        );
        assert_eq!(mapped.layers[0].effects, original.layers[0].effects);
        let reopened: Project =
            serde_json::from_slice(&serde_json::to_vec(&mapped).unwrap()).unwrap();
        assert_eq!(reopened, mapped);
    }
}
