//! One application-local node clipboard, independent of text and project history.
use super::*;
use crate::shortcuts::Command;

#[derive(Clone)]
pub(super) struct Fragment {
    document: GraphDocument,
    primary: Option<String>,
    pastes: u32,
}

fn used_ids(nodes: &[Node], ids: &mut BTreeSet<String>) {
    for node in nodes {
        ids.insert(node.id.clone());
        if let NodeOp::Group { graph } = &node.operation {
            used_ids(&graph.nodes, ids);
        }
    }
}

fn references(nodes: &[Node], ids: &mut BTreeSet<String>) {
    for node in nodes {
        match &node.operation {
            NodeOp::Parameter { parameter } => {
                ids.insert(parameter.clone());
            }
            NodeOp::Group { graph } => references(&graph.nodes, ids),
            _ => {}
        }
    }
}

// Draft graphs may have no output. Check every scope's edges before the shared
// validator's normal MissingOutput/MissingInput draft boundary can short-circuit.
fn check_scopes(document: &GraphDocument, depth: usize) -> Result<(), Diagnostic> {
    if depth > 16 {
        return Err(diag(
            ErrorCode::Budget,
            None,
            None,
            "Clipboard group nesting exceeds 16",
        ));
    }
    let mut degrees: BTreeMap<_, usize> =
        document.nodes.iter().map(|n| (n.id.clone(), 0)).collect();
    let mut inputs = BTreeSet::new();
    for edge in &document.connections {
        motion_graph::check_connection(document, &edge.from, &edge.to)
            .map_err(|e| e.diagnostic())?;
        if !inputs.insert(edge.to.clone()) {
            return Err(diag(
                ErrorCode::DuplicateInput,
                Some(&edge.to.node),
                Some(&edge.to.port),
                "Clipboard input has multiple wires",
            ));
        }
        if let Some(d) = degrees.get_mut(&edge.to.node) {
            *d += 1;
        }
    }
    let mut ready: Vec<_> = degrees
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut seen = 0;
    while let Some(id) = ready.pop() {
        seen += 1;
        for edge in document.connections.iter().filter(|e| e.from.node == id) {
            if let Some(d) = degrees.get_mut(&edge.to.node) {
                *d -= 1;
                if *d == 0 {
                    ready.push(edge.to.node.clone());
                }
            }
        }
    }
    if seen != document.nodes.len() {
        return Err(diag(
            ErrorCode::Cycle,
            None,
            None,
            "Clipboard has a dependency cycle or duplicate node IDs",
        ));
    }
    for node in &document.nodes {
        if let NodeOp::Group { graph } = &node.operation {
            let mut child = document.clone();
            child.nodes = graph.nodes.clone();
            child.connections = graph.connections.clone();
            check_scopes(&child, depth + 1)?;
        }
    }
    Ok(())
}

// IDs are allocated in a candidate-local counter, so rejection changes no editor state.
fn fresh(
    counter: &mut u64,
    used: &mut BTreeSet<String>,
    prefix: &str,
) -> Result<String, Diagnostic> {
    loop {
        let id = format!("{prefix}_{}", *counter);
        *counter = counter
            .checked_add(1)
            .ok_or_else(|| diag(ErrorCode::Budget, None, None, "Node ID space exhausted"))?;
        if used.insert(id.clone()) {
            return Ok(id);
        }
    }
}
fn mapped(ids: &BTreeMap<String, String>, old: &str) -> Result<String, Diagnostic> {
    ids.get(old).cloned().ok_or_else(|| {
        diag(
            ErrorCode::UnknownNode,
            Some(old),
            None,
            "Clipboard reference is missing",
        )
    })
}
fn remap(
    nodes: &mut [Node],
    edges: &mut [Connection],
    parameters: &BTreeMap<String, String>,
    counter: &mut u64,
    used: &mut BTreeSet<String>,
) -> Result<BTreeMap<String, String>, Diagnostic> {
    let mut ids = BTreeMap::new();
    for node in nodes.iter_mut() {
        let id = fresh(counter, used, "pasted_node")?;
        if ids.insert(node.id.clone(), id.clone()).is_some() {
            return Err(diag(
                ErrorCode::DuplicateNode,
                Some(&node.id),
                None,
                "Duplicate clipboard node ID",
            ));
        }
        node.id = id;
        match &mut node.operation {
            NodeOp::Parameter { parameter } => *parameter = mapped(parameters, parameter)?,
            NodeOp::Group { graph } => {
                let child = remap(
                    &mut graph.nodes,
                    &mut graph.connections,
                    parameters,
                    counter,
                    used,
                )?;
                graph.input_node = mapped(&child, &graph.input_node)?;
                graph.output_node = mapped(&child, &graph.output_node)?;
            }
            _ => {}
        }
    }
    for edge in edges {
        edge.from.node = mapped(&ids, &edge.from.node)?;
        edge.to.node = mapped(&ids, &edge.to.node)?;
    }
    Ok(ids)
}

impl NodeEditor {
    pub(crate) fn take_clipboard_from(&mut self, previous: &mut Self) {
        self.clipboard = previous.clipboard.take();
    }

    fn fragment(&self) -> Result<Option<Fragment>, Diagnostic> {
        if self.selected.is_empty() {
            return Ok(None);
        }
        let mut document = self.document.clone();
        document.nodes.retain(|n| self.selected.contains(&n.id));
        if document.nodes.len() != self.selected.len() {
            return Err(diag(
                ErrorCode::UnknownNode,
                None,
                None,
                "Selection contains a missing node; select again",
            ));
        }
        if document
            .nodes
            .iter()
            .any(|n| grouping::boundary(&n.operation))
        {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Group boundary nodes belong to their group; copy the group instead",
            ));
        }
        document
            .connections
            .retain(|e| self.selected.contains(&e.from.node) && self.selected.contains(&e.to.node));
        let images = document.referenced_images();
        document.images.retain(|id, _| images.contains(id));
        document.validate_images().map_err(|e| e.diagnostic())?;
        check_scopes(&document, 0)?;
        let mut parameters = BTreeSet::new();
        references(&document.nodes, &mut parameters);
        document
            .public_parameters
            .retain(|p| parameters.contains(&p.id));
        if parameters.len() != document.public_parameters.len() {
            return Err(diag(
                ErrorCode::UnknownParameter,
                None,
                None,
                "Clipboard public parameter is missing",
            ));
        }
        self.check_nested_budget(&document)?;
        // Do not retain an arbitrarily large payload even if a malformed graph reached the editor.
        document.validate_metadata_budget().map_err(|e| e.diagnostic())?;
        Ok(Some(Fragment {
            document,
            primary: self.primary.clone(),
            pastes: 0,
        }))
    }

    fn copy_nodes(&mut self, cut: bool) -> Result<Option<usize>, Diagnostic> {
        let Some(fragment) = self.fragment()? else {
            return Ok(None);
        };
        let count = fragment.document.nodes.len();
        if cut {
            let mut candidate = self.document.clone();
            candidate.nodes.retain(|n| !self.selected.contains(&n.id));
            candidate.connections.retain(|e| {
                !self.selected.contains(&e.from.node) && !self.selected.contains(&e.to.node)
            });
            let images = self.root_with(&candidate).referenced_images();
            candidate.images.retain(|id, _| images.contains(id));
            self.checked_structure(&candidate)?;
            let before = self.state(true);
            // Cache before deleting. There is no fallible work after this point.
            self.clipboard = Some(fragment);
            self.document = candidate;
            self.selected.clear();
            self.primary = None;
            self.finish_edit(before);
        } else {
            self.clipboard = Some(fragment);
        }
        Ok(Some(count))
    }

    fn paste_nodes(&mut self, screen: [f32; 2]) -> Result<Option<usize>, Diagnostic> {
        let Some(fragment) = &self.clipboard else {
            return Ok(None);
        };
        check_scopes(&fragment.document, 0)?;
        if fragment.document.working_space != self.document.working_space
            || fragment.document.alpha_mode != self.document.alpha_mode
        {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Clipboard and target graph colour/alpha contracts differ",
            ));
        }
        let mut candidate = self.document.clone();
        candidate.images.extend(fragment.document.images.clone());
        let mut nodes = fragment.document.nodes.clone();
        let mut edges = fragment.document.connections.clone();
        let mut used: BTreeSet<_> = candidate
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .chain(candidate.public_parameters.iter().map(|p| p.id.clone()))
            .collect();
        let mut counter = self.next_id;
        used_ids(&candidate.nodes, &mut used);
        used_ids(&fragment.document.nodes, &mut used);
        used.extend(
            fragment
                .document
                .public_parameters
                .iter()
                .map(|p| p.id.clone()),
        );
        let mut parameters = BTreeMap::new();
        for p in &fragment.document.public_parameters {
            let mut parameter = p.clone();
            parameter.id = fresh(&mut counter, &mut used, "pasted_parameter")?;
            parameters.insert(p.id.clone(), parameter.id.clone());
            candidate.public_parameters.push(parameter);
        }
        let ids = remap(&mut nodes, &mut edges, &parameters, &mut counter, &mut used)?;
        let origin = nodes.iter().fold([f32::INFINITY; 2], |p, n| {
            [p[0].min(n.position[0]), p[1].min(n.position[1])]
        });
        let screen = if self.last_canvas.contains(screen) {
            screen
        } else {
            [
                (self.last_canvas.min[0] + self.last_canvas.max[0]) * 0.5,
                (self.last_canvas.min[1] + self.last_canvas.max[1]) * 0.5,
            ]
        };
        let position = self.view.screen_to_world(screen, self.last_canvas);
        for node in &mut nodes {
            for i in 0..2 {
                node.position[i] =
                    (node.position[i] - origin[i] + position[i] + fragment.pastes as f32 * 32.)
                        .clamp(-1.0e6, 1.0e6);
            }
        }
        let primary = fragment
            .primary
            .as_deref()
            .and_then(|p| ids.get(p).cloned())
            .or_else(|| ids.values().next().cloned());
        let count = nodes.len();
        candidate.nodes.extend(nodes);
        candidate.connections.extend(edges);
        candidate.format_version = candidate
            .format_version
            .max(candidate.minimum_format_version());
        check_scopes(&candidate, self.parents.len())?;
        if self
            .root_with(&candidate)
            .nodes
            .iter()
            .map(|n| grouping::output_count(&n.operation))
            .sum::<usize>()
            > 1
        {
            return Err(diag(
                ErrorCode::MultipleOutputs,
                None,
                None,
                "The target supports one Color Output; exclude the copied output",
            ));
        }
        self.checked_structure(&candidate)?;
        let before = self.state(true);
        self.document = candidate;
        self.next_id = counter;
        self.selected = ids.into_values().collect();
        self.primary = primary;
        if let Some(fragment) = &mut self.clipboard {
            fragment.pastes = fragment.pastes.saturating_add(1).min(1024);
        }
        self.finish_edit(before);
        Ok(Some(count))
    }

    pub(super) fn clipboard_command(
        &mut self,
        command: Command,
        language: Language,
        screen: [f32; 2],
    ) {
        if self.editing() {
            self.trace_noop("node_clipboard", "editing");
            return;
        }
        let result = match command {
            Command::CopyNodes => self.copy_nodes(false),
            Command::CutNodes => self.copy_nodes(true),
            Command::PasteNodes => self.paste_nodes(screen),
            _ => return,
        };
        match result {
            Ok(Some(count)) => {
                self.notice = Some(match command {
                    Command::CopyNodes => format!(
                        "{} {count} · {}",
                        language.text("已复制节点", "Nodes copied"),
                        language.text(
                            "应用内剪贴板；仅保留内部连线",
                            "Application clipboard; internal wires only"
                        )
                    ),
                    Command::CutNodes => {
                        format!("{} {count}", language.text("已剪切节点", "Nodes cut"))
                    }
                    _ => format!(
                        "{} {count} · {}",
                        language.text("已粘贴节点", "Nodes pasted"),
                        language.text(
                            "外部连线未重接；图像节点使用当前图像源",
                            "External wires detached; image nodes use target graph images"
                        )
                    ),
                })
            }
            Ok(None) => {
                self.notice = Some(
                    language
                        .text(
                            if command == Command::PasteNodes {
                                "节点剪贴板为空"
                            } else {
                                "请先选择节点"
                            },
                            if command == Command::PasteNodes {
                                "Node clipboard is empty"
                            } else {
                                "Select nodes first"
                            },
                        )
                        .into(),
                )
            }
            Err(error) => {
                log::error!("node clipboard rejected operation={command:?} epoch={} revision={} graph={} depth={} selected={} cause={}",self.epoch,self.revision,self.document.id.chars().take(128).collect::<String>(),self.parents.len(),self.selected.len(),error.to_string().chars().take(512).collect::<String>());
                self.notice = Some(format!(
                    "{}: {error}",
                    language.text(
                        "节点剪贴板操作失败，图未改变",
                        "Node clipboard rejected; graph unchanged"
                    )
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "clipboard/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "clipboard/image_tests.rs"]
mod image_tests;
