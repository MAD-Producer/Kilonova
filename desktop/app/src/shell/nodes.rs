use crate::*;

impl Shell {
    pub(crate) fn open_node_effect(
        &mut self,
        object: u64,
        effect: Option<u64>,
        use_current: bool,
        event_loop: Option<&ActiveEventLoop>,
    ) -> Result<(), String> {
        if use_current {
            self.panels
                .node_editor
                .snapshot()
                .validate()
                .map_err(|e| e.to_string())?;
        }
        let layer = self.state["project"]["layers"]
            .as_array()
            .and_then(|ls| ls.iter().find(|l| l["id"].as_u64() == Some(object)))
            .ok_or("select a layer for a node effect")?
            .clone();
        if layer["locked"] == true
            || matches!(layer["content"]["kind"].as_str(), Some("audio" | "null"))
        {
            return Err("select an unlocked visual layer for a node effect".into());
        }
        let existing = effect
            .and_then(|id| {
                layer["effects"]
                    .as_array()?
                    .iter()
                    .find(|e| e["id"].as_u64() == Some(id))
            })
            .cloned();
        let source = existing.as_ref().and_then(|e| e.get("node_graph")).cloned();
        if effect.is_some() && source.as_ref().is_none_or(Value::is_null) {
            return Err("this effect package does not contain an editable node source".into());
        }
        if effect.is_none()
            && layer["effects"].as_array().map_or(0, Vec::len)
                >= motion_effects::MAX_EFFECTS_PER_LAYER
        {
            return Err("layer effect limit exceeded".into());
        }
        let id = match effect {
            Some(id) => id,
            None => layer["effects"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e["id"].as_u64())
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("effect ID space exhausted")?,
        };
        let key = source
            .as_ref()
            .and_then(|v| v["key"].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                format!(
                    "node-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos()
                )
            });
        let target = crate::node_layer::Target {
            root: PathBuf::from(self.state["root"].as_str().ok_or("missing project root")?),
            composition: self.state["composition"]
                .as_str()
                .unwrap_or(
                    self.state["project"]["composition_id"]
                        .as_str()
                        .unwrap_or(""),
                )
                .into(),
            object,
            effect: id,
            key,
            creating: effect.is_none(),
            label: layer["name"].as_str().unwrap_or("Layer").into(),
        };
        if self
            .panels
            .node_editor
            .layer_target
            .as_ref()
            .is_some_and(|old| {
                old.root == target.root
                    && old.composition == target.composition
                    && old.object == target.object
                    && old.effect == target.effect
            })
        {
            self.panels.selected = Some(object);
            self.switch_workspace(workspace::Preset::Nodes, event_loop)?;
            return Ok(());
        }
        self.flush_bound_source()?;
        self.node_tasks.cancel(&mut self.panels.node_editor);
        if !use_current {
            let mut next = node_editor::NodeEditor::new(crate::node_layer::identity());
            if let Some(source) = &source {
                next.load_layer_source(source, target.clone())?;
            } else {
                next.layer_target = Some(target.clone());
            }
            let old = std::mem::replace(&mut self.panels.node_editor, next);
            if old.layer_target.is_none() {
                self.node_standalone = Some(old);
            }
        } else {
            self.panels.node_editor.renew_session();
            self.panels.node_editor.layer_target = Some(target.clone());
        }
        self.panels.selected = Some(object);
        self.switch_workspace(workspace::Preset::Nodes, event_loop)?;
        self.schedule_node_compile()?;
        self.panels
            .node_editor
            .report_status(if self.workspace.locale == Locale::Zh {
                "正在绑定到图层并更新合成…"
            } else {
                "Binding layer effect and updating composition…"
            });
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn flush_bound_source(&mut self) -> Result<(), String> {
        if self.panels.node_editor.layer_target.is_some()
            && !self.panels.node_editor.bound_creating(&self.state)
        {
            let node_editor::Action::Save(snapshot) = self
                .panels
                .node_editor
                .request(node_editor::Request::Save)
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            self.state = self
                .engine
                .node_source(snapshot.clone())?
                .recv()
                .map_err(|_| "node source worker disconnected")??;
            self.panels.node_editor.accept_project_saved(&snapshot);
        }
        Ok(())
    }
    pub(crate) fn restore_node_draft(&mut self) -> Result<(), String> {
        self.flush_bound_source()?;
        self.node_tasks.cancel(&mut self.panels.node_editor);
        self.node_compile_due = None;
        self.panels.node_editor = self.node_standalone.take().unwrap_or_else(|| {
            log::debug!("node standalone draft unavailable reason=no_saved_standalone fallback=default");
            node_editor::NodeEditor::default()
        });
        self.schedule_node_compile()?;
        self.dirty = true;
        Ok(())
    }
    pub(crate) fn reconcile_node_target(&mut self) -> Result<(), String> {
        let Some(target) = self.panels.node_editor.layer_target.clone() else {
            return Ok(());
        };
        let same = self.state["root"].as_str() == target.root.to_str()
            && self.state["composition"].as_str() == Some(target.composition.as_str());
        let effect = self.state["project"]["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["id"].as_u64() == Some(target.object))
            .flat_map(|l| l["effects"].as_array().into_iter().flatten())
            .find(|e| e["id"].as_u64() == Some(target.effect));
        if !same
            || (!target.creating
                && effect
                    .is_none_or(|e| e["node_graph"]["key"].as_str() != Some(target.key.as_str())))
        {
            self.node_tasks.cancel(&mut self.panels.node_editor);
            self.node_compile_due = None;
            self.panels.node_editor = self.node_standalone.take().unwrap_or_else(|| {
                log::debug!("node standalone draft unavailable reason=bound_target_invalidated fallback=default");
                node_editor::NodeEditor::default()
            });
            self.schedule_node_compile()?;
            return Ok(());
        }
        if !self.panels.node_editor.dirty()
            && self.node_compile_due.is_none()
            && !self.node_tasks.busy()
            && self.node_installing.is_empty()
            && self.node_sources.is_empty()
        {
            if let Some(source) = effect.and_then(|e| e.get("node_graph")) {
                let current = self.panels.node_editor.snapshot();
                if source["document"]
                    != serde_json::to_value(&current.document).map_err(|e| e.to_string())?
                    || source["options"]
                        != serde_json::to_value(&current.options).map_err(|e| e.to_string())?
                {
                    self.panels.node_editor.load_layer_source(source, target)?;
                    self.schedule_node_compile()?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn store_node_source(
        &mut self,
        snapshot: node_editor::EditorSnapshot,
        save: bool,
    ) -> Result<(), String> {
        if snapshot.layer_target.is_none() {
            return Ok(());
        }
        if let Some((_, _, saving)) = self
            .node_sources
            .iter_mut()
            .find(|(s, _, _)| s.epoch == snapshot.epoch && s.revision == snapshot.revision)
        {
            *saving |= save;
            return Ok(());
        }
        self.node_sources
            .push((snapshot.clone(), self.engine.node_source(snapshot)?, save));
        Ok(())
    }

    pub(crate) fn schedule_node_compile(&mut self) -> Result<(), String> {
        self.node_tasks.invalidate_compile();
        self.engine.node_expect(self.panels.node_editor.stamp())?;
        self.node_compile_due = Some(Instant::now() + Duration::from_millis(150));
        Ok(())
    }
    pub(crate) fn node_action(&mut self, action: node_editor::Action) -> Result<(), String> {
        if self.closing {
            return Ok(());
        }
        if matches!(action, node_editor::Action::ShowProperties) {
            self.actions.push_back(panels::Action::ShowPanel(
                dock::Panel::NodeProperties.dock(),
            ));
            self.panels.focused = Some(dock::Panel::NodeProperties);
            return Ok(());
        }
        if matches!(action, node_editor::Action::PreviewChanged) {
            self.schedule_node_compile()?;
            return Ok(());
        }
        if matches!(
            &action,
            node_editor::Action::Open(_) | node_editor::Action::Examples(_)
        ) {
            self.flush_bound_source()?;
        }
        if let node_editor::Action::Examples(snapshot) = action {
            let needs_save = self.panels.node_editor.dirty() && snapshot.revision > 0;
            self.node_examples = Some(node_examples::Browser::new(snapshot, needs_save));
            self.dirty = true;
            return Ok(());
        }
        if let node_editor::Action::Changed { semantic, .. } = action {
            if !semantic {
                self.store_node_source(self.panels.node_editor.snapshot(), false)?;
            }
            if semantic {
                self.schedule_node_compile()?;
            }
            return Ok(());
        }
        if let node_editor::Action::Save(snapshot) = &action {
            if snapshot.layer_target.is_some() {
                if (self.node_compile_due.is_some()
                    || self.node_tasks.compiling(snapshot)
                    || !self.node_installing.is_empty())
                    && snapshot.validate().is_ok()
                {
                    // The canonical install saves source and shader in one undo.
                    // Wait for its acknowledgement before writing the project.
                    self.node_save_project = Some(snapshot.clone());
                } else {
                    self.store_node_source(snapshot.clone(), true)?;
                }
                self.dirty = true;
                return Ok(());
            }
        }
        if self.closing {
            return Ok(());
        }
        let snapshot = match &action {
            node_editor::Action::Open(s)
            | node_editor::Action::Save(s)
            | node_editor::Action::SaveAs(s)
            | node_editor::Action::Validate(s)
            | node_editor::Action::Compile(s)
            | node_editor::Action::ExportMsfx(s) => s.clone(),
            _ => unreachable!(),
        };
        if matches!(&action, node_editor::Action::Compile(_)) {
            self.engine.node_expect(snapshot.preview_stamp())?;
            self.node_compile_due = None;
            if let Err(error) = self.engine.node_preview_available() {
                self.panels.node_editor.cancel_compile(&snapshot);
                return Err(error);
            }
        }
        if let Err(error) = self.node_tasks.submit(
            action,
            self.workspace.locale,
            self.panels.node_editor.layer_target.is_none() && self.panels.node_editor.dirty(),
        ) {
            self.panels.node_editor.cancel_compile(&snapshot);
            self.panels.node_editor.report_error(&error);
            return Err(error);
        }
        self.panels
            .node_editor
            .report_status(self.workspace.locale.t(T::NodeWorking));
        Ok(())
    }
    pub(crate) fn poll_nodes(&mut self) -> Result<(), String> {
        self.reconcile_node_target()?;
        let before = self.panels.node_editor.stamp();
        let (changed, ready) = self
            .node_tasks
            .poll(&mut self.panels.node_editor, self.workspace.locale);
        if changed {
            self.dirty = true;
        }
        if self.node_close_save.is_some() && !self.node_tasks.busy() && self.node_sources.is_empty()
        {
            let snapshot = self.node_close_save.take().unwrap();
            if self.node_close_current(&snapshot) && !self.panels.node_editor.dirty() {
                self.begin_close();
            } else if !self.node_close_current(&snapshot) {
                self.panels
                    .node_editor
                    .report_error(self.workspace.locale.t(T::NodeStaleClose));
            }
        }
        if before != self.panels.node_editor.stamp() {
            self.schedule_node_compile()?;
        }
        for ready in ready {
            let report = ready.compiled.report.clone();
            match self.engine.node_install_bound(
                ready.snapshot.clone(),
                ready.compiled,
                ready.canonical,
            ) {
                Ok(receive) => self.node_installing.push((ready.snapshot, report, receive)),
                Err(error) => {
                    self.panels.node_editor.cancel_compile(&ready.snapshot);
                    self.panels.node_editor.report_error(error);
                }
            }
        }
        self.node_installing
            .retain(|(snapshot, report, receive)| match receive.try_recv() {
                Ok(Ok(true)) => {
                    self.panels
                        .node_editor
                        .accept_compile(snapshot, Ok(report.clone()));
                    self.panels.node_preview.active = Some(snapshot.preview_stamp());
                    if self.panels.node_editor.stamp() == snapshot.preview_stamp() {
                        self.panels
                            .node_editor
                            .report_status(self.workspace.locale.t(T::NodeCompiled));
                    }
                    if snapshot.layer_target.is_some() {
                        self.panels.node_editor.accept_project_saved(snapshot);
                        match self.engine.state() {
                            Ok(state) => self.state = state,
                            Err(error) => log::error!("node install refresh failed: installed=true phase=state epoch={} revision={} reason={error}", snapshot.epoch, snapshot.revision),
                        }
                        match self.engine.plugin("{\"op\":\"catalogue\"}".into()) {
                            Ok(catalogue) => self.panels.effect_controls.catalogue = std::sync::Arc::new(catalogue),
                            Err(error) => log::error!("node install refresh failed: installed=true phase=catalogue epoch={} revision={} reason={error}", snapshot.epoch, snapshot.revision),
                        }
                    }
                    self.dirty = true;
                    false
                }
                Ok(Ok(false)) => {
                    self.panels.node_editor.cancel_compile(snapshot);
                    false
                }
                Ok(Err(error)) => {
                    self.panels.node_editor.accept_compile(
                        snapshot,
                        Err(motion_graph::Diagnostic {
                            code: motion_graph::ErrorCode::Shader,
                            node: None,
                            port: None,
                            message: error,
                        }),
                    );
                    self.dirty = true;
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.panels.node_editor.cancel_compile(snapshot);
                    self.panels
                        .node_editor
                        .report_error("node GPU worker disconnected");
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
            });
        if self.node_example_after_save.is_some() && !self.node_tasks.busy() {
            let (snapshot, id) = self.node_example_after_save.take().unwrap();
            if self.node_close_current(&snapshot) && !self.panels.node_editor.dirty() {
                if let Err(error) = self.open_node_example(&snapshot, id) {
                    self.panels.node_editor.report_error(error);
                }
            } else if !self.node_close_current(&snapshot) {
                self.panels
                    .node_editor
                    .report_error(self.workspace.locale.t(T::NodeStaleOpen));
            }
        }
        let mut completed = vec![];
        self.node_sources
            .retain(|(snapshot, receive, save)| match receive.try_recv() {
                Ok(result) => {
                    completed.push((snapshot.clone(), result, *save));
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => true,
                Err(_) => {
                    completed.push((
                        snapshot.clone(),
                        Err("node source worker disconnected".into()),
                        *save,
                    ));
                    false
                }
            });
        for (snapshot, result, save) in completed {
            match result {
                Ok(_state) => {
                    if snapshot.layer_target.as_ref().is_some_and(|t| {
                        self.state["root"].as_str() == t.root.to_str()
                            && self.state["composition"].as_str() == Some(t.composition.as_str())
                    }) {
                        self.state = self.engine.state()?;
                        self.panels.node_editor.accept_project_saved(&snapshot);
                        if save {
                            self.node_save_project = Some(snapshot.clone());
                        }
                    }
                }
                Err(error) => self.panels.node_editor.report_error(error),
            }
            self.dirty = true;
        }
        if self.panels.node_editor.layer_target.is_some()
            && self.panels.node_editor.dirty()
            && self.panels.node_editor.diagnostic().is_some()
            && !self.node_tasks.busy()
            && self.node_installing.is_empty()
            && self.node_compile_due.is_none()
        {
            self.store_node_source(self.panels.node_editor.snapshot(), false)?;
        }
        if self.attached
            && !self.closing
            && self
                .node_compile_due
                .is_some_and(|due| Instant::now() >= due)
        {
            self.node_compile_due = None;
            let snapshot = self.panels.node_editor.snapshot();
            if !self.node_tasks.compiling(&snapshot) {
                let action = self
                    .panels
                    .node_editor
                    .request(node_editor::Request::Compile)
                    .map_err(|d| d.to_string())?;
                self.node_action(action)?;
            }
        }
        if !self.node_tasks.busy()
            && self.node_installing.is_empty()
            && self.node_sources.is_empty()
            && self.node_compile_due.is_none()
        {
            if let Some(saved) = self.node_save_project.take() {
                if saved
                    .layer_target
                    .as_ref()
                    .is_some_and(|t| self.state["root"].as_str() == t.root.to_str())
                {
                    self.state = self.engine.save()?;
                    let message = if self.workspace.locale == Locale::Zh {
                        "节点源和工程已保存"
                    } else {
                        "Node source and project saved"
                    };
                    self.panels.node_editor.report_status(message);
                    self.panels.message = Some(message.into());
                    self.dirty = true;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn node_viewport(
        &self,
        rect: Option<motion_ui::Rect>,
        scale: f32,
    ) -> Option<node_preview::Viewport> {
        if self.panels.node_preview.active.is_none()
            || self.node_close_prompt.is_some()
            || self.node_examples.is_some()
        {
            return None;
        }
        rect.map(|rect| node_preview::Viewport {
            rect: [
                rect.min[0] * scale,
                rect.min[1] * scale,
                rect.width() * scale,
                rect.height() * scale,
            ],
            seconds: self.panels.node_preview.seconds(),
        })
    }
    pub(crate) fn node_close_current(&self, snapshot: &node_editor::EditorSnapshot) -> bool {
        let current = self.panels.node_editor.snapshot();
        current.epoch == snapshot.epoch
            && current.revision == snapshot.revision
            && !self.panels.node_editor.editing()
    }
    pub(crate) fn open_node_example(
        &mut self,
        snapshot: &node_editor::EditorSnapshot,
        id: &str,
    ) -> Result<(), String> {
        if !self.node_close_current(snapshot) {
            return Err(self.workspace.locale.t(T::NodeStaleOpen).into());
        }
        let example = motion_graph::builtin_examples()
            .iter()
            .find(|example| example.id == id)
            .ok_or("unknown builtin graph")?;
        let document = example.load().map_err(|error| error.to_string())?;
        let target = self.panels.node_editor.layer_target.clone();
        self.panels.node_editor.replace_document(document, None);
        self.panels.node_editor.layer_target = target;
        self.schedule_node_compile()?;
        self.dirty = true;
        Ok(())
    }
}
