//! One transaction adds coordinate mapping without replacing image consumers.
use super::*;
#[cfg(test)]
#[path = "texture_chain/tests.rs"]
mod tests;

impl NodeEditor {
    pub fn add_texture_mapping(&mut self, language: Language) -> Result<String, Diagnostic> {
        let reject = |zh: &str, en: &str| {
            diag(
                ErrorCode::InvalidValue,
                self.primary.as_deref(),
                None,
                language.text(zh, en),
            )
        };
        if self.editing() || self.selected.len() != 1 {
            return Err(reject(
                "请选择一个图像输出或 UV 采样节点，再添加映射",
                "Select one image output or UV sampler before adding mapping",
            ));
        }
        let id = self
            .primary
            .as_deref()
            .ok_or_else(|| reject("没有映射目标", "No mapping target"))?;
        let selected = self
            .document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| reject("所选节点已不存在", "The selected node no longer exists"))?;
        clipboard::check_scopes(&self.document, self.parents.len())?;
        let uv_sampler = |n: &Node| {
            matches!(
                n.operation,
                NodeOp::SampleImage {
                    coordinates: SampleCoordinates::Uv
                }
            )
        };
        let step = view::NODE_WIDTH * 1.25;
        let mut candidate = self.document.clone();
        let mut used: BTreeSet<_> = candidate
            .public_parameters
            .iter()
            .map(|p| p.id.clone())
            .collect();
        clipboard::used_ids(&self.root_document().nodes, &mut used);
        let mut counter = self.next_id;
        let mut created_sample = false;
        let sample = if uv_sampler(selected) {
            selected.clone()
        } else {
            let ports = node_ports(selected, &self.document.public_parameters)
                .map_err(|e| e.diagnostic())?;
            let images: Vec<_> = ports
                .outputs
                .iter()
                .filter(|(_, ty)| **ty == ValueType::Image)
                .collect();
            if images.len() != 1 {
                return Err(reject(
                    "目标必须提供唯一的 Image 输出，或是 UV 模式的图像采样节点",
                    "The target needs one Image output, or must be a UV image sampler",
                ));
            }
            let image = PortRef::new(id, images[0].0);
            let consumers: Vec<_> = self
                .document
                .connections
                .iter()
                .filter(|e| e.from == image)
                .collect();
            if consumers.len() > 1 {
                return Err(reject(
                    "图像有多个使用者，请选择要修改的 UV 采样节点",
                    "This image has multiple consumers; select the UV sampler to modify",
                ));
            }
            if let Some(edge) = consumers.first() {
                let sampler = self.document.nodes.iter().find(|n| n.id == edge.to.node)
                    .filter(|n| edge.to.port == "image" && uv_sampler(n))
                    .ok_or_else(|| reject("请直接选择 UV 模式的图像采样节点，现有连接将保持不变", "Select a UV image sampler directly; existing connections are preserved"))?;
                sampler.clone()
            } else {
                let sample_id = clipboard::fresh(&mut counter, &mut used, "texture_sample")?;
                let mut sampler = Node::new(
                    &sample_id,
                    NodeOp::SampleImage {
                        coordinates: SampleCoordinates::Uv,
                    },
                );
                sampler.position = [
                    (selected.position[0] + step * 2.).clamp(-1.0e6, 1.0e6),
                    selected.position[1],
                ];
                candidate.nodes.push(sampler.clone());
                candidate.connections.push(Connection {
                    from: image,
                    to: PortRef::new(&sample_id, "image"),
                });
                created_sample = true;
                sampler
            }
        };
        if !candidate
            .connections
            .iter()
            .any(|e| e.to == PortRef::new(&sample.id, "image"))
        {
            return Err(reject(
                "先为采样节点连接 Image 输入，再添加映射",
                "Connect the sampler's Image input before adding mapping",
            ));
        }
        let coordinate = PortRef::new(&sample.id, "coordinate");
        let previous = candidate
            .connections
            .iter()
            .find(|e| e.to == coordinate)
            .cloned();
        if previous.as_ref().is_some_and(|edge| {
            candidate
                .nodes
                .iter()
                .any(|n| n.id == edge.from.node && matches!(n.operation, NodeOp::UvMapping))
        }) {
            return Err(reject(
                "此采样节点已经连接 UV 映射",
                "This sampler already has UV mapping",
            ));
        }
        let mapping_id = clipboard::fresh(&mut counter, &mut used, "texture_mapping")?;
        let mut mapping = Node::new(&mapping_id, NodeOp::UvMapping);
        mapping.input_defaults = motion_graph::authoring_input_defaults(&mapping.operation);
        mapping.position = [
            (sample.position[0] - step).clamp(-1.0e6, 1.0e6),
            (sample.position[1] + step).clamp(-1.0e6, 1.0e6),
        ];
        if let Some(mut edge) = previous {
            candidate.connections.retain(|edge| edge.to != coordinate);
            edge.to = PortRef::new(&mapping_id, "coordinate");
            candidate.connections.push(edge);
        } else if let Some(value) = sample.input_defaults.get("coordinate") {
            // An explicit constant is authored content, not a missing input.
            mapping
                .input_defaults
                .insert("coordinate".into(), value.clone());
        } else {
            let uv_id = clipboard::fresh(&mut counter, &mut used, "texture_uv")?;
            let mut uv = Node::new(&uv_id, NodeOp::Uv);
            uv.position = [
                (mapping.position[0] - step).clamp(-1.0e6, 1.0e6),
                mapping.position[1],
            ];
            candidate.nodes.push(uv);
            candidate.connections.push(Connection {
                from: PortRef::new(&uv_id, "value"),
                to: PortRef::new(&mapping_id, "coordinate"),
            });
        }
        candidate.nodes.push(mapping);
        candidate.connections.push(Connection {
            from: PortRef::new(&mapping_id, "value"),
            to: coordinate,
        });
        candidate.format_version = candidate
            .format_version
            .max(candidate.minimum_format_version());
        clipboard::check_scopes(&candidate, self.parents.len())?;
        self.checked_structure(&candidate)?;
        let before = self.state(true);
        self.document = candidate;
        self.next_id = counter;
        self.selected = BTreeSet::from([mapping_id.clone()]);
        self.primary = Some(mapping_id.clone());
        if created_sample {
            self.preview_target = Some(PortRef::new(&sample.id, "color"));
        }
        self.finish_edit(before);
        Ok(mapping_id)
    }
}
