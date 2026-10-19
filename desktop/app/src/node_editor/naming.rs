//! Author-facing names remain separate from stable graph identities.
use super::*;

#[derive(Clone)]
pub(super) enum RenameTarget {
    Node(String),
    Port {
        node: String,
        output: bool,
        key: String,
    },
}
pub(super) struct NameEdit {
    pub target: RenameTarget,
    pub editor: motion_ui::widgets::TextEditor,
    pub position: [f32; 2],
    pub fresh: bool,
}

fn checked_name(id: &str, text: &str, allow_empty: bool) -> Result<String, Diagnostic> {
    if text.len() > 120 || text.chars().any(char::is_control) {
        return Err(diag(
            ErrorCode::InvalidValue,
            Some(id),
            None,
            "Use visible text of at most 120 UTF-8 bytes",
        ));
    }
    let name = text.trim();
    if !allow_empty && name.is_empty() {
        return Err(diag(
            ErrorCode::InvalidValue,
            Some(id),
            None,
            "Enter a name",
        ));
    }
    Ok(name.to_owned())
}

fn numbered_names<'a>(
    chinese: &str,
    english: &str,
    existing: impl Iterator<Item = (&'a str, &'a str)>,
) -> (String, String) {
    let existing: Vec<_> = existing.collect();
    // Each saved bilingual pair can occupy at most two candidate numbers.
    for index in 1..=existing.len().saturating_mul(2).saturating_add(1) {
        let names = (format!("{chinese} {index}"), format!("{english} {index}"));
        if existing
            .iter()
            .all(|(zh, en)| *zh != names.0 && *en != names.1)
        {
            return names;
        }
    }
    unreachable!("finite name pairs cannot occupy more than twice their count")
}

impl NodeEditor {
    pub(super) fn begin_rename(
        &mut self,
        target: RenameTarget,
        position: [f32; 2],
        language: Language,
    ) {
        let value = match &target {
            RenameTarget::Node(id) => self
                .document
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .map(|n| catalog::node_title(n, &self.document.public_parameters, language)),
            RenameTarget::Port { node, output, key } => self
                .document
                .nodes
                .iter()
                .find(|n| n.id == *node)
                .and_then(|n| motion_graph::group_port(&n.operation, key, *output))
                .map(|p| language.text(&p.name, &p.english_name).to_owned()),
        };
        let Some(value) = value else {
            return;
        };
        self.cancel_edit_only();
        self.focus = true;
        let mut editor = motion_ui::widgets::TextEditor::default();
        editor.begin(&value);
        self.rename = Some(NameEdit {
            target,
            editor,
            position,
            fresh: true,
        });
    }

    /// User aliases are language-independent; empty restores the operation title.
    /// Group names use their existing bilingual metadata so breadcrumbs agree.
    pub fn rename_node(&mut self, id: &str, text: &str) -> Result<(), Diagnostic> {
        let index = self
            .document
            .nodes
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| diag(ErrorCode::UnknownNode, Some(id), None, "Node is missing"))?;
        let node = &self.document.nodes[index];
        let is_group = matches!(node.operation, NodeOp::Group { .. });
        let name = checked_name(id, text, !is_group)?;
        let unchanged = match &node.operation {
            NodeOp::Group { graph } => {
                node.display_name.is_empty() && graph.name == name && graph.english_name == name
            }
            _ => node.display_name == name,
        };
        if unchanged {
            return Ok(());
        }
        let before = self.state(false);
        let node = &mut self.document.nodes[index];
        if let NodeOp::Group { graph } = &mut node.operation {
            graph.name = name.clone();
            graph.english_name = name;
            node.display_name.clear();
        } else {
            node.display_name = name;
        }
        self.finish_edit(before);
        Ok(())
    }

    /// Rename a group socket without changing its key, type, semantic or edges.
    pub fn rename_group_port(
        &mut self,
        id: &str,
        output: bool,
        key: &str,
        text: &str,
    ) -> Result<(), Diagnostic> {
        let name = checked_name(id, text, false)?;
        let node = self
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| {
                diag(
                    ErrorCode::UnknownNode,
                    Some(id),
                    Some(key),
                    "Node is missing",
                )
            })?;
        let port = motion_graph::group_port(&node.operation, key, output).ok_or_else(|| {
            diag(
                ErrorCode::UnknownPort,
                Some(id),
                Some(key),
                "Group port is missing",
            )
        })?;
        if port.name == name && port.english_name == name {
            return Ok(());
        }
        let mut candidate = self.document.clone();
        let node = candidate
            .nodes
            .iter_mut()
            .find(|n| n.id == id)
            .expect("checked node");
        let ports = match &mut node.operation {
            NodeOp::Group { graph } => {
                let boundary = if output {
                    &graph.output_node
                } else {
                    &graph.input_node
                };
                let boundary = graph
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == boundary)
                    .ok_or_else(|| {
                        diag(
                            ErrorCode::UnknownNode,
                            Some(id),
                            Some(key),
                            "Group boundary is missing",
                        )
                    })?;
                match &mut boundary.operation {
                    NodeOp::GroupInput { ports } if !output => ports,
                    NodeOp::GroupOutput { ports } if output => ports,
                    _ => {
                        return Err(diag(
                            ErrorCode::UnknownPort,
                            Some(id),
                            Some(key),
                            "Group boundary kind does not match",
                        ))
                    }
                }
            }
            NodeOp::GroupInput { ports } if output => ports,
            NodeOp::GroupOutput { ports } if !output => ports,
            _ => {
                return Err(diag(
                    ErrorCode::UnknownPort,
                    Some(id),
                    Some(key),
                    "Group port is missing",
                ))
            }
        };
        let port = ports.get_mut(key).ok_or_else(|| {
            diag(
                ErrorCode::UnknownPort,
                Some(id),
                Some(key),
                "Group port is missing",
            )
        })?;
        port.name = name.clone();
        port.english_name = name;
        let before = self.state(false);
        self.document = candidate;
        self.finish_edit(before);
        Ok(())
    }

    pub(super) fn next_parameter_names(&self, ty: ValueType) -> (String, String) {
        let (chinese, english) = match ty {
            ValueType::Float => ("数值", "Value"),
            ValueType::Vec2 => ("二维向量", "2D Vector"),
            ValueType::Vec3 => ("三维向量", "3D Vector"),
            ValueType::Color => ("颜色", "Color"),
            ValueType::Bool => ("开关", "Switch"),
            _ => ("参数", "Parameter"),
        };
        numbered_names(
            chinese,
            english,
            self.document
                .public_parameters
                .iter()
                .map(|p| (p.name.as_str(), p.english_name.as_str())),
        )
    }

    pub(super) fn next_group_names(&self) -> (String, String) {
        numbered_names(
            "节点组",
            "Node Group",
            self.document.nodes.iter().filter_map(|n| {
                if let NodeOp::Group { graph } = &n.operation {
                    Some((graph.name.as_str(), graph.english_name.as_str()))
                } else {
                    None
                }
            }),
        )
    }
}
