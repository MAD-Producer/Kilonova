//! Drop positions are captured in graph coordinates before asynchronous IO.
use super::*;

impl NodeEditor {
    pub(crate) fn image_drop(
        &mut self,
        path: PathBuf,
        screen: [f32; 2],
        language: Language,
    ) -> Option<Action> {
        if !self.last_canvas.contains(screen) {
            return None;
        }
        if self.editing() {
            self.report_status(language.text(
                "请先完成当前节点编辑，再拖入图像",
                "Finish the current node edit before dropping an image",
            ));
            return None;
        }
        let position = self.view.screen_to_world(screen, self.last_canvas);
        if !position.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
            self.report_status(language.text(
                "拖入位置超出节点画布范围",
                "The drop position is outside the graph coordinate range",
            ));
            return None;
        }
        self.focus_panel();
        Some(Action::ImportImage {
            snapshot: self.snapshot(),
            path,
            position,
        })
    }

    pub(crate) fn accept_imported_image(
        &mut self,
        snapshot: &EditorSnapshot,
        image: motion_graph::GraphImage,
        position: [f32; 2],
        name: &str,
    ) -> Result<Option<String>, Diagnostic> {
        if self.epoch != snapshot.epoch
            || self.revision != snapshot.revision
            || self.preview_revision != snapshot.preview_revision
            || self.editing()
        {
            return Ok(None);
        }
        if !position.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
            return Err(diag(
                ErrorCode::InvalidValue,
                None,
                None,
                "Invalid image drop position",
            ));
        }
        let mut candidate = self.document.clone();
        let mut used: BTreeSet<_> = candidate
            .public_parameters
            .iter()
            .map(|p| p.id.clone())
            .collect();
        clipboard::used_ids(&self.root_document().nodes, &mut used);
        let mut counter = self.next_id;
        let id = clipboard::fresh(&mut counter, &mut used, "image")?;
        let mut node = Node::new(
            &id,
            NodeOp::ImageAsset {
                image: image.id().into(),
            },
        );
        node.position = position;
        let title: String = name.chars().filter(|c| !c.is_control()).collect();
        node.display_name = title
            .trim()
            .char_indices()
            .take_while(|(i, c)| i + c.len_utf8() <= 120)
            .map(|(_, c)| c)
            .collect();
        candidate.images.entry(image.id().into()).or_insert(image);
        candidate.nodes.push(node);
        candidate.format_version = candidate
            .format_version
            .max(candidate.minimum_format_version());
        candidate.validate_images().map_err(|e| e.diagnostic())?;
        clipboard::check_scopes(&candidate, self.parents.len())?;
        self.checked_structure(&candidate)?;
        let before = self.state(true);
        self.document = candidate;
        self.next_id = counter;
        self.selected = BTreeSet::from([id.clone()]);
        self.primary = Some(id.clone());
        self.preview_target = Some(PortRef::new(&id, "image"));
        self.finish_edit(before);
        Ok(Some(id))
    }
}

#[cfg(test)]
#[path = "image_import/tests.rs"]
mod tests;
