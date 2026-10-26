//! Stable executable order for a graph already checked by validate_root.
//! Authoring IDs and grouping do not determine shader temporary identities.
use crate::GraphDocument;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn emission_order(document: &GraphDocument, root: &str) -> Vec<String> {
    let mut incoming: BTreeMap<&str, BTreeMap<&str, &str>> = BTreeMap::new();
    for edge in &document.connections {
        incoming
            .entry(&edge.to.node)
            .or_default()
            .insert(&edge.to.port, &edge.from.node);
    }
    let mut complete = BTreeSet::new();
    let mut pending = vec![(root, false)];
    let mut order = Vec::with_capacity(document.nodes.len());
    // Iterative traversal retains the validated node/edge bounds without
    // consuming one call-stack frame per nested operation.
    while let Some((node, expanded)) = pending.pop() {
        if complete.contains(node) {
            continue;
        }
        if expanded {
            complete.insert(node);
            order.push(node.to_owned());
        } else {
            pending.push((node, true));
            if let Some(ports) = incoming.get(node) {
                for dependency in ports.values().rev() {
                    if !complete.contains(dependency) {
                        pending.push((dependency, false));
                    }
                }
            }
        }
    }
    order
}
