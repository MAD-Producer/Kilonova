use crate::GraphDocument;

/// The checked-in editable source is also the canonical programmatic demo.
pub fn demo_graph() -> GraphDocument {
    GraphDocument::from_json(include_str!("../examples/drifting-circle.msgraph"))
        .expect("validated N1 demo source")
}
