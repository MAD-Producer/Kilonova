use serde_json::Value;
use std::sync::Arc;

pub(super) fn get<T>(
    cache: &mut Option<((String, String, u64), Arc<T>)>,
    state: &Value,
    build: impl FnOnce() -> T,
) -> Arc<T> {
    let root = state["root"].as_str().unwrap_or("");
    let composition = state["composition"].as_str().unwrap_or("");
    let revision = state["revision"].as_u64().unwrap_or(0);
    if let Some((key, rows)) = cache.as_ref() {
        if key.0 == root && key.1 == composition && key.2 == revision {
            return Arc::clone(rows);
        }
    }
    // Only invalidation needs owned identity strings and rebuilt row storage.
    let rows = Arc::new(build());
    *cache = Some((
        (root.to_owned(), composition.to_owned(), revision),
        Arc::clone(&rows),
    ));
    rows
}
