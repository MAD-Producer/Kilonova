use motion_graph::Node;
use std::collections::HashMap;

/// Borrow only this paint pass's document. No node, ID, or operation is cloned
/// or retained across edits, group navigation, undo, or document replacement.
pub(super) struct NodeIndex<'a> {
    source: &'a [Node],
    nodes: Option<HashMap<&'a str, &'a Node>>,
    linear_queries: u8,
}

impl<'a> NodeIndex<'a> {
    pub(super) fn new(nodes: &'a [Node]) -> Self {
        Self {
            source: nodes,
            nodes: None,
            linear_queries: 0,
        }
    }

    pub(super) fn find(&mut self, id: &str) -> Option<&'a Node> {
        // Small/mostly culled scenes keep the original allocation-free search.
        // Bound the linear prefix; larger visible sets build one local index.
        if self.linear_queries < 32 {
            self.linear_queries += 1;
            return self.source.iter().find(|node| node.id == id);
        }
        self.nodes
            .get_or_insert_with(|| {
                let mut index = HashMap::with_capacity(self.source.len());
                for node in self.source {
                    // Preserve .find()'s first match for malformed duplicate IDs.
                    index.entry(node.id.as_str()).or_insert(node);
                }
                index
            })
            .get(id)
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motion_graph::NodeOp;
    use std::{hint::black_box, time::Instant};

    #[test]
    fn lookup_preserves_first_identity_for_duplicate_reordered_and_missing_ids() {
        let nodes: Vec<_> = ["duplicate", "中文", "", "duplicate", "a\0b"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| Node::new(id, NodeOp::Float { value: i as f32 }))
            .collect();
        let mut index = NodeIndex::new(&nodes);
        // Queries need not follow document/card order and may be a subset.
        for id in ["a\0b", "duplicate", "", "中文", "missing", "duplicate"]
            .into_iter()
            .cycle()
            .take(72)
        {
            let expected = nodes.iter().find(|node| node.id == id);
            let actual = index.find(id);
            assert_eq!(actual.is_some(), expected.is_some());
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!(std::ptr::eq(actual, expected));
            }
        }
        assert!(std::ptr::eq(index.find("duplicate").unwrap(), &nodes[0]));
        assert!(index.nodes.is_some());
        assert!(NodeIndex::new(&[]).find("").is_none());
    }

    #[test]
    fn fresh_pass_sees_removed_replaced_and_reordered_nodes() {
        let mut nodes = vec![
            Node::new("a", NodeOp::Float { value: 1. }),
            Node::new("b", NodeOp::Bool { value: true }),
        ];
        assert!(std::ptr::eq(
            NodeIndex::new(&nodes).find("a").unwrap(),
            &nodes[0]
        ));
        nodes.swap(0, 1);
        nodes.retain(|node| node.id != "b");
        nodes[0].operation = NodeOp::Float { value: 7. };
        let mut index = NodeIndex::new(&nodes);
        assert!(index.find("b").is_none());
        assert!(matches!(
            index.find("a").unwrap().operation,
            NodeOp::Float { value: 7. }
        ));
        let replacement = vec![Node::new("a", NodeOp::Bool { value: false })];
        assert!(std::ptr::eq(
            NodeIndex::new(&replacement).find("a").unwrap(),
            &replacement[0]
        ));
    }

    #[test]
    fn small_visible_sets_and_normal_misses_do_not_build_a_table() {
        let nodes: Vec<_> = (0..2048)
            .map(|i| Node::new(format!("node-{i}"), NodeOp::Float { value: i as f32 }))
            .collect();
        let mut index = NodeIndex::new(&nodes);
        for i in 0..32 {
            let id = if i % 2 == 0 { "node-2047" } else { "missing" };
            assert_eq!(index.find(id).is_some(), i % 2 == 0);
            assert!(index.nodes.is_none());
        }
        assert!(std::ptr::eq(index.find("node-2047").unwrap(), &nodes[2047]));
        assert!(index.nodes.is_some());
        for _ in 0..300 {
            assert!(index.find("missing").is_none());
        }
        assert_eq!(index.linear_queries, 32);
        // A fresh paint never retains the previous table or its query count.
        let fresh = NodeIndex::new(&nodes);
        assert!(fresh.nodes.is_none());
        assert_eq!(fresh.linear_queries, 0);
    }

    #[test]
    fn real_node_lookup_scaling_matches_linear_identity() {
        for count in [128, 512, 2048] {
            let nodes: Vec<_> = (0..count)
                .map(|i| Node::new(format!("node-{i}"), NodeOp::Float { value: i as f32 }))
                .collect();
            let start = Instant::now();
            let mut comparisons = 0usize;
            let expected: Vec<_> = nodes
                .iter()
                .rev()
                .map(|query| {
                    nodes
                        .iter()
                        .find(|node| {
                            comparisons += 1;
                            node.id == query.id
                        })
                        .unwrap()
                })
                .collect();
            let linear_time = start.elapsed();
            let start = Instant::now();
            let mut index = NodeIndex::new(&nodes);
            let actual: Vec<_> = nodes
                .iter()
                .rev()
                .map(|query| black_box(index.find(black_box(&query.id)).unwrap()))
                .collect();
            let indexed_time = start.elapsed();
            for (actual, expected) in actual.into_iter().zip(expected) {
                assert!(std::ptr::eq(actual, expected));
            }
            assert_eq!(comparisons, count * (count + 1) / 2);
            println!("nodes={count} queries={count} linear_comparisons={comparisons} linear_us={} index_build_and_lookup_us={}",
                linear_time.as_micros(), indexed_time.as_micros());
        }
    }
}
