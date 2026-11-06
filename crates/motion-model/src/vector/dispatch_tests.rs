//! Internal invariant injection, not a claim that a public edit can corrupt its wrapper.
use super::*;

#[test]
fn invalid_flat_wrapper_returns_native_error_without_publishing_partial_contents() {
    for empty in [true, false] {
        let mut vector = VectorContent::shape(ShapeKind::Rectangle);
        let before = vector.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vector.edit_contents([64.; 2], true, |group| {
                group.items.clear();
                if !empty {
                    group.items.push(groups::GroupItem::Fill {
                        id: 99,
                        name: "Injected non-geometry".into(),
                        color: Track::constant([1.; 4]),
                        fill_rule: FillRule::NonZero,
                        composite: groups::Composite::Below,
                    });
                }
                Ok(())
            })
        }));
        let error = result
            .expect("a broken private wrapper must return an error")
            .unwrap_err();
        assert!(matches!(error, crate::Error::Invalid(ref cause)
            if cause == "flat vector contents require a first geometry item"));
        assert_eq!(vector, before, "failed temporary contents must not escape");
    }
}

#[test]
fn healthy_flat_wrapper_preserves_authored_vector_and_edit_errors() {
    let mut vector = VectorContent::shape(ShapeKind::Rectangle);
    let before = vector.clone();
    vector.edit_contents([64.; 2], true, |_| Ok(())).unwrap();
    assert_eq!(vector, before);
    let error = vector
        .edit_contents([64.; 2], true, |_| {
            Err(crate::Error::Invalid("authored edit failure".into()))
        })
        .unwrap_err();
    assert!(matches!(error, crate::Error::Invalid(ref cause) if cause == "authored edit failure"));
    assert_eq!(vector, before);
}
