use motion_model::{node_resources::source_resources, NodeResource};
use serde_json::json;

fn image() -> serde_json::Value {
    let id = "a".repeat(64);
    json!({"format_version":2,"key":"source","document":{"images":{id.clone():{
        "width":2,"height":2,"resource":NodeResource{path:format!("assets/node-{id}.png"), bytes:80, sha256:id}
    }}},"options":{}})
}
#[test]
fn node_source_accepts_only_bounded_owned_references_and_current_format() {
    let valid = image();
    let id = "a".repeat(64);
    assert_eq!(source_resources(&valid).unwrap().len(), 1);
    let mut candidates = vec![];
    let mut old = valid.clone();
    old["format_version"] = 1.into();
    candidates.push(old);
    let mut payload = valid.clone();
    payload["document"]["images"][&id]["png"] = "unexpected payload".into();
    candidates.push(payload);
    let mut path = valid.clone();
    path["document"]["images"][&id]["resource"]["path"] = "../foreign.png".into();
    candidates.push(path);
    let mut wrong = valid.clone();
    wrong["document"]["images"][&id]["resource"]["sha256"] = "b".repeat(64).into();
    candidates.push(wrong);
    let mut overflow = valid.clone();
    overflow["document"]["images"][&id]["width"] = u32::MAX.into();
    overflow["document"]["images"][&id]["height"] = u32::MAX.into();
    candidates.push(overflow);
    let mut budget = valid.clone();
    budget["document"]["images"][&id]["resource"]["bytes"] = (16 * 1024 * 1024).into();
    candidates.push(budget);
    let mut runtime = valid;
    runtime["runtime"] = json!({"package_hex":"0000"});
    candidates.push(runtime);
    for candidate in candidates {
        assert!(
            source_resources(&candidate).is_err(),
            "accepted invalid source: {candidate}"
        );
    }
}

#[test]
fn image_pool_count_and_decoded_sum_are_checked_without_reading_payloads() {
    let mut source = image();
    let id = "a".repeat(64);
    source["document"]["images"][&id]["width"] = 4096.into();
    source["document"]["images"][&id]["height"] = 4096.into();
    assert!(source_resources(&source).is_ok());
    let second = "b".repeat(64);
    source["document"]["images"][&second] = json!({"width":4096,"height":4097,"resource":{
        "path":format!("assets/node-{second}.png"),"bytes":80,"sha256":second
    }});
    assert!(source_resources(&source).is_err());
    let mut many = serde_json::Map::new();
    for i in 0..33 {
        let id = format!("{i:064x}");
        many.insert(
            id.clone(),
            json!({"width":1,"height":1,"resource":{
                "path":format!("assets/node-{id}.png"),"bytes":80,"sha256":id
            }}),
        );
    }
    source["document"]["images"] = many.into();
    assert!(source_resources(&source).is_err());
}
