//! Structured editor subgraphs lower to the existing single-pass typed DAG.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn structured(document: &GraphDocument) -> bool {
    document.nodes.iter().any(|node| {
        matches!(
            node.operation,
            NodeOp::Group { .. } | NodeOp::GroupInput { .. } | NodeOp::GroupOutput { .. }
        )
    })
}
pub fn scope_topological_order(document: &GraphDocument) -> Result<Vec<String>> {
    let mut incoming = BTreeMap::new();
    for node in &document.nodes {
        if !motion_effects::valid_id(&node.id) || incoming.insert(node.id.clone(), 0usize).is_some()
        {
            return Err(Error::invalid(
                ErrorCode::DuplicateNode,
                Some(&node.id),
                None,
                "node IDs must be valid and unique in their scope",
            ));
        }
    }
    let mut outgoing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in &document.connections {
        if !incoming.contains_key(&edge.from.node) || !incoming.contains_key(&edge.to.node) {
            return Err(Error::invalid(
                ErrorCode::UnknownNode,
                Some(&edge.to.node),
                Some(&edge.to.port),
                "connection refers to a missing node",
            ));
        }
        *incoming.get_mut(&edge.to.node).unwrap() += 1;
        outgoing
            .entry(edge.from.node.clone())
            .or_default()
            .push(edge.to.node.clone());
    }
    let mut ready: BTreeSet<String> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut order = vec![];
    while let Some(id) = ready.pop_first() {
        for target in outgoing.get(&id).into_iter().flatten() {
            let count = incoming.get_mut(target).unwrap();
            *count -= 1;
            if *count == 0 {
                ready.insert(target.clone());
            }
        }
        order.push(id);
    }
    if order.len() != document.nodes.len() {
        return Err(Error::invalid(
            ErrorCode::Cycle,
            None,
            None,
            "group/scope connections contain a cycle",
        ));
    }
    Ok(order)
}
pub(crate) struct FlatGraph {
    pub document: GraphDocument,
    ids: BTreeMap<String, String>,
    aliases: BTreeMap<PortRef, PortRef>,
    incoming: BTreeMap<PortRef, PortRef>,
    /// Missing boundary connections deferred until a dependent leaf is required.
    missing_inputs: BTreeMap<PortRef, Diagnostic>,
    reserved: BTreeSet<String>,
    nodes: usize,
    edges: usize,
}
fn qualified(path: &str, id: &str) -> String {
    if path.is_empty() {
        id.into()
    } else {
        format!("{path}/{id}")
    }
}
impl FlatGraph {
    pub fn resolve(&self, target: &PortRef) -> Result<PortRef> {
        let mut source = target.clone();
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(source.clone()) {
                return Err(Error::invalid(
                    ErrorCode::Cycle,
                    Some(&source.node),
                    Some(&source.port),
                    "group boundary aliases form a cycle",
                ));
            }
            if let Some(id) = self.ids.get(&source.node) {
                return Ok(PortRef::new(id, &source.port));
            }
            let input = self.aliases.get(&source).ok_or_else(|| {
                Error::invalid(
                    ErrorCode::UnknownPort,
                    Some(&source.node),
                    Some(&source.port),
                    "group preview output is missing",
                )
            })?;
            source = self.incoming.get(input).cloned().ok_or_else(|| {
                Error::invalid(
                    ErrorCode::MissingInput,
                    Some(&input.node),
                    Some(&input.port),
                    "group boundary input is not connected",
                )
            })?;
        }
    }
    pub fn describe(&self, error: Error) -> Error {
        if let Error::Invalid(mut diagnostic) = error {
            if diagnostic.code == ErrorCode::MissingInput {
                if let (Some(node), Some(port)) = (&diagnostic.node, &diagnostic.port) {
                    if let Some(boundary) = self.missing_inputs.get(&PortRef::new(node, port)) {
                        return Error::Invalid(boundary.clone());
                    }
                }
            }
            if let Some(id) = &diagnostic.node {
                if let Some((qualified, _)) = self.ids.iter().find(|(_, value)| *value == id) {
                    diagnostic.node = Some(qualified.clone());
                }
            }
            Error::Invalid(diagnostic)
        } else {
            error
        }
    }
    fn collect(
        &mut self,
        scope: &GraphDocument,
        path: &str,
        depth: usize,
        options: &CompileOptions,
    ) -> Result<()> {
        if depth > 16 {
            return Err(Error::invalid(
                ErrorCode::Budget,
                Some(path),
                None,
                "node groups support at most 16 nested levels",
            ));
        }
        self.nodes += scope.nodes.len();
        self.edges += scope.connections.len();
        if self.nodes > 4096 || self.edges > 32768 {
            return Err(Error::invalid(
                ErrorCode::Budget,
                Some(path),
                None,
                "structured graph exceeds its authoring budget",
            ));
        }
        scope_topological_order(scope)?;
        let specs: BTreeMap<_, _> = scope
            .nodes
            .iter()
            .map(|node| {
                Ok((
                    node.id.clone(),
                    node_port_specs(node, &scope.public_parameters)?,
                ))
            })
            .collect::<Result<_>>()?;
        for edge in &scope.connections {
            let from_spec = specs[&edge.from.node]
                .outputs
                .get(&edge.from.port)
                .ok_or_else(|| {
                    Error::invalid(
                        ErrorCode::UnknownPort,
                        Some(&edge.from.node),
                        Some(&edge.from.port),
                        "unknown group/scope output",
                    )
                })?;
            let to_spec = specs[&edge.to.node]
                .inputs
                .get(&edge.to.port)
                .ok_or_else(|| {
                    Error::invalid(
                        ErrorCode::UnknownPort,
                        Some(&edge.to.node),
                        Some(&edge.to.port),
                        "unknown group/scope input",
                    )
                })?;
            if !ports_compatible(from_spec, to_spec) {
                return Err(Error::invalid(
                    ErrorCode::TypeMismatch,
                    Some(&edge.to.node),
                    Some(&edge.to.port),
                    "group/scope port types or units do not match",
                ));
            }
            let from = PortRef::new(qualified(path, &edge.from.node), &edge.from.port);
            let to = PortRef::new(qualified(path, &edge.to.node), &edge.to.port);
            if self.incoming.insert(to.clone(), from).is_some() {
                return Err(Error::invalid(
                    ErrorCode::DuplicateInput,
                    Some(&to.node),
                    Some(&to.port),
                    "each input has at most one connection",
                ));
            }
        }
        for node in &scope.nodes {
            let q = qualified(path, &node.id);
            if node.version != 1 {
                return Err(Error::invalid(
                    ErrorCode::NodeVersion,
                    Some(&q),
                    None,
                    "only node version 1 is implemented",
                ));
            }
            if !node.position.iter().all(|value| value.is_finite()) {
                return Err(Error::invalid(
                    ErrorCode::InvalidValue,
                    Some(&q),
                    Some("position"),
                    "node layout must be finite",
                ));
            }
            match &node.operation {
                NodeOp::Group { graph } => {
                    if graph.name.is_empty()
                        || graph.english_name.is_empty()
                        || graph.name.len() > 120
                        || graph.english_name.len() > 120
                    {
                        return Err(Error::invalid(
                            ErrorCode::InvalidValue,
                            Some(&q),
                            None,
                            "invalid bilingual group name",
                        ));
                    }
                    if graph
                        .nodes
                        .iter()
                        .filter(|n| matches!(n.operation, NodeOp::GroupInput { .. }))
                        .count()
                        != 1
                        || graph
                            .nodes
                            .iter()
                            .filter(|n| matches!(n.operation, NodeOp::GroupOutput { .. }))
                            .count()
                            != 1
                    {
                        return Err(Error::invalid(
                            ErrorCode::InvalidValue,
                            Some(&q),
                            None,
                            "a group requires one input and one output boundary node",
                        ));
                    }
                    for port in graph.ports(false)?.keys() {
                        self.aliases.insert(
                            PortRef::new(qualified(&q, &graph.input_node), port),
                            PortRef::new(&q, port),
                        );
                    }
                    for port in graph.ports(true)?.keys() {
                        self.aliases.insert(
                            PortRef::new(&q, port),
                            PortRef::new(qualified(&q, &graph.output_node), port),
                        );
                    }
                    let child = GraphDocument {
                        nodes: graph.nodes.clone(),
                        connections: graph.connections.clone(),
                        ..metadata(scope)
                    };
                    self.collect(&child, &q, depth + 1, options)?;
                }
                NodeOp::GroupInput { .. } | NodeOp::GroupOutput { .. } => {
                    if path.is_empty() {
                        return Err(Error::invalid(
                            ErrorCode::InvalidValue,
                            Some(&q),
                            None,
                            "group boundary nodes cannot appear at the root",
                        ));
                    }
                }
                _ => {
                    let mut leaf = node.clone();
                    if !path.is_empty() {
                        let mut number = self.ids.len();
                        loop {
                            let id = format!("group_leaf_{number}");
                            number += 1;
                            if self.reserved.insert(id.clone()) {
                                leaf.id = id;
                                break;
                            }
                        }
                    }
                    self.ids.insert(q, leaf.id.clone());
                    self.document.nodes.push(leaf);
                }
            }
        }
        Ok(())
    }
}
fn metadata(document: &GraphDocument) -> GraphDocument {
    GraphDocument {
        format_version: document.format_version,
        id: document.id.clone(),
        name: document.name.clone(),
        english_name: document.english_name.clone(),
        working_space: document.working_space,
        alpha_mode: document.alpha_mode,
        nodes: vec![],
        connections: vec![],
        public_parameters: document.public_parameters.clone(),
    }
}
pub(crate) fn flatten(document: &GraphDocument, options: &CompileOptions) -> Result<FlatGraph> {
    let mut flat = FlatGraph {
        document: metadata(document),
        ids: BTreeMap::new(),
        aliases: BTreeMap::new(),
        incoming: BTreeMap::new(),
        missing_inputs: BTreeMap::new(),
        reserved: document.nodes.iter().map(|node| node.id.clone()).collect(),
        nodes: 0,
        edges: 0,
    };
    flat.collect(document, "", 0, options)?;
    for (target, source) in &flat.incoming {
        if let Some(id) = flat.ids.get(&target.node) {
            let to = PortRef::new(id, &target.port);
            match flat.resolve(source) {
                Ok(from) => flat.document.connections.push(Connection { from, to }),
                Err(Error::Invalid(diagnostic)) if diagnostic.code == ErrorCode::MissingInput => {
                    // Keep the leaf input missing. Reachability validation will
                    // reject it if used, with the original group boundary cause.
                    // An authored wire still overrides the stored default even
                    // when its upstream group boundary is incomplete.
                    flat.document
                        .nodes
                        .iter_mut()
                        .find(|node| node.id == to.node)
                        .unwrap()
                        .input_defaults
                        .remove(&to.port);
                    flat.missing_inputs.insert(to, diagnostic);
                }
                Err(error) => return Err(error),
            }
        }
    }
    // Inspect every alias for malformed structure/cycles, but an unconnected
    // boundary is a draft until an output or an explicit preview depends on it.
    for port in flat.aliases.keys() {
        match flat.resolve(port) {
            Ok(_) => {}
            Err(Error::Invalid(diagnostic)) if diagnostic.code == ErrorCode::MissingInput => {}
            Err(error) => return Err(error),
        }
    }
    Ok(flat)
}
