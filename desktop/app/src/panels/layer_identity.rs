//! A layer supplied to these property renderers must carry a u64 ID. Each
//! renderer instance counts consecutive malformed inputs, saturates at 60 and
//! emits once at that threshold. A valid ID (including legacy camera ID zero)
//! resets the counter. No user-controlled text is retained or logged.
use serde_json::Value;

#[derive(Default)]
pub(super) struct Reader {
    failures: u8,
    last_object: Option<u64>,
}

fn kind(value: Option<&Value>) -> &'static str {
    match value {
        None => "missing",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "boolean",
        Some(Value::Number(_)) => "number",
        Some(Value::String(_)) => "string",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
    }
}

impl Reader {
    pub(super) fn read(&mut self, layer: &Value, state: &Value, site: &'static str) -> Option<u64> {
        if let Some(object) = layer.get("id").and_then(Value::as_u64) {
            self.failures = 0;
            self.last_object = Some(object);
            return Some(object);
        }
        let previous = self.failures;
        self.failures = previous.saturating_add(1).min(60);
        if previous == 0 {
            log::debug!("panel degradation site={site} count=1 operation=render phase=identity last_object={:?} layer_type={} id_type={} generation={:?} revision={:?} cause=missing or non-u64 layer ID",
                self.last_object, kind(Some(layer)), kind(layer.get("id")),
                state["document_generation"].as_u64(), state["revision"].as_u64());
        } else if previous == 59 {
            log::error!("persistent panel degradation site={site} count=60 operation=render phase=identity last_object={:?} layer_type={} id_type={} generation={:?} revision={:?} cause=missing or non-u64 layer ID",
                self.last_object, kind(Some(layer)), kind(layer.get("id")),
                state["document_generation"].as_u64(), state["revision"].as_u64());
        }
        None
    }
}

#[cfg(test)]
mod tests;
