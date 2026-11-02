//! A paired edit cannot expose a new plate with the previous plate's solve.
use motion_core::tracking::{
    asset::{Observation, Sample},
    plate::Calibration,
    shared::SharedData,
};
use motion_core::tracking_assets::replacement::{BindingUpdate, Replacement};
use motion_core::*;
#[path = "../../motion-model/tests/support/tracking_camera.rs"]
mod fixture;
#[path = "tracking_replacement/bindings.rs"]
mod bindings;
#[path = "tracking_replacement/layer_time.rs"]
mod mapped_clock;

fn replacement(project: &Project) -> Replacement {
    let mut video = project.video_assets[0].clone();
    video.path = "assets/new-plate.mp4".into();
    video.bytes = 120;
    video.width = 480;
    video.height = 360;
    video.display_width = 480;
    video.display_height = 360;
    video.frame_count = 5;
    video.video_end_us = 400000;
    video.duration_us = 400000;
    let mut attachment = project.tracking_assets[0].clone();
    attachment.path = "assets/tracking/new-original.mstrack".into();
    attachment.sha256 = "11".repeat(32);
    attachment.source_sha256 = "09".repeat(32);
    let mut data = (*attachment.data).clone();
    data.source_sha256 = [9; 32];
    data.source_extent = tracking::plate::Extent::new(480, 360).unwrap();
    data.source_pts = data.source_pts.iter().map(|t| t + 6000).collect();
    data.source_pts.push(14000);
    let track = &mut data.tracks[0];
    track.id = "new-camera".into();
    let tail = track.samples.last().unwrap().clone();
    track.samples.push(Sample {
        source_index: 4,
        ..tail
    });
    for sample in &mut track.samples {
        if let Some(Observation::Camera {
            camera_to_world,
            calibration,
        }) = &mut sample.observation
        {
            camera_to_world.position[0] += 1.;
            *calibration = Calibration::new(
                data.source_extent,
                [315., 270.],
                [240., 180.],
                [0.; 3],
                [0.; 2],
            )
            .unwrap();
        }
    }
    attachment.data = SharedData::new(data).unwrap();
    let mut binding = project.camera.tracking.clone().unwrap();
    binding.track = "new-camera".into();
    binding.source_anchor_pts = 2000;
    binding.crop = [0., 0., 480., 360.];
    Replacement {
        video,
        audio: None,
        tracking: vec![attachment],
        bindings: vec![BindingUpdate::Camera {
            composition: project.composition_id.clone(),
            object: 0,
            binding: Some(binding),
        }],
    }
}

#[test]
fn plate_and_fresh_solve_change_in_one_history_step_without_baking() {
    let before = fixture::camera_fixture();
    let paired = replacement(&before);
    let mut engine = Engine::new(before.clone()).unwrap();
    // A plain media edit cannot cross this intermediate invalid state.
    assert!(engine
        .apply(Command::MediaAsset {
            action: media_assets::AssetAction::ReplaceVideo {
                video: paired.video.clone(),
                audio: None
            }
        })
        .is_err());
    assert_eq!(engine.project(), &before);
    let command = Command::ReplaceTrackedVideo {
        replacement: Box::new(paired),
    };
    assert!(command.changes_resources());
    assert!(command.composition_transaction());
    engine.apply(command).unwrap();
    let after = engine.snapshot();
    assert_eq!(engine.revision(), 1);
    assert_eq!(after.tracking_assets[0].id, before.tracking_assets[0].id);
    assert_eq!(after.video_assets[0].id, before.video_assets[0].id);
    assert_eq!(after.camera.position, before.camera.position);
    assert_eq!(after.camera.fov, before.camera.fov);
    let mut scene = motion_render::Scene::new(&after).unwrap();
    scene
        .sample(&after, 1., None, &ExpressionEvaluator)
        .unwrap();
    assert!(
        (scene.camera.eye.x - 110.).abs() < 0.001,
        "{}",
        scene.camera.eye.x
    );
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &after);
    assert!(!engine.redo().unwrap());
}

#[test]
fn incomplete_solve_set_and_invalid_rebinding_preserve_revision_history_and_redo() {
    let mut before = fixture::camera_fixture();
    let mut another = before.tracking_assets[0].clone();
    another.id = 3;
    before.tracking_assets.push(another);
    let paired = replacement(&before);
    let mut engine = Engine::new(before.clone()).unwrap();
    engine
        .apply(Command::SetColor {
            object: 0,
            value: [0.1, 0.2, 0.3, 1.],
        })
        .unwrap();
    let redone = engine.snapshot();
    assert!(engine.undo().unwrap());
    let revision = engine.revision();
    let cause = engine
        .apply(Command::ReplaceTrackedVideo {
            replacement: Box::new(paired.clone()),
        })
        .unwrap_err();
    assert!(cause.to_string().contains("every solve"), "{cause}");
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.project(), &before);
    let mut complete = paired;
    let mut second = complete.tracking[0].clone();
    second.id = 3;
    second.path = "assets/tracking/second-new.mstrack".into();
    complete.tracking.push(second);
    complete.bindings.clear();
    let cause = engine
        .apply(Command::ReplaceTrackedVideo {
            replacement: Box::new(complete),
        })
        .unwrap_err();
    assert!(cause.to_string().contains("feature"), "{cause}");
    assert_eq!(engine.revision(), revision);
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &redone);
}

#[test]
fn duplicate_solve_or_binding_and_reused_owned_paths_are_rejected() {
    let before = fixture::camera_fixture();
    let paired = replacement(&before);
    let mut cases = Vec::new();
    let mut duplicate = paired.clone();
    duplicate.tracking.push(duplicate.tracking[0].clone());
    cases.push(duplicate);
    let mut duplicate = paired.clone();
    duplicate.bindings.push(duplicate.bindings[0].clone());
    cases.push(duplicate);
    let mut overwrite = paired.clone();
    overwrite.video.path = before.video_assets[0].path.clone();
    cases.push(overwrite);
    let mut overwrite = paired.clone();
    overwrite.tracking[0].path = before.tracking_assets[0].path.clone();
    cases.push(overwrite);
    let mut foreign = paired;
    foreign.tracking[0].source.id = 99;
    cases.push(foreign);
    for case in cases {
        assert!(case.propose(&before).is_err());
        let mut engine = Engine::new(before.clone()).unwrap();
        assert!(engine
            .apply(Command::ReplaceTrackedVideo {
                replacement: Box::new(case)
            })
            .is_err());
        assert_eq!(engine.project(), &before);
        assert_eq!(engine.revision(), 0);
    }
}

#[test]
fn paired_edit_rejects_an_active_gesture_and_requires_explicit_nullable_fields() {
    let before = fixture::camera_fixture();
    let command = Command::ReplaceTrackedVideo {
        replacement: Box::new(replacement(&before)),
    };
    let encoded = serde_json::to_value(&command).unwrap();
    assert_eq!(CommandRegistry::id(&command), "replace_tracked_video");
    assert_eq!(
        CommandRegistry::parse(&encoded.to_string()).unwrap().len(),
        1
    );
    for field in ["binding", "object", "composition"] {
        let mut wrong = encoded.clone();
        wrong["replacement"]["bindings"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            CommandRegistry::parse(&wrong.to_string()).is_err(),
            "{field}"
        );
    }
    let mut wrong = encoded.clone();
    wrong["replacement"]
        .as_object_mut()
        .unwrap()
        .remove("audio");
    assert!(CommandRegistry::parse(&wrong.to_string()).is_err());
    let mut engine = Engine::new(before.clone()).unwrap();
    engine.begin_gesture().unwrap();
    assert!(engine.apply(command).is_err());
    assert!(engine.gesture_active());
    assert_eq!(engine.project(), &before);
    assert_eq!(engine.revision(), 0);
}
