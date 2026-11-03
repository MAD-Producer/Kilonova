use super::*;
use motion_model::layer_time::{LayerClock, LayerTimeMapping, TimeDirection, TimeStretch};

#[test]
fn mapped_camera_keeps_pose_calibration_and_manual_corrections_on_one_local_clock() {
    let mut original = camera_fixture();
    let position = original.camera.position.value;
    let target = original.camera.target.value;
    original
        .camera
        .position
        .upsert(0, position, Ease::Linear)
        .unwrap();
    original
        .camera
        .target
        .upsert(0, target, Ease::Linear)
        .unwrap();
    original
        .camera
        .position
        .upsert(
            4,
            [position[0] + 8., position[1], position[2]],
            Ease::Linear,
        )
        .unwrap();
    original
        .camera
        .target
        .upsert(4, [target[0] + 8., target[1], target[2]], Ease::Linear)
        .unwrap();
    for direction in [TimeDirection::Forward, TimeDirection::Reverse] {
        let mut mapped = original.clone();
        let mut layer = Layer::camera(7, "Mapped solved camera", mapped.width, mapped.height);
        layer.content = Content::Camera {
            camera: Box::new(original.camera.clone()),
        };
        layer.timeline = Some(LayerTimeline {
            in_frame: 10,
            out_frame: 110,
            offset_frame: 10,
        });
        layer.time_mapping = Some(LayerTimeMapping {
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
        mapped.camera = Camera::new(mapped.width, mapped.height);
        mapped.layers.push(layer);
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
            let expected = sample(&original, local);
            let mut worlds = Vec::new();
            let mut states = Vec::new();
            let mut cameras = Vec::new();
            hierarchy::matrices_with_cameras(
                &mapped,
                composition,
                &mut worlds,
                &mut states,
                &AuthoredCameraSampler,
                &mut cameras,
            )
            .unwrap();
            let actual = cameras[0].unwrap();
            assert_eq!(
                actual.pose.eye, expected.pose.eye,
                "{direction:?} C={composition}"
            );
            assert_eq!(actual.pose.view_projection, expected.pose.view_projection);
            assert_eq!(actual.lens, expected.lens);
        }
        assert_eq!(mapped.layers[0].camera_data().unwrap(), &original.camera);
        let reopened: Project =
            serde_json::from_slice(&serde_json::to_vec(&mapped).unwrap()).unwrap();
        assert_eq!(reopened, mapped);
    }
}
