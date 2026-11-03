use super::super::properties::anchor_key_tests::{replace_middle, spatial_state};
use super::*;
const VIEW: Rect = Rect {
    min: [0., 0.],
    max: [640., 360.],
};
fn pointer(point: [f32; 2], press: bool, release: bool) -> Input {
    Input {
        mouse: point,
        held: (!release).then_some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, point)),
        released: release.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
#[test]
fn canvas_anchor_binds_both_keys_and_cancels_replacements_including_late_release() {
    for reverse in [false, true] {
        for property in ["anchor", "position"] {
            let state = spatial_state(reverse);
            let pose = Pose::new(&state, 7, VIEW).unwrap();
            let start = pose.pivot;
            let end = [start[0] + 20., start[1] + 10.];
            let mut controls = Controls::default();
            let mut actions = vec![];
            controls.choose(Tool::Anchor, &mut actions);
            controls.draw(
                &mut PaintList::default(),
                VIEW,
                &state,
                Some(7),
                &pointer(start, true, false),
                &mut actions,
            );
            assert!(controls.gesture_active());
            actions.clear();
            controls.draw(
                &mut PaintList::default(),
                VIEW,
                &state,
                Some(7),
                &pointer(end, false, false),
                &mut actions,
            );
            assert!(actions.iter().any(|a|matches!(a,Action::Edit(Command::AnchorAt {keys:Some(keys),..}) if keys.anchor==Some(2) && keys.position==Some(2))));
            let mut changed = state.clone();
            replace_middle(&mut changed, property);
            actions.clear();
            controls.draw(
                &mut PaintList::default(),
                VIEW,
                &changed,
                Some(7),
                &pointer(end, false, true),
                &mut actions,
            );
            assert!(!controls.gesture_active());
            assert!(matches!(actions.as_slice(), [Action::History(4)]));
            actions.clear();
            controls.draw(
                &mut PaintList::default(),
                VIEW,
                &changed,
                Some(7),
                &pointer(end, false, true),
                &mut actions,
            );
            assert!(actions.is_empty());
        }
    }
}
