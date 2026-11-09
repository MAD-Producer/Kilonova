//! Strict batch admission controls; actual GPU reuse needs separate A/B/A evidence.
#[path = "../admission.rs"]
mod admission;
#[path = "../batch_admission.rs"]
mod batch_admission;
#[path = "../convolution_admission.rs"]
mod convolution_admission;
#[path = "../index_admission.rs"]
mod index_admission;
use batch_admission::{BatchRequest, MAX_INPUTS};
use serde_json::{Value, json};
const PROGRAM: &str = "authored-control-identity";
fn request(count: usize) -> Value {
    let input = json!({"name":"rgb","dtype":"float32","shape":[1,3,518,518],
        "file":"rgb.f32","sha256":"0".repeat(64)});
    json!({"version":1,"program":"control.json","program_sha256":PROGRAM,
        "requested_backend":"vulkan","inputs":vec![input;count]})
}
fn validate(value: Value) -> Result<(), String> {
    serde_json::from_value::<BatchRequest>(value)
        .map_err(|e| e.to_string())?
        .validate(PROGRAM)
}
#[test]
fn bounded_batch_preserves_order_and_repeated_inputs() {
    let mut value = request(MAX_INPUTS);
    value["inputs"][1]["file"] = "other.f32".into();
    let parsed: BatchRequest = serde_json::from_value(value).unwrap();
    parsed.validate(PROGRAM).unwrap();
    assert_eq!(parsed.inputs[0].file, parsed.inputs[2].file);
    assert_ne!(parsed.inputs[0].file, parsed.inputs[1].file);
    assert!(validate(request(1)).is_ok());
}
#[test]
fn invalid_batch_cannot_select_an_implicit_or_different_execution() {
    assert!(validate(request(0)).is_err());
    assert!(validate(request(MAX_INPUTS + 1)).is_err());
    for field in ["version", "program_sha256", "requested_backend"] {
        let mut value = request(1);
        value[field] = if field == "version" {
            json!(2)
        } else {
            json!("different")
        };
        assert!(validate(value).is_err(), "{field}");
    }
    for field in ["name", "dtype", "shape"] {
        let mut value = request(2);
        value["inputs"][1][field] = if field == "shape" {
            json!([1, 3, 266, 266])
        } else {
            json!("wrong")
        };
        assert!(validate(value).is_err(), "{field}");
    }
    let mut value = request(1);
    value["ignore_failure"] = true.into();
    assert!(validate(value).is_err());
    let mut value = request(1);
    value["program"] = "x".repeat(4097).into();
    assert!(validate(value).is_err());
}
