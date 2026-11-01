//! Measured OpenCV board poses, not a synthetic camera-motion claim.
//! Source hashes/license/split and external-tool version are in the fixture.
//! This model-only helper uses placeholder video bytes, not native relink proof.
use motion_model::{
    tracking::{asset::*, plate::*, shared::SharedData},
    *,
};
use serde::Deserialize;
#[path = "tracking_camera.rs"]
mod synthetic;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurements {
    #[serde(rename = "K")]
    pub k: [[f64; 3]; 3],
    pub distortion: [f64; 5],
    pub object_points: Vec<[f64; 3]>,
    pub measured_corners: std::collections::BTreeMap<String, Vec<[f64; 2]>>,
    pub views: Vec<View>,
    pub inverse_oracle: Vec<Inverse>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub name: String,
    pub pose_fit_indices: Vec<usize>,
    pub validation_indices: Vec<usize>,
    pub world_to_camera_rotation: [[f64; 3]; 3],
    pub world_to_camera_translation: [f64; 3],
    pub projected: Vec<[f64; 2]>,
    pub held_out_rms: f64,
    pub held_out_max: f64,
}
#[derive(Deserialize)]
pub struct Inverse {
    pub distorted: [f64; 2],
    pub ideal: [f64; 2],
}
impl Measurements {
    pub fn load() -> Self {
        serde_json::from_str(include_str!("../fixtures/tracking-photographs.json")).unwrap()
    }
    pub fn calibration(&self) -> Calibration {
        let [k1, k2, p1, p2, k3] = self.distortion;
        Calibration::new(
            Extent::new(640, 480).unwrap(),
            [self.k[0][0], self.k[1][1]],
            [self.k[0][2], self.k[1][2]],
            [k1, k2, k3],
            [p1, p2],
        )
        .unwrap()
    }
    pub fn project(&self) -> Project {
        let mut p = synthetic::camera_fixture();
        let mut data = (*p.tracking_assets[0].data).clone();
        data.source_id = "opencv-board-square-scale-gauge".into();
        data.source_extent = Extent::new(640, 480).unwrap();
        // A square is assigned one project metre only as an arbitrary scale
        // gauge. These photographs have no measured physical square length.
        data.meters_per_world_unit = Some(1.);
        data.ticks_per_second = 24_000;
        data.nominal_rate = Rate::new(24, 1).unwrap();
        data.source_pts = vec![0, 1000, 2000];
        data.tracks[0].samples = self
            .views
            .iter()
            .enumerate()
            .map(|(i, view)| {
                let r = view.world_to_camera_rotation;
                // Rows of world-to-camera become columns of camera-to-world.
                let rotation = glam::DMat3::from_cols_array_2d(&r);
                let position =
                    -(rotation * glam::DVec3::from_array(view.world_to_camera_translation));
                Sample {
                    source_index: i as u32,
                    visibility: Visibility::Visible,
                    confidence: 1.,
                    observation: Some(Observation::Camera {
                        camera_to_world: Pose {
                            position: position.to_array(),
                            orientation_xyzw: glam::DQuat::from_mat3(&rotation)
                                .normalize()
                                .to_array(),
                        },
                        calibration: self.calibration(),
                    }),
                    manual_correction: false,
                }
            })
            .collect();
        p.tracking_assets[0].data = SharedData::new(data).unwrap();
        let video = &mut p.video_assets[0];
        video.width = 640;
        video.height = 480;
        video.display_width = 640;
        video.display_height = 480;
        video.video_start_us = 0;
        video.video_end_us = 125_000;
        video.duration_us = 125_000;
        video.frame_count = 3;
        video.variable_frame_rate = false;
        video.nominal_frame_rate = 24.;
        let binding = p.camera.tracking.as_mut().unwrap();
        binding.crop = [0., 0., 640., 480.];
        binding.source_anchor_pts = 0;
        p.validate().unwrap();
        p
    }
}
