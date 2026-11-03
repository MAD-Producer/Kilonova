//! Borrow catalogue data for the duration of a draw instead of copying its tree.
use serde_json::Value;

pub(crate) fn find<'a>(catalogue: &'a Value, effect: &Value) -> Option<&'a Value> {
    catalogue["packages"].as_array()?.iter().find(|package| {
        package["hash"] == effect["hash"]
            && package["manifest"]["id"] == effect["plugin"]
            && package["manifest"]["version"] == effect["version"]
    })?["manifest"]["effects"]
        .as_array()?
        .iter()
        .find(|definition| definition["id"] == effect["effect"])
}

/// The previous stable sort moved curves last, preserving both groups' order.
pub(super) fn parameters(definition: &Value) -> impl Iterator<Item = &Value> {
    let values = definition["params"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    values
        .iter()
        .filter(|parameter| parameter["kind"] != "curve")
        .chain(
            values
                .iter()
                .filter(|parameter| parameter["kind"] == "curve"),
        )
}
