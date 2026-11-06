// Pending native execution: host indexing controls, separate from GPU comparison.
#[path = "../index_admission.rs"]
mod admission;
use admission::{Case, Plan};
use serde_json::{Value, json};
fn tensor(shape: &[u32]) -> Value {
    json!({"name":"authored-index-test","dtype":"float32","shape":shape,
        "file":"tensors/authored.f32","sha256":"a".repeat(64)})
}
fn case(operation: Value, inputs: &[&[u32]], output: &[u32]) -> Case {
    serde_json::from_value(json!({"id":"index-host-control","origin":"authored_control","source_node":null,
        "operation":operation,"inputs":inputs.iter().map(|s|tensor(s)).collect::<Vec<_>>(),"expected":tensor(output)})).unwrap()
}
fn plan(case: &Case) -> Plan {
    case.plan(&wgpu::Limits::downlevel_defaults(), 256 << 20)
        .unwrap()
}
fn mapped(plan: &Plan, input: usize, index: u32) -> i64 {
    let mut rest = index;
    let mut result = i64::from(plan.parameters[48 + input] as i32);
    for axis in (0..5).rev() {
        let size = plan.parameters[4 + axis];
        let coordinate = rest % size;
        rest /= size;
        result += i64::from(coordinate) * i64::from(plan.parameters[12 + input * 8 + axis] as i32);
    }
    result
}
#[test]
fn rank_five_permutation_matches_independent_source_to_destination_enumeration() {
    let shape = [1, 2, 3, 2, 2];
    let permutation = [2, 0, 3, 1, 4];
    let output = [3, 1, 2, 2, 2];
    let p = plan(&case(
        json!({"kind":"Transpose","permutation":permutation}),
        &[&shape],
        &output,
    ));
    let mut reference = vec![u32::MAX; 24];
    for source in 0..24u32 {
        let mut coordinate = [0u32; 5];
        let mut remaining = source;
        for axis in (0..5).rev() {
            coordinate[axis] = remaining % shape[axis];
            remaining /= shape[axis];
        }
        let destination = permutation
            .iter()
            .enumerate()
            .fold(0, |flat, (i, &axis)| flat * output[i] + coordinate[axis]);
        assert_eq!(reference[destination as usize], u32::MAX);
        reference[destination as usize] = source;
    }
    assert_eq!(
        (0..24).map(|i| mapped(&p, 0, i) as u32).collect::<Vec<_>>(),
        reference
    );
}
#[test]
fn scalar_gather_normalizes_negative_indices_without_reading_control_as_float() {
    let p = plan(&case(
        json!({"kind":"GatherScalar","axis":-3,"index":-1}),
        &[&[3, 2, 2]],
        &[2, 2],
    ));
    assert_eq!(
        (0..4).map(|i| mapped(&p, 0, i)).collect::<Vec<_>>(),
        vec![8, 9, 10, 11]
    );
    for index in [-4, 3] {
        let c = case(
            json!({"kind":"GatherScalar","axis":0,"index":index}),
            &[&[3, 2, 2]],
            &[2, 2],
        );
        assert!(
            c.plan(&wgpu::Limits::downlevel_defaults(), 1 << 20)
                .is_err()
        );
    }
}
#[test]
fn reverse_slice_with_extreme_end_preserves_signed_coordinates() {
    let p = plan(&case(
        json!({"kind":"Slice","starts":[2,3],"ends":[-4,i64::MIN],"axes":[1,2],"steps":[-1,-2]}),
        &[&[2, 3, 4]],
        &[2, 3, 2],
    ));
    assert_eq!(
        (0..12).map(|i| mapped(&p, 0, i)).collect::<Vec<_>>(),
        vec![11, 9, 7, 5, 3, 1, 23, 21, 19, 17, 15, 13]
    );
}
#[test]
fn interleaved_binary_broadcast_maps_each_axis_and_rejects_extra_expansion() {
    let p = plan(&case(
        json!({"kind":"Add"}),
        &[&[2, 1, 3], &[1, 4, 1]],
        &[2, 4, 3],
    ));
    for i in 0..24 {
        assert_eq!(mapped(&p, 0, i), i64::from(i / 12 * 3 + i % 3));
        assert_eq!(mapped(&p, 1, i), i64::from(i / 3 % 4));
    }
    let c = case(json!({"kind":"Mul"}), &[&[1], &[1]], &[4]);
    assert!(
        c.plan(&wgpu::Limits::downlevel_defaults(), 1 << 20)
            .is_err()
    );
    let p = plan(&case(json!({"kind":"Expand"}), &[&[1]], &[4]));
    assert!((0..4).all(|i| mapped(&p, 0, i) == 0));
}
#[test]
fn concatenation_has_explicit_axis_extents_and_rejects_non_axis_mismatch() {
    let p = plan(&case(
        json!({"kind":"Concat","axis":-2}),
        &[&[2, 1, 3], &[2, 2, 3], &[2, 1, 3]],
        &[2, 4, 3],
    ));
    assert_eq!(p.parameters[42], 3);
    assert_eq!(&p.parameters[43..46], &[1, 2, 1]);
    let c = case(
        json!({"kind":"Concat","axis":1}),
        &[&[2, 1, 3], &[2, 2, 4]],
        &[2, 3, 3],
    );
    assert!(
        c.plan(&wgpu::Limits::downlevel_defaults(), 1 << 20)
            .is_err()
    );
}
#[test]
fn malformed_mapping_controls_are_refused_before_indexing() {
    for c in [
        case(
            json!({"kind":"Transpose","permutation":[0,0]}),
            &[&[2, 3]],
            &[2, 3],
        ),
        case(
            json!({"kind":"Slice","starts":[0,0],"ends":[2,2],"axes":[0,0],"steps":[1,1]}),
            &[&[2, 3]],
            &[2, 3],
        ),
        case(
            json!({"kind":"Slice","starts":[0],"ends":[2],"axes":[0],"steps":[0]}),
            &[&[2, 3]],
            &[2, 3],
        ),
    ] {
        assert!(
            c.plan(&wgpu::Limits::downlevel_defaults(), 1 << 20)
                .is_err()
        );
    }
}
#[test]
fn rank_signed_storage_budget_and_dispatch_bounds_are_checked() {
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_compute_workgroups_per_dimension = 2;
    let c = case(json!({"kind":"Relu"}), &[&[129]], &[129]);
    let p = c.plan(&limits, 1 << 20).unwrap();
    assert_eq!(p.dispatch, [2, 2, 1]);
    assert_eq!(p.parameters[1], 128);
    assert!(c.plan(&limits, 1).is_err());
    for shape in [&[257][..], &[u32::MAX][..], &[1, 1, 1, 1, 1, 1][..]] {
        let c = case(json!({"kind":"Relu"}), &[shape], shape);
        assert!(c.plan(&limits, 256 << 20).is_err());
    }
}
