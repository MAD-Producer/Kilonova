//! Real native drawing, pointer requests and GPU output for the beginner path.
use super::*;
use motion_ui::{input::MouseButton, Rect};

fn center(rect: Rect) -> [f32; 2] {
    [
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    ]
}

impl Shell {
    fn onboarding_click(&mut self, point: [f32; 2]) -> Result<(), String> {
        let f = self
            .floating
            .values()
            .find(|f| f.panel == dock::Panel::Nodes)
            .ok_or("onboarding floating window missing")?;
        let physical = f.window.inner_size();
        let size = [
            physical.width as f32 / f.scale,
            physical.height as f32 / f.scale,
        ];
        for input in [
            Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            Input {
                mouse: point,
                released: Some((MouseButton::Left, point)),
                ..Default::default()
            },
        ] {
            let actions = panels::floating_chrome(
                &mut PaintList::default(),
                &mut self.atlas,
                dock::Panel::Nodes,
                size,
                &mut self.panels,
                &self.state,
                &input,
            );
            for action in actions {
                match action {
                    panels::Action::Node(action) => self.node_action(action)?,
                    unexpected => {
                        return Err(format!("unexpected onboarding action: {unexpected:?}"))
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn onboarding_screenshot(&mut self, name: &str, key: &str) -> Result<(), String> {
        let screenshot = self.capture_node_float()?;
        let path = self
            .smoke
            .as_ref()
            .ok_or("onboarding report missing")?
            .with_extension(format!("{name}.png"));
        std::fs::copy(screenshot, &path).map_err(|e| e.to_string())?;
        self.smoke_node_report[key] = serde_json::to_value(path).map_err(|e| e.to_string())?;
        Ok(())
    }
    pub(super) fn start_node_onboarding_smoke(&mut self) -> Result<(), String> {
        self.panels.node_editor = node_editor::NodeEditor::default();
        self.schedule_node_compile()?;
        self.smoke_stage = 35;
        Ok(())
    }
    fn show_node_onboarding_smoke(&mut self) -> Result<(), String> {
        let styles = PaintList::default().styles();
        self.onboarding_screenshot("onboarding-welcome", "onboardingWelcomeScreenshot")?;
        let before = self.panels.node_editor.snapshot();
        let (_, dismiss) = self
            .panels
            .node_editor
            .onboarding_controls(&styles)
            .ok_or("welcome strip missing")?;
        self.onboarding_click(center(dismiss.ok_or("continue button missing")?))?;
        if self
            .panels
            .node_editor
            .onboarding_controls(&styles)
            .is_some()
            || self.panels.node_editor.snapshot().document != before.document
            || self.panels.node_editor.stamp() != before.preview_stamp()
        {
            return Err("dismissing welcome changed the graph or preview".into());
        }
        self.smoke_node_report["onboardingDismissPreservedSource"] = true.into();
        self.onboarding_screenshot("onboarding-continued", "onboardingContinuedScreenshot")?;
        let mut empty = before.document;
        empty.nodes.clear();
        empty.connections.clear();
        empty.public_parameters.clear();
        self.panels.node_editor = node_editor::NodeEditor::new(empty);
        self.onboarding_screenshot("onboarding-empty", "onboardingEmptyScreenshot")?;
        if self
            .panels
            .node_editor
            .onboarding_controls(&styles)
            .is_none()
        {
            return Err("empty root did not offer a starting point".into());
        }
        self.smoke_node_report["onboardingEmptyHasStartingPoint"] = true.into();
        self.panels.node_editor = node_editor::NodeEditor::default();
        self.capture_node_float()?;
        let (examples, _) = self
            .panels
            .node_editor
            .onboarding_controls(&styles)
            .ok_or("fresh welcome missing")?;
        self.onboarding_click(center(examples))?;
        let size = self.size().ok_or("example browser window missing")?;
        let browser = self
            .node_examples
            .as_mut()
            .ok_or("real Examples action did not open browser")?;
        let mut paint = PaintList::default();
        browser.draw(
            &mut paint,
            &mut self.atlas,
            size,
            &Input::default(),
            self.workspace.locale,
        );
        let config = self.atlas.config();
        let path = self
            .smoke
            .as_ref()
            .unwrap()
            .with_extension("onboarding-examples.png");
        self.engine.capture_ui(
            UiFrame {
                paint,
                atlas: Some(AtlasUpload {
                    pixels: self.atlas.pixel_data().to_vec(),
                    width: config.width,
                    height: config.height,
                }),
                logical_size: size,
                composition_image: None,
                composition: None,
                window: 0,
                node_preview: None,
            },
            path.clone(),
        )?;
        self.smoke_node_report["onboardingExamplesScreenshot"] =
            serde_json::to_value(path).map_err(|e| e.to_string())?;
        let point = center(node_examples::Browser::open_button(size));
        let event = self.node_examples.as_mut().unwrap().draw(
            &mut PaintList::default(),
            &mut self.atlas,
            size,
            &Input {
                mouse: point,
                pressed: Some((MouseButton::Left, point)),
                held: Some(MouseButton::Left),
                ..Default::default()
            },
            self.workspace.locale,
        );
        let browser = self.node_examples.take().unwrap();
        if !matches!(
            event,
            Some(node_examples::Event::Open {
                id: "solid_color",
                save_first: false
            })
        ) {
            return Err("default browser did not offer the two-node starter".into());
        }
        self.open_node_example(&browser.snapshot, "solid_color")?;
        self.smoke_node_report["onboardingOpenedInTwoClicks"] = true.into();
        self.smoke_stage = 32;
        Ok(())
    }
    pub(super) fn advance_node_onboarding_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        match self.smoke_stage {
            35 => self.show_node_onboarding_smoke()?,
            32 => {
                let snapshot = self.panels.node_editor.snapshot();
                if snapshot.document.id != "solid_color"
                    || snapshot.document.nodes.len() != 2
                    || snapshot.document.connections.len() != 1
                {
                    return Err("starter source is not the actual two-node graph".into());
                }
                self.smoke_node_report["onboardingInitial"] = self
                    .engine
                    .node_capture(0., directory.join("onboarding-solid.png"))?;
                self.onboarding_screenshot("onboarding-result", "onboardingResultScreenshot")?;
                self.panels.node_editor.select(Some("color"), false);
                self.panels
                    .node_editor
                    .set_constant("color", 0, 0.9)
                    .map_err(|e| e.to_string())?;
                self.schedule_node_compile()?;
                self.smoke_stage = 33;
            }
            33 => {
                let changed = self
                    .engine
                    .node_capture(0., directory.join("onboarding-edited.png"))?;
                if changed["fingerprint"]
                    == self.smoke_node_report["onboardingInitial"]["fingerprint"]
                {
                    return Err("starter color edit did not change GPU pixels".into());
                }
                self.smoke_node_report["onboardingEditChangedPixels"] = true.into();
                self.onboarding_screenshot("onboarding-edited-ui", "onboardingEditedScreenshot")?;
                let snapshot = self.panels.node_editor.snapshot();
                let path = directory.join("onboarding-edited.msgraph");
                std::fs::write(
                    &path,
                    snapshot.document.to_json().map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                let reopened = motion_graph::GraphDocument::from_json(
                    &std::fs::read_to_string(&path).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                if reopened != snapshot.document {
                    return Err("starter source did not round trip".into());
                }
                self.smoke_node_report["onboardingSourceReopened"] = true.into();
                if !self.panels.node_editor.undo() {
                    return Err("starter color edit was not undoable".into());
                }
                self.schedule_node_compile()?;
                self.smoke_stage = 34;
            }
            34 => {
                let restored = self
                    .engine
                    .node_capture(0., directory.join("onboarding-restored.png"))?;
                if restored["fingerprint"]
                    != self.smoke_node_report["onboardingInitial"]["fingerprint"]
                {
                    return Err("starter undo did not restore GPU pixels".into());
                }
                self.smoke_node_report["onboardingUndoRestoredPixels"] = true.into();
                self.start_node_gradient_smoke()?;
            }
            _ => return Err("unexpected onboarding acceptance stage".into()),
        }
        Ok(())
    }
    pub(super) fn finish_node_starter_smoke(
        &mut self,
        directory: &std::path::Path,
    ) -> Result<(), String> {
        let state = self.engine.state()?;
        if state["revision"] != self.smoke_node_report["projectRevision"]
            || state["project"]["layers"] != self.smoke_node_report["projectEffects"]
        {
            return Err("onboarding changed the user project".into());
        }
        self.redock_panel(dock::Panel::Nodes.dock())?;
        let path = directory.join("home-after-node-preview.png");
        self.capture_home_preview_smoke(path.clone())?;
        self.smoke_node_report["homePreviewHidden"] = true.into();
        self.smoke_node_report["homeAfterNodeScreenshot"] =
            serde_json::to_value(path).map_err(|e| e.to_string())?;
        self.smoke_stage = 7;
        self.dirty = true;
        Ok(())
    }
}
