//! The author must preserve the entire host state. The sole exception is the
//! lifetime diagnostic contention counter: worker and synchronous wait timing
//! can legitimately contend even while reading State. It must never decrease.
use serde_json::Value;

const DROPS: &str = "/desktop/diagnostics/droppedRecords";

pub(super) fn compare(before: &Value, after: &Value) -> Result<[u64; 2], &'static str> {
    let previous = before
        .pointer(DROPS)
        .and_then(Value::as_u64)
        .ok_or("missing or invalid previous diagnostic drop count")?;
    let current = after
        .pointer(DROPS)
        .and_then(Value::as_u64)
        .ok_or("missing or invalid current diagnostic drop count")?;
    if current < previous {
        return Err("diagnostic drop count decreased");
    }
    let mut checked = after.clone();
    *checked
        .pointer_mut(DROPS)
        .ok_or("missing diagnostic drop count")? = previous.into();
    if &checked != before {
        return Err("host state changed outside diagnostic contention count");
    }
    Ok([previous, current])
}

#[test]
fn isolation_accepts_only_monotonic_contention_observations() {
    let before = fixture();
    assert_eq!(compare(&before, &before), Ok([2, 2]));
    let mut after = before.clone();
    *after.pointer_mut(DROPS).unwrap() = 8.into();
    assert_eq!(compare(&before, &after), Ok([2, 8]));
    assert!(compare(&after, &before).is_err());
}

#[test]
fn isolation_rejects_missing_malformed_or_reset_counters() {
    let before = fixture();
    for invalid in [
        Value::Null,
        (-1).into(),
        1.5.into(),
        "2".into(),
        false.into(),
        1.into(),
    ] {
        let mut after = before.clone();
        *after.pointer_mut(DROPS).unwrap() = invalid;
        assert!(compare(&before, &after).is_err());
    }
    let mut missing = before.clone();
    missing["desktop"]["diagnostics"]
        .as_object_mut()
        .unwrap()
        .remove("droppedRecords");
    assert!(compare(&before, &missing).is_err());
    assert!(compare(&missing, &before).is_err());
}

// Also used with the actual full native snapshot, rather than just this unit
// fixture. Every other leaf, removal and new key must still fail isolation.
pub(super) fn reject_mutations(before: &Value) {
    fn visit(before: &Value, node: &Value, path: &str) {
        if path == DROPS {
            return;
        }
        let mut changed = before.clone();
        *changed.pointer_mut(path).unwrap() = if node.is_null() {
            true.into()
        } else {
            Value::Null
        };
        assert!(
            compare(before, &changed).is_err(),
            "mutation accepted: {path}"
        );
        match node {
            Value::Object(fields) => {
                let mut added = before.clone();
                added
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert("isolationNegativeControl".into(), true.into());
                assert!(compare(before, &added).is_err(), "new key accepted: {path}");
                for (key, value) in fields {
                    let mut removed = before.clone();
                    removed
                        .pointer_mut(path)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .remove(key);
                    assert!(
                        compare(before, &removed).is_err(),
                        "removal accepted: {path}/{key}"
                    );
                    let key = key.replace('~', "~0").replace('/', "~1");
                    visit(before, value, &format!("{path}/{key}"));
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    visit(before, value, &format!("{path}/{index}"));
                }
            }
            _ => {}
        }
    }
    visit(before, before, "");
}

#[test]
fn isolation_rejects_every_other_mutation_addition_and_removal() {
    reject_mutations(&fixture());
}

fn fixture() -> Value {
    serde_json::json!({
        "project": {"width":640, "layers":[{"id":7,"name":"host","effects":[]}]},
        "frame":12.5, "revision":3, "history":{"canUndo":true,"canRedo":false},
        "desktop":{"diagnostics":{"droppedRecords":2,"droppedJobs":0,
            "localWriteErrors":0,"queuedUploads":0,"configured":false,"enabled":false}},
        "optional":null, "escaped/key~":{"value":1}
    })
}
