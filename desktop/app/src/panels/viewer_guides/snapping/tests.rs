use super::*;
use crate::viewer_navigation::Navigation;

fn fixture(zoom: f64) -> (Value, Mapping) {
    let mut navigation = Navigation::default();
    let clip = Rect::new(20., 60., 1400., 1000.);
    assert!(navigation.zoom_at(clip, [1920., 1080.], [720., 560.], zoom));
    assert!(navigation.pan(clip, [1920., 1080.], [37., -23.]));
    let mapping = navigation.mapping(clip, [1920., 1080.]).unwrap();
    let state = serde_json::json!({"root":"snap-targets","document_generation":2,"revision":3,"frame":0.,"composition":"comp-main",
    "project":{"composition_id":"comp-main","guides":[]},
    "projectedLayers":[
        {"id":7,"corners":[[735.35,200.],[755.35,200.],[755.35,220.],[735.35,220.]]},
        {"id":8,"corners":[[800.,400.],[1000.,400.],[1000.,600.],[800.,600.]]}
    ]});
    (state, mapping)
}
fn context(state: &Value, mapping: Mapping) -> Snapping {
    Controls::default()
        .transform_snapping(
            state,
            mapping,
            [1920., 1080.],
            Some(7),
            &PaintList::default().styles(),
        )
        .unwrap()
}

#[test]
fn automatic_geometry_keeps_screen_tolerance_and_falls_back_to_integer_pixels() {
    for zoom in [0.25, 1., 4.] {
        let (state, mapping) = fixture(zoom);
        let snapping = context(&state, mapping);
        for reference in [0., 800., 900., 1000., 1920.] {
            let point = mapping.screen([reference, 700.37]);
            let near = [point[0] + 5., point[1]];
            let actual = snapping.targets(near)[0].unwrap();
            assert!(
                (actual - point[0]).abs() < 0.01,
                "zoom={zoom} target={reference}"
            );
            let far = [point[0] + 7., point[1]];
            assert!((snapping.targets(far)[0].unwrap() - point[0]).abs() > 0.1);
        }
        let point = mapping.screen([735.35, 700.37]);
        let snapped = mapping.local(snapping.point(point));
        assert!(
            (snapped[0] - 735.).abs() < 0.001,
            "self excluded: {snapped:?}"
        );
        assert!(
            (snapped[1] - 700.).abs() < 0.001,
            "integer fallback: {snapped:?}"
        );
        assert!(snapping
            .targets([f32::NAN, f32::INFINITY])
            .iter()
            .all(Option::is_none));
    }
}

#[test]
fn explicit_guides_survive_integer_fallback_and_visibility_only_gates_their_own_targets() {
    let (mut state, mapping) = fixture(1.);
    state["project"]["guides"] = serde_json::json!([{"id":1,"axis":"x","position":1234.25}]);
    let point = mapping.screen([1233.9, 700.37]);
    let mut controls = Controls::default();
    let snapped = controls
        .transform_snapping(
            &state,
            mapping,
            [1920., 1080.],
            Some(7),
            &PaintList::default().styles(),
        )
        .unwrap()
        .point(point);
    assert!((mapping.local(snapped)[0] - 1234.25).abs() < 0.001);
    controls.visibility.guides = false;
    assert!(controls
        .snapping(
            &state,
            mapping,
            [1920., 1080.],
            &PaintList::default().styles()
        )
        .is_none());
    let snapped = controls
        .transform_snapping(
            &state,
            mapping,
            [1920., 1080.],
            Some(7),
            &PaintList::default().styles(),
        )
        .unwrap()
        .point(point);
    assert!((mapping.local(snapped)[0] - 1234.).abs() < 0.001);
    state["observing"] = true.into();
    assert!(controls
        .transform_snapping(
            &state,
            mapping,
            [1920., 1080.],
            Some(7),
            &PaintList::default().styles()
        )
        .is_none());
}

#[test]
fn malformed_projected_geometry_is_an_error_but_empty_or_collapsed_geometry_is_normal() {
    let (mut state, _) = fixture(1.);
    assert!(layer_targets(&state, Some(7)).is_ok());
    for malformed in [
        serde_json::json!([[1., null]]),
        serde_json::json!([[1., 2., 3.]]),
        Value::Null,
    ] {
        state["projectedLayers"][1]["corners"] = malformed;
        assert!(layer_targets(&state, Some(7))
            .unwrap_err()
            .contains("layer=8"));
    }
    state["projectedLayers"][1]["corners"] = serde_json::json!(vec![[12., 14.]; 4]);
    assert!(layer_targets(&state, Some(7)).is_ok());
    state["projectedLayers"] = serde_json::json!([]);
    assert_eq!(
        layer_targets(&state, None).unwrap(),
        [Vec::<f64>::new(), Vec::new()]
    );
}
