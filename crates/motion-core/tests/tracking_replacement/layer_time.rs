use super::*;
use motion_core::layer_time::{LayerTimeAction, TimeStretch};

#[test]
fn layer_clock_and_paired_replacement_keep_distinct_atomic_history_steps() {
    let mut before = fixture::camera_fixture();
    let mut plate = Layer::solid(8, "Original plate", [320., 240.], [320., 240., 0.], [1.; 4]);
    plate.content = Content::Video {
        video: VideoClip::new(1),
    };
    plate.timeline = Some(LayerTimeline {
        in_frame: 0,
        out_frame: 4,
        offset_frame: 0,
    });
    before.layers.push(plate);
    let mut engine = Engine::new(before.clone()).unwrap();
    let command = Command::LayerTime {
        object: 8,
        action: LayerTimeAction::Stretch {
            stretch: TimeStretch::new(3, 2).unwrap(),
            anchor_frame: 0.,
            resize_clip: false,
        },
    };
    assert_eq!(CommandRegistry::id(&command), "layer_time");
    engine.apply(command).unwrap();
    let timed = engine.snapshot();
    assert_eq!(engine.revision(), 1);
    assert_eq!(timed.layers[0].checked_local_frame(1.5).unwrap(), 1.);
    engine
        .apply(Command::ReplaceTrackedVideo {
            replacement: Box::new(replacement(&timed)),
        })
        .unwrap();
    let replaced = engine.snapshot();
    assert_eq!(engine.revision(), 2);
    assert_eq!(replaced.layers, timed.layers);
    assert_ne!(replaced.camera.tracking, timed.camera.tracking);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &timed);
    assert!(engine.undo().unwrap());
    assert_eq!(engine.project(), &before);
    assert!(!engine.undo().unwrap());
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &timed);
    assert!(engine.redo().unwrap());
    assert_eq!(engine.project(), &replaced);
    assert!(!engine.redo().unwrap());
    let reopened: Project =
        serde_json::from_slice(&serde_json::to_vec(&replaced).unwrap()).unwrap();
    assert_eq!(reopened, replaced);
    assert_eq!(reopened.version, CURRENT_PROJECT_VERSION);
}
