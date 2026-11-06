//! Independent graph ownership controls. No GPU or learned inference here.
#[path = "../admission.rs"]
mod admission;
#[path = "../convolution_admission.rs"]
mod convolution_admission;
#[path = "../graph_admission.rs"]
mod graph_admission;
#[path = "../index_admission.rs"]
mod index_admission;
use graph_admission::Blueprint;
use serde_json::{Value, json};

const RGB: u64 = 1 * 3 * 518 * 518 * 4;
const DEPTH: u64 = 518 * 518 * 4;
fn fixture() -> Value {
    let tensor = |shape: &[u32], bytes| json!({"dtype":"float32","shape":shape,"bytes":bytes});
    let mut tensors = serde_json::Map::new();
    for name in ["rgb", "a", "a_alias", "spare"] {
        tensors.insert(name.into(), tensor(&[1, 3, 518, 518], RGB));
    }
    for name in ["b", "tail", "relative_inverse_depth"] {
        tensors.insert(name.into(), tensor(&[1, 518, 518], DEPTH));
    }
    let node = |index, input: &str, output: &str, op: Value| {
        json!({
        "index":index,"name":format!("authored-node-{index}"),"family":"index","operation":op,
        "inputs":[input],"inputOwners":[if input=="a_alias" {"a"} else {input}],"output":output,"owner":output})
    };
    json!({"version":1,"capability":"static_program_only_not_executable",
        "modelSha256":admission::MODEL,"originalWeightsSha256":admission::WEIGHTS,
        "input":{"name":"rgb","dtype":"float32","shape":[1,3,518,518],"preprocessing":"authored metadata only"},
        "outputs":["relative_inverse_depth"],"tensors":tensors,"resources":{},
        "aliases":{"a_alias":"a","relative_inverse_depth":"tail"},
        "ownerSlots":{"rgb":0,"a":1,"spare":2,"b":3,"tail":0},
        "poolSlotCapacities":[RGB,RGB,RGB,DEPTH],"persistentCapacities":{},
        "capacityBudgetBytes":256u64<<20,"declaredRetainedBufferCapacityBytes":RGB*3+DEPTH*2+1024+4096+12,
        "uniformRingBytes":1024,"uniformStride":256,"readbackBytes":DEPTH,"uploadStagingBytes":4096,"dummyBindingBytes":12,
        "orderingContract":"Separate ordered compute passes in one submission; retain pool until completion before next-job reuse",
        "allocationContract":"Allocate every final slot capacity upfront; trace assigns owners without runtime resize",
        "operations":[node(0,"rgb","a",json!({"kind":"Relu"})),node(1,"rgb","spare",json!({"kind":"Relu"})),
            node(2,"a_alias","b",json!({"kind":"GatherScalar","axis":1,"index":0})),node(3,"b","tail",json!({"kind":"Relu"}))]})
}
fn compile(v: Value) -> Result<graph_admission::CompiledGraph, String> {
    serde_json::from_value::<Blueprint>(v)
        .map_err(|e| e.to_string())?
        .compile_for_research(&wgpu::Limits::downlevel_defaults())
}
#[test]
fn reusable_slots_preserve_alias_reads_and_final_output_lifetime() {
    let graph = compile(fixture()).unwrap();
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.nodes[2].input_buffers[0], 1);
    assert_eq!(graph.nodes[3].output_buffer, 0);
    assert_eq!(graph.output_buffer, 0);
    assert_eq!(
        graph
            .nodes
            .iter()
            .map(|n| n.uniform_offset)
            .collect::<Vec<_>>(),
        vec![0, 256, 512, 768]
    );
    assert_eq!(
        graph.nodes.iter().map(|n| n.entry).collect::<Vec<_>>(),
        vec!["relu", "relu", "index_copy", "relu"]
    );
}
#[test]
fn a_later_alias_read_prevents_intermediate_slot_reuse() {
    let mut v = fixture();
    v["ownerSlots"]["spare"] = 1.into();
    assert!(
        compile(v)
            .unwrap_err_string()
            .contains("overlapping graph owner lifetimes")
    );
}
trait ErrString {
    fn unwrap_err_string(self) -> String;
}
impl<T> ErrString for Result<T, String> {
    fn unwrap_err_string(self) -> String {
        match self {
            Err(e) => e,
            Ok(_) => panic!("invalid graph accepted"),
        }
    }
}
#[test]
fn current_read_write_alias_and_unproduced_source_are_refused() {
    let mut v = fixture();
    v["ownerSlots"]["a"] = 0.into();
    assert!(compile(v).unwrap_err_string().contains("current read"));
    let mut v = fixture();
    v["operations"][0]["inputs"][0] = "b".into();
    v["operations"][0]["inputOwners"][0] = "b".into();
    assert!(compile(v).unwrap_err_string().contains("input missing"));
}
#[test]
fn accounting_and_single_byte_budget_difference_are_refused() {
    let mut v = fixture();
    v["declaredRetainedBufferCapacityBytes"] = 1.into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["capacityBudgetBytes"] =
        json!(v["declaredRetainedBufferCapacityBytes"].as_u64().unwrap() - 1);
    assert!(compile(v).unwrap_err_string().contains("exhausted budget"));
    let mut v = fixture();
    v["poolSlotCapacities"][1] = 4.into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["readbackBytes"] = 4.into();
    assert!(compile(v).is_err());
}
#[test]
fn alias_cycles_extent_changes_and_missing_owners_are_refused() {
    let mut v = fixture();
    v["aliases"]["a"] = "a_alias".into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["aliases"]["relative_inverse_depth"] = "rgb".into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["ownerSlots"]["extra"] = 0.into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["operations"][2]["inputOwners"][0] = "spare".into();
    assert!(compile(v).is_err());
}
#[test]
fn identity_unknown_fields_and_unqualified_contracts_are_refused() {
    for key in [
        "version",
        "capability",
        "modelSha256",
        "originalWeightsSha256",
        "orderingContract",
        "allocationContract",
    ] {
        let mut v = fixture();
        v[key] = if key == "version" {
            json!(2)
        } else {
            json!("unqualified")
        };
        assert!(compile(v).is_err(), "{key}");
    }
    let mut v = fixture();
    v["unexpected"] = true.into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["operations"][1]["index"] = 0.into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["operations"][1]["family"] = "unsupported".into();
    assert!(compile(v).is_err());
    let mut v = fixture();
    v["operations"][2]["operation"]["index"] = 99.into();
    let cause = compile(v).unwrap_err_string();
    assert!(cause.contains("index node #2 authored-node-2 admission:"));
    assert!(cause.contains("Gather scalar index out of bounds"));
    let mut v = fixture();
    let unbounded = "x".repeat(257);
    v["operations"][0]["inputs"][0] = unbounded.clone().into();
    let cause = compile(v).unwrap_err_string();
    assert!(!cause.contains(&unbounded));
}
#[test]
fn device_buffer_and_alignment_limits_are_checked_without_division_by_zero() {
    let graph: Blueprint = serde_json::from_value(fixture()).unwrap();
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.min_uniform_buffer_offset_alignment = 0;
    assert!(graph.compile_for_research(&limits).is_err());
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_storage_buffer_binding_size = RGB - 4;
    assert!(graph.compile_for_research(&limits).is_err());
}
