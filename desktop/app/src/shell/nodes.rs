use crate::*;

impl Shell {
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
        if let node_editor::Action::Examples(snapshot) = action {
            let needs_save = self.panels.node_editor.dirty() && snapshot.revision > 0;
            self.node_examples = Some(node_examples::Browser::new(snapshot, needs_save));
            self.dirty = true;
            return Ok(());
        }
        if let node_editor::Action::Changed { semantic, .. } = action {
            if semantic {
                self.schedule_node_compile()?;
            }
            return Ok(());
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
        let gpu = if matches!(&action, node_editor::Action::Compile(_)) {
            self.engine
                .node_expect((snapshot.epoch, snapshot.semantic_revision))?;
            self.node_compile_due = None;
            Some(self.engine.node_gpu()?)
        } else {
            None
        };
        if let Err(error) = self.node_tasks.submit(
            action,
            self.workspace.locale,
            self.panels.node_editor.dirty(),
            gpu,
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
        let before = self.panels.node_editor.stamp();
        let (changed, ready) = self
            .node_tasks
            .poll(&mut self.panels.node_editor, self.workspace.locale);
        if changed {
            self.dirty = true;
        }
        if self.node_close_save.is_some() && !self.node_tasks.busy() {
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
            match self.engine.node_install(ready.gpu) {
                Ok(receive) => self.node_installing.push((ready.snapshot, receive)),
                Err(error) => self.panels.node_editor.report_error(error),
            }
        }
        self.node_installing
            .retain(|(snapshot, receive)| match receive.try_recv() {
                Ok(Ok(true)) => {
                    self.panels.node_preview.active =
                        Some((snapshot.epoch, snapshot.semantic_revision));
                    if self.panels.node_editor.stamp()
                        == (snapshot.epoch, snapshot.semantic_revision)
                    {
                        self.panels
                            .node_editor
                            .report_status(self.workspace.locale.t(T::NodeCompiled));
                    }
                    self.dirty = true;
                    false
                }
                Ok(Ok(false)) => false,
                Ok(Err(error)) => {
                    self.panels.node_editor.report_error(error);
                    self.dirty = true;
                    false
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
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
        self.panels.node_editor.replace_document(document, None);
        self.schedule_node_compile()?;
        self.dirty = true;
        Ok(())
    }
}
