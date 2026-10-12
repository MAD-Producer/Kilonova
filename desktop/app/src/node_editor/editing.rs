//! Atomic subgraph edits shared by pointer and keyboard interactions.
use super::*;

impl NodeEditor {
    pub fn disconnect_inputs(&mut self, inputs: &BTreeSet<PortRef>) -> bool {
        if !self
            .document
            .connections
            .iter()
            .any(|edge| inputs.contains(&edge.to))
        {
            return false;
        }
        let before = self.state(true);
        self.document
            .connections
            .retain(|edge| !inputs.contains(&edge.to));
        self.finish_edit(before);
        true
    }
    pub fn duplicate_selected(&mut self) -> Result<bool, Diagnostic> {
        let mut nodes: Vec<_> = self
            .document
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id) && !grouping::boundary(&n.operation))
            .cloned()
            .collect();
        if nodes.is_empty() {
            return Ok(false);
        }
        if nodes
            .iter()
            .any(|n| grouping::output_count(&n.operation) > 0)
        {
            return Err(diag(
                ErrorCode::MultipleOutputs,
                None,
                None,
                "The graph supports one Color Output; exclude it before duplicating",
            ));
        }
        let copied_ids: BTreeSet<_> = nodes.iter().map(|node| node.id.clone()).collect();
        let edges: Vec<_> = self
            .document
            .connections
            .iter()
            .filter(|e| copied_ids.contains(&e.from.node) && copied_ids.contains(&e.to.node))
            .cloned()
            .collect();
        if self.document.nodes.len() + nodes.len() > self.options.max_nodes
            || self.document.connections.len() + edges.len() > self.options.max_connections
        {
            return Err(diag(
                ErrorCode::Budget,
                None,
                None,
                "Duplicated subgraph exceeds graph budget",
            ));
        }
        let before = self.state(true);
        let mut ids = BTreeMap::new();
        for node in &mut nodes {
            let new_id = self.id(node.operation.kind());
            ids.insert(node.id.clone(), new_id.clone());
            node.id = new_id;
            node.position = node.position.map(|v| (v + 32.).clamp(-1.0e6, 1.0e6));
        }
        let mut candidate = self.document.clone();
        candidate.connections.extend(edges.into_iter().map(|mut e| {
            e.from.node = ids[&e.from.node].clone();
            e.to.node = ids[&e.to.node].clone();
            e
        }));
        candidate.nodes.extend(nodes);
        self.checked_structure(&candidate)?;
        self.document = candidate;
        self.primary = self.primary.as_ref().and_then(|id| ids.get(id).cloned());
        self.selected = ids.into_values().collect();
        self.primary = self
            .primary
            .take()
            .or_else(|| self.selected.iter().next().cloned());
        self.finish_edit(before);
        Ok(true)
    }

    pub fn move_selected(&mut self, delta: [f32; 2]) -> bool {
        let before = self.state(false);
        for node in &mut self.document.nodes {
            if self.selected.contains(&node.id) {
                node.position = [
                    (node.position[0] + delta[0]).clamp(-1.0e6, 1.0e6),
                    (node.position[1] + delta[1]).clamp(-1.0e6, 1.0e6),
                ];
            }
        }
        if self.document == before.document {
            return false;
        }
        self.finish_edit(before);
        true
    }

    pub fn arrange_selected(&mut self) -> bool {
        let mut remaining: BTreeSet<_> = self
            .document
            .nodes
            .iter()
            .filter(|n| self.selected.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        if remaining.is_empty() {
            return false;
        }
        let edges: Vec<_> = self
            .document
            .connections
            .iter()
            .filter(|e| remaining.contains(&e.from.node) && remaining.contains(&e.to.node))
            .collect();
        let mut levels = BTreeMap::new();
        while !remaining.is_empty() {
            let ready = remaining
                .iter()
                .find(|id| {
                    !edges
                        .iter()
                        .any(|e| &e.to.node == *id && remaining.contains(&e.from.node))
                })
                .cloned();
            let Some(id) = ready else {
                return false;
            };
            let level = edges
                .iter()
                .filter(|e| e.to.node == id)
                .map(|e| levels.get(&e.from.node).copied().unwrap_or(0) + 1)
                .max()
                .unwrap_or(0);
            remaining.remove(&id);
            levels.insert(id, level);
        }
        let before = self.state(false);
        let origin = self
            .document
            .nodes
            .iter()
            .filter(|n| levels.contains_key(&n.id))
            .fold([f32::INFINITY; 2], |p, n| {
                [p[0].min(n.position[0]), p[1].min(n.position[1])]
            });
        let mut rows = BTreeMap::<usize, f32>::new();
        let heights: BTreeMap<_, _> = self
            .document
            .nodes
            .iter()
            .map(|n| (n.id.clone(), view::node_height(n, &self.document)))
            .collect();
        for node in &mut self.document.nodes {
            if let Some(level) = levels.get(&node.id) {
                let y = rows.entry(*level).or_default();
                node.position = [
                    origin[0] + *level as f32 * (view::NODE_WIDTH + 50.),
                    origin[1] + *y,
                ];
                *y += heights[&node.id] + 34.;
            }
        }
        if self.document == before.document {
            return false;
        }
        self.finish_edit(before);
        true
    }
}
