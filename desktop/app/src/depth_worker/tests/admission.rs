// Host admission is tested without creating an adapter or executing a shader.
#[path = "../admission.rs"]
mod admission;
#[path = "../convolution_admission.rs"]
mod convolution_admission;
#[path = "../execution_identity.rs"]
mod execution_identity;

use admission::{Case, MODEL, Request, WEIGHTS};
use serde_json::{Value, json};

fn tensor(shape: &[u32]) -> Value {
    json!({"name":"authored-test", "dtype":"float32", "shape":shape,
        "file":"tensors/authored.f32", "sha256":"a".repeat(64)})
}
fn case(operation: Value, inputs: &[&[u32]], output: &[u32]) -> Case {
    serde_json::from_value(
        json!({"id":"host-contract", "origin":"authored_control", "source_node":null,
        "operation":operation,"inputs":inputs.iter().map(|s| tensor(s)).collect::<Vec<_>>(),
        "expected":tensor(output)}),
    )
    .expect("independently authored fixture schema")
}
fn request(case: &Case) -> Request {
    // Reconstruct one minimal real request so identity/origin admission is also
    // exercised separately from device-dependent planning.
    serde_json::from_value(json!({"version":1,"model_sha256":MODEL,"original_weights_sha256":WEIGHTS,
        "requested_backend":"vulkan","buffer_budget_bytes":256*1024*1024u64,
        "cases":[{"id":case.id,"origin":"authored_control","source_node":null,
        "operation":{"kind":"Softmax","axis":-1},"inputs":[tensor(&[2,3])],"expected":tensor(&[2,3])}]}))
        .expect("request fixture schema")
}
fn limits() -> wgpu::Limits {
    wgpu::Limits::downlevel_defaults()
}

#[test]
fn odd_matrix_tiles_and_shared_batches_have_explicit_dimensions() {
    let c = case(
        json!({"kind":"MatMul"}),
        &[&[2, 3, 17], &[17, 5]],
        &[2, 3, 5],
    );
    let p = c.plan(&limits(), 1024 * 1024).unwrap();
    assert_eq!(&p.parameters[..4], &[3, 5, 17, 2]);
    assert_eq!(&p.parameters[4..6], &[51, 0]);
    assert_eq!(p.dispatch, [1, 1, 2]);
    assert_eq!(p.output_bytes, 120);
    assert!(p.admitted_buffer_bytes > p.output_bytes * 2);
}

#[test]
fn interleaved_broadcasting_is_refused_instead_of_misindexed() {
    let c = case(
        json!({"kind":"MatMul"}),
        &[&[2, 1, 3, 5], &[1, 4, 5, 7]],
        &[2, 4, 3, 7],
    );
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
}

#[test]
fn shape_contraction_and_missing_inputs_are_rejected_before_indexing() {
    let bad = case(json!({"kind":"MatMul"}), &[&[3, 5], &[6, 7]], &[3, 7]);
    assert!(bad.plan(&limits(), 1024 * 1024).is_err());
    let absent = case(json!({"kind":"MatMul"}), &[], &[3, 7]);
    assert!(absent.plan(&limits(), 1024 * 1024).is_err());
}

#[test]
fn logical_budget_includes_readback_inputs_and_uniforms() {
    let c = case(json!({"kind":"Softmax","axis":-1}), &[&[2, 3]], &[2, 3]);
    let p = c.plan(&limits(), 1024 * 1024).unwrap();
    assert!(c.plan(&limits(), p.admitted_buffer_bytes - 1).is_err());
    assert!(c.plan(&limits(), p.admitted_buffer_bytes).is_ok());
}

#[test]
fn normalization_rejects_wrong_axis_scale_and_nonpositive_epsilon() {
    let c = case(json!({"kind":"Softmax","axis":0}), &[&[2, 3]], &[2, 3]);
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
    for epsilon in [0.0, -1e-6] {
        let c = case(
            json!({"kind":"LayerNormalization","axis":-1,"epsilon":epsilon}),
            &[&[2, 3], &[3], &[3]],
            &[2, 3],
        );
        assert!(c.plan(&limits(), 1024 * 1024).is_err());
    }
    let c = case(
        json!({"kind":"LayerNormalization","axis":-1,"epsilon":1e-6}),
        &[&[2, 3], &[2], &[3]],
        &[2, 3],
    );
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
}

#[test]
fn reduction_dispatch_flattens_rows_and_refuses_unrepresentable_counts() {
    let mut l = limits();
    l.max_compute_workgroups_per_dimension = 2;
    let c = case(json!({"kind":"Softmax","axis":-1}), &[&[3, 5]], &[3, 5]);
    let p = c.plan(&l, 1024 * 1024).unwrap();
    assert_eq!(p.dispatch, [2, 2, 1]);
    assert_eq!(p.parameters[13], 2);
    let c = case(json!({"kind":"Softmax","axis":-1}), &[&[5, 5]], &[5, 5]);
    assert!(c.plan(&l, 1024 * 1024).is_err());
}

#[test]
fn resize_large_feature_maps_use_two_dimensional_dispatch() {
    let c = case(
        json!({"kind":"Resize","mode":"linear","coordinate_transformation_mode":"align_corners"}),
        &[&[1, 64, 37, 37]],
        &[1, 64, 518, 518],
    );
    let p = c.plan(&limits(), 256 * 1024 * 1024).unwrap();
    assert!(p.dispatch[1] > 1);
    assert_eq!(p.parameters[12], p.dispatch[0] * 64);
    assert_eq!(&p.parameters[4..6], &[518, 518]);
}

#[test]
fn resize_cannot_silently_substitute_coordinate_rules_or_channels() {
    let c = case(
        json!({"kind":"Resize","mode":"linear","coordinate_transformation_mode":"half_pixel"}),
        &[&[1, 3, 3, 5]],
        &[1, 3, 7, 9],
    );
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
    let c = case(
        json!({"kind":"Resize","mode":"linear","coordinate_transformation_mode":"align_corners"}),
        &[&[1, 3, 3, 5]],
        &[1, 4, 7, 9],
    );
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
}

#[test]
fn oversized_zero_and_overflowing_tensors_are_rejected() {
    for shape in [&[0, 3][..], &[u32::MAX, 2][..], &[1, 1, 1, 1, 1][..]] {
        let c = case(json!({"kind":"Softmax","axis":-1}), &[shape], shape);
        assert!(c.plan(&limits(), 512 * 1024 * 1024).is_err());
    }
    let mut l = limits();
    l.max_storage_buffer_binding_size = 16;
    let c = case(json!({"kind":"Softmax","axis":-1}), &[&[2, 3]], &[2, 3]);
    assert!(c.plan(&l, 1024 * 1024).is_err());
}

#[test]
fn current_model_identity_and_control_origin_are_explicit() {
    let c = case(json!({"kind":"Softmax","axis":-1}), &[&[2, 3]], &[2, 3]);
    let mut r = request(&c);
    r.validate().unwrap();
    r.model_sha256 = "0".repeat(64);
    assert!(r.validate().is_err());
    r.model_sha256 = MODEL.into();
    r.cases[0].source_node = Some("fake-model-node".into());
    assert!(r.validate().is_err());
    r.cases[0].source_node = None;
    r.requested_backend = "auto".into();
    assert!(r.validate().is_err());
}

#[test]
fn erf_shape_and_flat_dispatch_are_admitted_before_submission() {
    let mut l = limits();
    l.max_compute_workgroups_per_dimension = 2;
    let c = case(json!({"kind":"Erf"}), &[&[129]], &[129]);
    let p = c.plan(&l, 1024 * 1024).unwrap();
    assert_eq!(p.entry, "erf_elementwise");
    assert_eq!(p.dispatch, [2, 2, 1]);
    assert_eq!(&p.parameters[..2], &[129, 128]);
    let mismatched = case(json!({"kind":"Erf"}), &[&[129]], &[128]);
    assert!(mismatched.plan(&l, 1024 * 1024).is_err());
}

#[test]
fn convolution_wire_contract_preserves_weights_bias_and_restricted_transpose() {
    let attrs = json!({"kernel":[2,2],"stride":[2,2],"padding":[0,0,0,0],
        "dilation":[1,1],"group":1,"output_padding":[0,0]});
    let c = case(
        json!({"kind":"ConvTranspose","attributes":attrs}),
        &[&[2, 3, 5, 7], &[3, 4, 2, 2], &[4]],
        &[2, 4, 10, 14],
    );
    let p = c.plan(&limits(), 1024 * 1024).unwrap();
    assert_eq!(p.entry, "conv_transpose_no_overlap_nchw");
    assert_eq!(p.parameters[14], 1);
    assert_eq!(&p.parameters[..4], &[2, 3, 5, 7]);
    let mut wrong = attrs.clone();
    wrong["stride"] = json!([1, 1]);
    let c = case(
        json!({"kind":"ConvTranspose","attributes":wrong}),
        &[&[2, 3, 5, 7], &[3, 4, 2, 2]],
        &[2, 4, 10, 14],
    );
    assert!(c.plan(&limits(), 1024 * 1024).is_err());
}
