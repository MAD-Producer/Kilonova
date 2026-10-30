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
            next.view_preferences = self.workspace.node_view;
            next.shortcut_bindings = self.panels.shortcut_bindings.clone();
            next.recent_nodes = self.workspace.node_recents.clone();
            if source.is_none() {
                next.layer_target = Some(target.clone());
            }
            // Metadata rejection or a saturated worker must leave the current
            // editor and its standalone draft intact. Publish the replacement
            // only after the owned request has actually entered the queue.
            if let Some(source) = &source {
                self.node_tasks.open_bound(next.snapshot(), source.clone(), target.clone(), self.workspace.locale)?;
            }
            next.take_clipboard_from(&mut self.panels.node_editor);
            let old = std::mem::replace(&mut self.panels.node_editor, next);
            if old.layer_target.is_none() {
                self.node_standalone = Some(old);
            }
            if source.is_some() {
                self.node_compile_due = None;
                self.panels.selected = Some(object);
                self.switch_workspace(workspace::Preset::Nodes, event_loop)?;
                self.panels.node_editor.report_status(self.workspace.locale.t(T::NodeWorking));
                self.dirty = true;
                return Ok(());
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
        self.flush_bound_source_with_timeout(Duration::from_secs(60))
    }
    fn flush_bound_source_with_timeout(&mut self, timeout: Duration) -> Result<(), String> {
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
            let (receive, save, reused) = if let Some(index) = self.node_sources.iter().position(|(pending, _, _)|
                pending.epoch == snapshot.epoch && pending.revision == snapshot.revision)
            {
                let (_, receive, save) = self.node_sources.remove(index);
                (receive, save, true)
            } else {
                (self.engine.node_source(snapshot.clone())?, false, false)
            };
            match await_node_source(&receive, timeout, Some(&snapshot)) {
                NodeSourceWait::Completed(result) => {
                    let acknowledged = result?;
                    self.panels.node_editor.accept_project_saved(&snapshot);
                    if save {
                        self.node_save_project = Some(snapshot.clone());
                    }
                    // A reused reply can predate other queued edits. Refresh
                    // after acknowledging it, as the asynchronous path does.
                    self.state = if reused { self.engine.state()? } else { acknowledged };
                }
                NodeSourceWait::Pending(error) => {
                    // The worker is still allowed to commit this request. Keep
                    // its receiver in the normal completion queue so a late
                    // result is reconciled, and a repeated flush cannot enqueue
                    // another copy while it remains in flight.
                    self.node_sources.push((snapshot, receive, save));
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn restore_node_draft(&mut self) -> Result<(), String> {
        self.flush_bound_source()?;
        self.node_tasks.cancel(&mut self.panels.node_editor);
        self.node_compile_due = None;
        let mut restored = self.node_standalone.take().unwrap_or_else(|| {
            log::debug!("node standalone draft unavailable reason=no_saved_standalone fallback=default");
            node_editor::NodeEditor::default()
        });
        restored.take_clipboard_from(&mut self.panels.node_editor);
        self.panels.node_editor = restored;
        self.panels.node_editor.view_preferences = self.workspace.node_view;
        self.panels.node_editor.shortcut_bindings = self.panels.shortcut_bindings.clone();
        self.panels.node_editor.recent_nodes = self.workspace.node_recents.clone();
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
            let mut restored = self.node_standalone.take().unwrap_or_else(|| {
                log::debug!("node standalone draft unavailable reason=bound_target_invalidated fallback=default");
                node_editor::NodeEditor::default()
            });
            restored.take_clipboard_from(&mut self.panels.node_editor);
            self.panels.node_editor = restored;
            self.panels.node_editor.view_preferences = self.workspace.node_view;
        self.panels.node_editor.shortcut_bindings = self.panels.shortcut_bindings.clone();
            self.panels.node_editor.recent_nodes = self.workspace.node_recents.clone();
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
                    != crate::node_layer::source_document(&current.document)?
                    || source["options"]
                        != serde_json::to_value(&current.options).map_err(|e| e.to_string())?
                {
                    self.node_tasks.open_bound(current, source.clone(), target, self.workspace.locale)?;
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
        if let node_editor::Action::RecentNodesChanged(recent) = action {
            self.workspace.node_recents = recent;
            self.dirty = true;
            self.workspace.save(&self.layout_path).map_err(|reason| {
                format!("save recent node choices path={} reason={reason}", self.layout_path.display())
            })?;
            return Ok(());
        }
        if let node_editor::Action::ViewPreferencesChanged(preferences) = action {
            self.workspace.node_view = preferences;
            self.dirty = true;
            self.workspace.save(&self.layout_path).map_err(|reason| {
                format!("save node view preferences path={} reason={reason}", self.layout_path.display())
            })?;
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
            | node_editor::Action::ExportMsfx(s)
            | node_editor::Action::ImportImage { snapshot: s, .. } => s.clone(),
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
        // Publish the clipboard acknowledgement before starting another node
        // install/save that can publish a newer project snapshot.
        if self.layer_clipboard.committing() { return Ok(()); }
        self.node_tasks.reconcile_bound_open(&mut self.panels.node_editor, &self.state, self.workspace.locale);
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

/// Wait for a queued node-source reply without blocking the UI thread forever.
///
/// Mirrors `Engine::call`: the same 60 s bound and the same timeout wording.
/// A timeout is a fault, not a cancellation — the serial worker still owns the
/// request and its reply can arrive later, so the caller must not treat this
/// as a completed save.
enum NodeSourceWait {
    Completed(Result<Value, String>),
    Pending(String),
}

fn await_node_source(
    receive: &std::sync::mpsc::Receiver<Result<Value, String>>,
    wait: Duration,
    snapshot: Option<&node_editor::EditorSnapshot>,
) -> NodeSourceWait {
    let began = Instant::now();
    match receive.recv_timeout(wait) {
        Ok(result) => NodeSourceWait::Completed(result),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            NodeSourceWait::Completed(Err("node source worker disconnected".into()))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let elapsed = began.elapsed().as_millis();
            let epoch = snapshot.map(|s| s.epoch);
            let revision = snapshot.map(|s| s.revision);
            let target = snapshot.and_then(|s| s.layer_target.as_ref());
            let object = target.map(|t| t.object);
            let effect = target.map(|t| t.effect);
            log::error!("node source refresh timed out: operation=node_source epoch={epoch:?} revision={revision:?} object={object:?} effect={effect:?} waited={elapsed}ms pending=true state=unchanged reason=node source worker reply has not arrived");
            NodeSourceWait::Pending(format!("node_source timed out after {elapsed}ms; its result is still pending; check project state before retrying"))
        }
    }
}

#[cfg(test)]
mod source_wait_tests {
    use super::*;

    #[test]
    fn timeout_retains_receiver_for_late_success_and_failure() {
        for result in [Ok(serde_json::json!({"revision": 7})), Err("late failure".into())] {
            let (send, receive) = std::sync::mpsc::channel();
            assert!(matches!(await_node_source(&receive, Duration::from_millis(1), None), NodeSourceWait::Pending(_)));
            send.send(result.clone()).unwrap();
            assert_eq!(receive.try_recv().unwrap(), result);
        }
    }

    #[test]
    fn shell_reuses_timed_out_source_and_preserves_save_intent() {
        let root = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::En)).unwrap();
        let target = crate::node_layer::Target {
            root: root.path().join("project"), composition: "comp-main".into(),
            object: 99, effect: 3, key: "test".into(), label: "test".into(), creating: false,
        };
        shell.state["project"]["layers"] = serde_json::json!([{ "id": 99, "effects": [{"id": 3}] }]);
        shell.panels.node_editor.layer_target = Some(target);
        let snapshot = shell.panels.node_editor.snapshot();
        let (send, receive) = std::sync::mpsc::channel();
        shell.node_sources.push((snapshot.clone(), receive, true));
        assert!(shell.flush_bound_source_with_timeout(Duration::from_millis(1)).is_err());
        assert_eq!(shell.node_sources.len(), 1);
        assert!(shell.node_sources[0].2);
        let current = shell.engine.state().unwrap();
        let mut old = current.clone();
        old["revision"] = 999.into();
        send.send(Ok(old)).unwrap();
        shell.flush_bound_source_with_timeout(Duration::from_secs(1)).unwrap();
        assert!(shell.node_sources.is_empty());
        assert_eq!(shell.node_save_project.as_ref().unwrap().revision, snapshot.revision);
        assert_eq!(shell.state["revision"], current["revision"]);
    }

    #[test]
    fn blocked_real_worker_delivers_its_late_node_source_failure() {
        let root = tempfile::tempdir().unwrap();
        let mut shell = Shell::new(root.path().join("project"), None, Some(Locale::En)).unwrap();
        let target_root = std::path::PathBuf::from(shell.state["root"].as_str().unwrap());
        let composition = shell.state["composition"].as_str().unwrap().to_owned();
        shell.panels.node_editor.layer_target = Some(crate::node_layer::Target {
            root: target_root, composition,
            object: 99, effect: 3, key: "missing".into(), label: "test".into(), creating: false,
        });
        shell.state["project"]["layers"] = serde_json::json!([{ "id": 99, "effects": [{"id": 3}] }]);
        let (entered, waiting) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let blocked = shell.engine.request(|reply| crate::engine::Command::TestWait {
            entered, release: gate, reply,
        }).unwrap();
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = shell.flush_bound_source_with_timeout(Duration::from_millis(1));
        // Release the actual serial worker before asserting, even on failure.
        release.send(()).unwrap();
        assert!(result.unwrap_err().contains("timed out"));
        blocked.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        assert_eq!(shell.node_sources.len(), 1);
        let error = shell.node_sources.pop().unwrap().1.recv_timeout(Duration::from_secs(5))
            .unwrap().unwrap_err();
        assert!(error.contains("target layer was removed"), "{error}");
    }

    #[test]
    fn disconnected_source_is_a_completed_error() {
        let (send, receive) = std::sync::mpsc::channel();
        drop(send);
        assert!(matches!(await_node_source(&receive, Duration::from_secs(1), None),
            NodeSourceWait::Completed(Err(error)) if error.contains("disconnected")));
    }

    #[test]
    fn timeout_reaches_default_log_and_stderr_without_stdout() {
        const CHILD: &str = "MOTION_NODE_SOURCE_TIMEOUT_LOG_CHILD";
        const NAME: &str = "shell::nodes::source_wait_tests::timeout_reaches_default_log_and_stderr_without_stdout";
        if let Some(root) = std::env::var_os(CHILD) {
            let _logger = crate::logging::init(std::path::PathBuf::from(root).join("logs")).unwrap();
            let (_send, receive) = std::sync::mpsc::channel();
            assert!(matches!(await_node_source(&receive, Duration::from_millis(1), None), NodeSourceWait::Pending(_)));
            crate::logging::flush();
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--nocapture"])
            .env(CHILD, root.path()).env_remove("MOTION_LOG").output().unwrap();
        assert!(output.status.success(), "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let marker = "node source refresh timed out: operation=node_source";
        let file = std::fs::read_to_string(root.path().join("logs/motion-studio.log")).unwrap();
        assert!(file.contains(marker));
        assert!(String::from_utf8_lossy(&output.stderr).contains(marker));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(marker));
    }
}
