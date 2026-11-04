//! The MCP shape shares binding payloads with the core command catalogue.
use serde_json::{json, Value};

pub(super) fn schema() -> Value {
    let request = json!({"type":"string","minLength":1,"maxLength":128});
    let path = json!({"type":"string","minLength":1,"maxLength":4096});
    let target = json!({"type":"object","additionalProperties":false,"required":["kind","id"],
        "properties":{"kind":{"const":"video"},"id":{"type":"integer","minimum":1}}});
    let revision = json!({"type":"integer","minimum":0});
    let bindings: Vec<_> = motion_core::CommandRegistry::input_schemas()
        .into_iter()
        .filter_map(|input| {
            let kind = match input["properties"]["op"]["const"].as_str()? {
                "set_tracking_point" => "point",
                "set_tracking_plane" => "plane",
                "set_tracking_camera" => "camera",
                _ => return None,
            };
            Some(super::schema(
                json!({"kind":{"const":kind},
            "composition":{"type":"string","minLength":1,"maxLength":128},
            "object":{"type":"integer","minimum":0},"binding":input["properties"]["binding"]}),
                &["kind", "composition", "object", "binding"],
            ))
        })
        .collect();
    let mut actions = vec![
        super::schema(
            json!({"action":{"const":"prepare"},"requestId":request,"target":target,"path":path,
            "expectedRevision":revision,"expectedDocumentGeneration":revision,"withAudio":{"type":"boolean"},
            "track":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0,"maximum":u32::MAX}]},
            "audioTrack":{"anyOf":[{"type":"null"},{"type":"integer","minimum":0,"maximum":u32::MAX}]}}),
            &[
                "action",
                "requestId",
                "target",
                "path",
                "expectedRevision",
                "expectedDocumentGeneration",
                "withAudio",
            ],
        ),
        super::schema(
            json!({"action":{"const":"finish"},"requestId":request,"target":target,
            "expectedRevision":revision,"expectedDocumentGeneration":revision,
            "solves":{"type":"array","minItems":1,"maxItems":motion_core::tracking_assets::MAX_TRACKING_ASSETS,
                "items":super::schema(json!({"asset":{"type":"integer","minimum":1},"path":path}),&["asset","path"])},
            "bindings":{"type":"array","maxItems":motion_core::tracking_assets::replacement::MAX_BINDING_UPDATES,
                "items":{"oneOf":bindings}}}),
            &[
                "action",
                "requestId",
                "target",
                "expectedRevision",
                "expectedDocumentGeneration",
                "solves",
                "bindings",
            ],
        ),
    ];
    for action in ["status", "cancel", "release"] {
        actions.push(super::schema(
            json!({"action":{"const":action},"requestId":request}),
            &["action", "requestId"],
        ));
    }
    json!({"type":"object","oneOf":actions})
}

#[cfg(test)]
#[path = "tracked_video_tests.rs"]
mod tests;
