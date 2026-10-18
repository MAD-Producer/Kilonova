use super::*;
use motion_graph::{GroupPort, NodeGroup};

#[derive(Clone)]
pub(super) struct ParentScope {
    pub document: GraphDocument,
    pub group: String,
    pub view: CanvasView,
    pub selection: BTreeSet<String>,
    pub primary: Option<String>,
}
pub(super) fn boundary(operation: &NodeOp) -> bool {
    matches!(
        operation,
        NodeOp::GroupInput { .. } | NodeOp::GroupOutput { .. }
    )
}
pub(super) fn output_count(operation: &NodeOp) -> usize {
    match operation {
        NodeOp::ColorOutput => 1,
        NodeOp::Group { graph } => graph
            .nodes
            .iter()
            .map(|node| output_count(&node.operation))
            .sum(),
        _ => 0,
    }
}
fn costs(nodes: &[Node], edges: &[Connection]) -> (usize, usize) {
    let mut total = (
        0,
        edges
            .iter()
            .filter(|edge| {
                nodes.iter().any(|node| {
                    node.id == edge.to.node
                        && !boundary(&node.operation)
                        && !matches!(node.operation, NodeOp::Group { .. })
                })
            })
            .count(),
    );
    for node in nodes {
        if let NodeOp::Group { graph } = &node.operation {
            let child = costs(&graph.nodes, &graph.connections);
            total.0 += child.0;
            total.1 += child.1;
        } else if !boundary(&node.operation) {
            total.0 += 1;
        }
    }
    total
}
fn named_port(node: &str, spec: &motion_graph::PortSpec) -> GroupPort {
    let trim = |mut text: String| {
        while text.len() > 120 {
            text.pop();
        }
        text
    };
    GroupPort {
        name: trim(format!("{node} · {}", spec.name)),
        english_name: trim(format!("{node} · {}", spec.english_name)),
        value_type: spec.value_type,
        semantic: spec.semantic,
    }
}
impl NodeEditor {
    pub(super) fn qualified_node(&self, id: &str) -> String {
        self.parents
            .iter()
            .map(|parent| parent.group.as_str())
            .chain(std::iter::once(id))
            .collect::<Vec<_>>()
            .join("/")
    }
    pub(super) fn root_document(&self) -> GraphDocument {
        self.root_with(&self.document)
    }
    pub(super) fn root_with(&self, working: &GraphDocument) -> GraphDocument {
        let mut current = working.clone();
        for parent in self.parents.iter().rev() {
            let mut outer = parent.document.clone();
            if let Some(Node {
                operation: NodeOp::Group { graph },
                ..
            }) = outer.nodes.iter_mut().find(|node| node.id == parent.group)
            {
                graph.nodes = current.nodes;
                graph.connections = current.connections;
                graph.name = current.name;
                graph.english_name = current.english_name;
            }
            outer.public_parameters = current.public_parameters;
            current = outer;
        }
        current
    }
    pub(super) fn restore_history(&mut self, history: &History) {
        if self
            .parents
            .iter()
            .map(|parent| &parent.group)
            .collect::<Vec<_>>()
            != history
                .parents
                .iter()
                .map(|parent| &parent.group)
                .collect::<Vec<_>>()
        {
            self.view = history.view;
            self.selected = history.selection.clone();
            self.primary = history.primary.clone();
        }
        self.parents = history.parents.clone();
        self.document = history.document.clone();
        self.options = history.options.clone();
        self.first_frame = false;
    }
    pub fn enter_group(&mut self, id: &str) -> bool {
        if self.editing() || self.parents.len() >= 16 {
            self.trace_noop("enter_group", "editing_or_depth_limit");
            return false;
        }
        let Some(Node {
            operation: NodeOp::Group { graph },
            ..
        }) = self.document.nodes.iter().find(|node| node.id == id)
        else {
            self.trace_noop("enter_group", "target_is_not_a_group");
            return false;
        };
        let graph = graph.clone();
        let mut child = self.document.clone();
        child.nodes = graph.nodes;
        child.connections = graph.connections;
        child.name = graph.name;
        child.english_name = graph.english_name;
        let mut parent_document = self.document.clone();
        if let Some(Node {
            operation: NodeOp::Group { graph },
            ..
        }) = parent_document.nodes.iter_mut().find(|node| node.id == id)
        {
            graph.nodes.clear();
            graph.connections.clear();
        }
        self.parents.push(ParentScope {
            document: parent_document,
            group: id.into(),
            view: self.view,
            selection: self.selected.clone(),
            primary: self.primary.clone(),
        });
        self.document = child;
        self.selected.clear();
        self.primary = None;
        self.last_group_click = None;
        self.preview_target = None;
        self.advance_preview();
        self.fit_all(self.last_canvas);
        self.first_frame = false;
        self.notice = None;
        true
    }
    pub fn leave_group(&mut self) -> bool {
        if self.editing() {
            self.trace_noop("leave_group", "editing");
            return false;
        }
        let Some(parent) = self.parents.pop() else {
            self.trace_noop("leave_group", "already_at_root");
            return false;
        };
        let mut outer = parent.document;
        if let Some(Node {
            operation: NodeOp::Group { graph },
            ..
        }) = outer.nodes.iter_mut().find(|node| node.id == parent.group)
        {
            graph.nodes = self.document.nodes.clone();
            graph.connections = self.document.connections.clone();
            graph.name = self.document.name.clone();
            graph.english_name = self.document.english_name.clone();
        }
        outer.public_parameters = self.document.public_parameters.clone();
        self.document = outer;
        self.view = parent.view;
        self.selected = parent.selection;
        self.primary = parent.primary;
        self.last_group_click = None;
        self.preview_target = None;
        self.advance_preview();
        self.first_frame = false;
        self.notice = None;
        true
    }
    pub(super) fn checked_structure(&self, document: &GraphDocument) -> Result<(), Diagnostic> {
        let root = self.root_with(document);
        self.check_nested_budget(document)?;
        match motion_graph::validate(&root, &self.options) {
            Ok(_) => Ok(()),
            Err(error)
                if matches!(
                    error.diagnostic().code,
                    ErrorCode::MissingInput | ErrorCode::MissingOutput
                ) =>
            {
                Ok(())
            }
            Err(error) => Err(error.diagnostic()),
        }
    }
    pub(super) fn check_nested_budget(&self, document: &GraphDocument) -> Result<(), Diagnostic> {
        let root = self.root_with(document);
        let (nodes, edges) = costs(&root.nodes, &root.connections);
        if nodes > self.options.max_nodes || edges > self.options.max_connections {
            return Err(diag(
                ErrorCode::Budget,
                None,
                None,
                "Nested graph exceeds its node/connection budget",
            ));
        }
        Ok(())
    }
    pub fn group_selected(&mut self) -> Result<bool, Diagnostic> {
        if self.editing() {
            self.trace_noop("group_selected", "editing");
            return Ok(false);
        }
        let mut members: Vec<_> = self
            .document
            .nodes
            .iter()
            .filter(|node| self.selected.contains(&node.id) && !boundary(&node.operation))
            .cloned()
            .collect();
        if members.is_empty() {
            self.trace_noop("group_selected", "no_eligible_selection");
            return Ok(false);
        }
        if members.iter().any(|node| boundary(&node.operation)) {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Boundary nodes must stay in their own group",
            ));
        }
        let ids: BTreeSet<_> = members.iter().map(|node| node.id.clone()).collect();
        let id = self.id("group");
        let local_id = |base: &str| {
            let mut index = 0;
            loop {
                let id = format!("{base}_{index}");
                if !ids.contains(&id) {
                    break id;
                }
                index += 1;
            }
        };
        let input_id = local_id("group_input");
        let output_id = local_id("group_output");
        let origin = members.iter().fold([f32::INFINITY; 2], |min, node| {
            [min[0].min(node.position[0]), min[1].min(node.position[1])]
        });
        let mut inputs = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        let mut output_keys: BTreeMap<PortRef, String> = BTreeMap::new();
        let mut inner = vec![];
        let mut outer = vec![];
        for edge in &self.document.connections {
            match (ids.contains(&edge.from.node), ids.contains(&edge.to.node)) {
                (true, true) => inner.push(edge.clone()),
                (false, false) => outer.push(edge.clone()),
                (false, true) => {
                    let node = members.iter().find(|node| node.id == edge.to.node).unwrap();
                    let spec =
                        motion_graph::node_port_specs(node, &self.document.public_parameters)
                            .map_err(|error| error.diagnostic())?
                            .inputs
                            .remove(&edge.to.port)
                            .ok_or_else(|| {
                                diag(
                                    ErrorCode::UnknownPort,
                                    Some(&node.id),
                                    Some(&edge.to.port),
                                    "Group input endpoint is missing",
                                )
                            })?;
                    let key = format!("in_{:04}", inputs.len() + 1);
                    inputs.insert(key.clone(), named_port(&node.id, &spec));
                    outer.push(Connection {
                        from: edge.from.clone(),
                        to: PortRef::new(&id, &key),
                    });
                    inner.push(Connection {
                        from: PortRef::new(&input_id, &key),
                        to: edge.to.clone(),
                    });
                }
                (true, false) => {
                    let key = if let Some(key) = output_keys.get(&edge.from) {
                        key.clone()
                    } else {
                        let node = members
                            .iter()
                            .find(|node| node.id == edge.from.node)
                            .unwrap();
                        let spec =
                            motion_graph::node_port_specs(node, &self.document.public_parameters)
                                .map_err(|error| error.diagnostic())?
                                .outputs
                                .remove(&edge.from.port)
                                .ok_or_else(|| {
                                    diag(
                                        ErrorCode::UnknownPort,
                                        Some(&node.id),
                                        Some(&edge.from.port),
                                        "Group output endpoint is missing",
                                    )
                                })?;
                        let key = format!("out_{:04}", outputs.len() + 1);
                        outputs.insert(key.clone(), named_port(&node.id, &spec));
                        output_keys.insert(edge.from.clone(), key.clone());
                        inner.push(Connection {
                            from: edge.from.clone(),
                            to: PortRef::new(&output_id, &key),
                        });
                        key
                    };
                    outer.push(Connection {
                        from: PortRef::new(&id, key),
                        to: edge.to.clone(),
                    });
                }
            }
        }
        for node in &mut members {
            node.position = [
                node.position[0] - origin[0] + 80.,
                node.position[1] - origin[1] + 80.,
            ];
        }
        let right = members
            .iter()
            .map(|node| node.position[0])
            .fold(80., f32::max)
            + 300.;
        let mut input = Node::new(&input_id, NodeOp::GroupInput { ports: inputs });
        input.position = [-180., 80.];
        let mut output = Node::new(&output_id, NodeOp::GroupOutput { ports: outputs });
        output.position = [right, 80.];
        members.push(input);
        members.push(output);
        let graph = NodeGroup {
            name: "节点组".into(),
            english_name: "Node Group".into(),
            nodes: members,
            connections: inner,
            input_node: input_id,
            output_node: output_id,
        };
        let mut group = Node::new(
            &id,
            NodeOp::Group {
                graph: Box::new(graph),
            },
        );
        group.position = origin;
        let mut document = self.document.clone();
        document.nodes.retain(|node| !ids.contains(&node.id));
        document.nodes.push(group);
        document.connections = outer;
        self.checked_structure(&document)?;
        let before = self.state(true);
        self.document = document;
        self.selected = [id.clone()].into();
        self.primary = Some(id);
        self.finish_edit(before);
        Ok(true)
    }
    pub fn ungroup_selected(&mut self) -> Result<bool, Diagnostic> {
        if self.editing() {
            self.trace_noop("ungroup_selected", "editing");
            return Ok(false);
        }
        let groups: Vec<_> = self
            .document
            .nodes
            .iter()
            .filter(|node| {
                self.selected.contains(&node.id) && matches!(node.operation, NodeOp::Group { .. })
            })
            .cloned()
            .collect();
        if groups.is_empty() {
            self.trace_noop("ungroup_selected", "no_selected_group");
            return Ok(false);
        }
        let mut candidate = self.document.clone();
        let mut selected = BTreeSet::new();
        for group_node in groups {
            let NodeOp::Group { graph } = group_node.operation else {
                unreachable!()
            };
            let mut mapping = BTreeMap::new();
            let mut occupied: BTreeSet<_> =
                candidate.nodes.iter().map(|node| node.id.clone()).collect();
            for node in graph.nodes.iter().filter(|node| !boundary(&node.operation)) {
                let mut id = node.id.clone();
                while occupied.contains(&id) {
                    id = self.id("ungrouped");
                }
                occupied.insert(id.clone());
                mapping.insert(node.id.clone(), id);
            }
            let incoming: BTreeMap<_, _> = candidate
                .connections
                .iter()
                .filter(|edge| edge.to.node == group_node.id)
                .map(|edge| (edge.to.port.clone(), edge.from.clone()))
                .collect();
            let drivers: BTreeMap<_, _> = graph
                .connections
                .iter()
                .map(|edge| (edge.to.clone(), edge.from.clone()))
                .collect();
            let source = |port: &PortRef| -> Result<PortRef, Diagnostic> {
                if port.node == graph.input_node {
                    return incoming.get(&port.port).cloned().ok_or_else(|| {
                        diag(
                            ErrorCode::MissingInput,
                            Some(&group_node.id),
                            Some(&port.port),
                            "Connect the group input before ungrouping",
                        )
                    });
                }
                mapping
                    .get(&port.node)
                    .map(|id| PortRef::new(id, &port.port))
                    .ok_or_else(|| {
                        diag(
                            ErrorCode::UnknownNode,
                            Some(&port.node),
                            None,
                            "Group source is missing",
                        )
                    })
            };
            let mut edges = vec![];
            for edge in &candidate.connections {
                if edge.to.node == group_node.id {
                    continue;
                }
                if edge.from.node == group_node.id {
                    let driver = drivers
                        .get(&PortRef::new(&graph.output_node, &edge.from.port))
                        .ok_or_else(|| {
                            diag(
                                ErrorCode::MissingInput,
                                Some(&group_node.id),
                                Some(&edge.from.port),
                                "Connect the group output before ungrouping",
                            )
                        })?;
                    edges.push(Connection {
                        from: source(driver)?,
                        to: edge.to.clone(),
                    });
                } else {
                    edges.push(edge.clone());
                }
            }
            for edge in &graph.connections {
                if let Some(id) = mapping.get(&edge.to.node) {
                    edges.push(Connection {
                        from: source(&edge.from)?,
                        to: PortRef::new(id, &edge.to.port),
                    });
                }
            }
            candidate.nodes.retain(|node| node.id != group_node.id);
            for mut node in graph
                .nodes
                .into_iter()
                .filter(|node| !boundary(&node.operation))
            {
                node.id = mapping[&node.id].clone();
                node.position = [
                    group_node.position[0] + node.position[0] - 80.,
                    group_node.position[1] + node.position[1] - 80.,
                ];
                selected.insert(node.id.clone());
                candidate.nodes.push(node);
            }
            candidate.connections = edges;
        }
        self.checked_structure(&candidate)?;
        let before = self.state(true);
        self.document = candidate;
        self.selected = selected;
        self.primary = self.selected.iter().next().cloned();
        self.finish_edit(before);
        Ok(true)
    }
    pub fn toggle_collapsed(&mut self, id: &str) -> bool {
        if self.editing() {
            self.trace_noop("toggle_collapsed", "editing");
            return false;
        }
        let Some(index) = self.document.nodes.iter().position(|node| node.id == id) else {
            self.trace_noop("toggle_collapsed", "node_missing");
            return false;
        };
        let before = self.state(false);
        self.document.nodes[index].collapsed = !self.document.nodes[index].collapsed;
        self.finish_edit(before);
        true
    }
    pub fn expose_group_port(&mut self, port: PortRef, output: bool) -> Result<bool, Diagnostic> {
        if self.parents.is_empty() || self.editing() {
            self.trace_noop("expose_group_port", "root_or_editing");
            return Ok(false);
        }
        let node = self
            .document
            .nodes
            .iter()
            .find(|node| node.id == port.node)
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownNode,
                    Some(&port.node),
                    None,
                    "Node is missing",
                )
            })?;
        let specs = motion_graph::node_port_specs(node, &self.document.public_parameters)
            .map_err(|error| error.diagnostic())?;
        let spec = if output {
            specs.outputs.get(&port.port)
        } else {
            specs.inputs.get(&port.port)
        }
        .ok_or_else(|| {
            diag(
                ErrorCode::UnknownPort,
                Some(&port.node),
                Some(&port.port),
                "Port is missing",
            )
        })?;
        let boundary_index = self
            .document
            .nodes
            .iter()
            .position(|node| {
                if output {
                    matches!(node.operation, NodeOp::GroupOutput { .. })
                } else {
                    matches!(node.operation, NodeOp::GroupInput { .. })
                }
            })
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownNode,
                    None,
                    None,
                    "Group boundary is missing",
                )
            })?;
        let boundary_id = self.document.nodes[boundary_index].id.clone();
        if self.document.connections.iter().any(|edge| {
            if output {
                edge.from == port && edge.to.node == boundary_id
            } else {
                edge.to == port && edge.from.node == boundary_id
            }
        }) {
            self.trace_noop("expose_group_port", "already_exposed");
            return Ok(false);
        }
        let mut candidate = self.document.clone();
        let ports = match &mut candidate.nodes[boundary_index].operation {
            NodeOp::GroupInput { ports } | NodeOp::GroupOutput { ports } => ports,
            _ => unreachable!(),
        };
        let mut index = 1;
        let key = loop {
            let key = format!("{}_{index:04}", if output { "out" } else { "in" });
            if !ports.contains_key(&key) {
                break key;
            }
            index += 1;
        };
        ports.insert(key.clone(), named_port(&node.id, spec));
        if output {
            candidate.connections.push(Connection {
                from: port,
                to: PortRef::new(boundary_id, key),
            });
        } else {
            candidate.connections.retain(|edge| edge.to != port);
            candidate.connections.push(Connection {
                from: PortRef::new(boundary_id, key),
                to: port,
            });
        }
        self.checked_structure(&candidate)?;
        let before = self.state(true);
        self.document = candidate;
        self.finish_edit(before);
        Ok(true)
    }
}
