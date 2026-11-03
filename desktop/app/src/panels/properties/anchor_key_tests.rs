use super::*;
use motion_core::{Project, Track, Tween};

pub(in crate::panels) fn spatial_state(reverse: bool) -> Value {
    let (_folder, engine) = mapped_anchor_tests::fixture(reverse);
    let mut state = engine.state().unwrap();
    let mut project: Project = serde_json::from_value(state["project"].clone()).unwrap();
    let transform = &mut project.layers[0].transform;
    // The legacy fixture separates position; restore its three vector keys.
    transform.position.merge_dimensions().unwrap();
    fn fixed_spatial<T: Tween>(track: &mut Track<T>) {
        track.enable_roving(&[-3]).unwrap();
        let spatial = track.spatial_animation.as_mut().unwrap();
        let key = spatial.keys()[1].id;
        spatial.set_fixed_time(key, -3.).unwrap();
    }
    fixed_spatial(&mut transform.anchor);
    fixed_spatial(&mut transform.position);
    project.validate().unwrap();
    state["project"] = serde_json::to_value(project).unwrap();
    state
}
fn pointer(point: [f32; 2], press: bool) -> Input {
    Input {
        mouse: point,
        held: Some(MouseButton::Left),
        pressed: press.then_some((MouseButton::Left, point)),
        ..Default::default()
    }
}
pub(in crate::panels) fn replace_middle(state: &mut Value, property: &str) {
    let spatial = &mut state["project"]["layers"][0]["transform"][property]["spatial_animation"];
    let next = spatial["next_id"].as_u64().unwrap();
    spatial["keys"][1]["id"] = next.into();
    spatial["next_id"] = (next + 1).into();
    let project: Project = serde_json::from_value(state["project"].clone()).unwrap();
    project.validate().unwrap();
}
#[test]
fn replacing_either_compensated_key_cancels_numeric_anchor_before_an_edit_is_sent() {
    for reverse in [false, true] {
        for property in ["anchor", "position"] {
            let state = spatial_state(reverse);
            let mut changed = state.clone();
            replace_middle(&mut changed, property);
            let mut atlas = TextAtlas::from_system_language(12., true).unwrap();
            let mut control = AnchorControls::default();
            let area = Rect::new(0., 0., 400., 160.);
            let styles = PaintList::default().styles();
            let rect = inspector_layout(&styles, area, 0., 0, 2).number;
            let start = [
                (rect.min[0] + rect.max[0]) * 0.5,
                (rect.min[1] + rect.max[1]) * 0.5,
            ];
            let mut actions = vec![];
            control.draw(
                &mut PaintList::default(),
                &mut atlas,
                area,
                0.,
                Panel::Properties,
                &state["project"]["layers"][0],
                &state,
                &pointer(start, true),
                false,
                &mut actions,
            );
            assert!(control.editing());
            actions.clear();
            control.process(
                &styles,
                &changed,
                &pointer([start[0] + 20., start[1]], false),
                &mut actions,
            );
            assert!(!control.editing(), "reverse={reverse} replaced={property}");
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Edit(_))),
                "stale anchor must not target the replacement key"
            );
        }
    }
}
