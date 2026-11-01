use motion_model::{
    tracking::{self, asset::*, camera::*, plate::*, shared::SharedData},
    *,
};
#[path = "tracking_plane.rs"]
mod plane_fixture;
pub fn camera_fixture() -> Project {
    let mut p = plane_fixture::plane_fixture();
    p.layers.clear();
    p.plugin_dependencies.clear();
    let mut data = (*p.tracking_assets[0].data).clone();
    data.meters_per_world_unit = Some(1.);
    let track = &mut data.tracks[0];
    track.kind = Kind::Camera;
    track.reference_index = None;
    for (i, s) in track.samples.iter_mut().enumerate() {
        let i = i as f64;
        s.observation = Some(Observation::Camera {
            camera_to_world: Pose {
                position: [0.1 * i, 0.05 * i, -5.],
                orientation_xyzw: [0., 0., 0., 1.],
            },
            calibration: Calibration::new(
                data.source_extent,
                [210. + 10. * i, 180. + 5. * i],
                [142. + i, 130. - i],
                [0.; 3],
                [0.; 2],
            )
            .unwrap(),
        });
    }
    p.tracking_assets[0].data = SharedData::new(data).unwrap();
    p.camera = Camera::new(p.width, p.height);
    let reference = CorrectionReference::capture(&p.camera, 0., [p.width, p.height]).unwrap();
    p.camera.tracking = Some(CameraBinding {
        asset: 2,
        track: "feature".into(),
        anchor_local_frame: 0,
        source_anchor_pts: -4000,
        speed_numerator: 1,
        speed_denominator: 1,
        world_basis: [1, -2, -3],
        target_meters_per_unit: 0.01,
        target_origin: [0.; 3],
        correction_reference: reference,
        crop: [0., 0., 320., 240.],
        rotation: Rotation::None,
        near: 0.5,
        far: 100000.,
        interpolation: tracking::point::Interpolation::Linear,
        max_gap_ticks: 4000,
        minimum_confidence: 0.8,
        missing: tracking::point::Missing::Error,
    });
    p.validate().unwrap();
    p
}
