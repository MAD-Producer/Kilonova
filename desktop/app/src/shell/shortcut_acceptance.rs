//! Native GPU captures use real preference controls and real menu renderers.
use crate::shortcuts::{bindings::Bindings, Command as Shortcut, Scope};
use crate::*;
use motion_ui::input::KeyChord;
use motion_ui::Rect;

impl Shell {
    pub(super) fn capture_shortcut_screens(
        &mut self,
        window_size: [f32; 2],
        report: &std::path::Path,
        output: &mut serde_json::Map<String, Value>,
    ) -> Result<(), String> {
        let original = self.engine.state()?;
        let mut bindings = Bindings::default();
        for (scope, command, chord) in [
            (
                Scope::Application,
                Shortcut::SelectAll,
                KeyChord::new(Key::B, true, false, false),
            ),
            (
                Scope::Application,
                Shortcut::Preferences,
                KeyChord::new(Key::E, true, false, false),
            ),
            (
                Scope::Application,
                Shortcut::Duplicate,
                KeyChord::new(Key::D, true, false, true),
            ),
            (
                Scope::Nodes,
                Shortcut::AddNode,
                KeyChord::new(Key::B, true, false, true),
            ),
            (
                Scope::Nodes,
                Shortcut::ToggleNodeGrid,
                KeyChord::new(Key::B, false, false, false),
            ),
        ] {
            bindings.bind(scope, command, Some(chord))?;
        }
        let bindings = Arc::new(bindings);
        for (name, size) in [
            ("shortcuts", [900., 600.]),
            ("shortcuts-capture", [900., 600.]),
            ("shortcuts-reserved", [900., 600.]),
            ("shortcuts-scope-menu", [900., 600.]),
            ("shortcuts-command-menu", [900., 600.]),
            ("shortcuts-compact", [360., 400.]),
            ("shortcuts-edit-menu", [900., 600.]),
            ("shortcuts-node-menu", [900., 600.]),
            ("shortcuts-layer-menu", [900., 600.]),
            ("shortcuts-layer-create-menu", [900., 600.]),
        ] {
            let mut paint = PaintList::default();
            paint.stylesheet = self.panels.styles.clone();
            let styles = paint.styles();
            let row = styles.metric("row.height", 22.);
            let gap = styles.metric("auxiliary.gap", 6.);
            if name.ends_with("edit-menu") || name.ends_with("layer-menu") || name.ends_with("layer-create-menu") {
                let mut panels = panels::Panels {
                    locale: self.workspace.locale,
                    styles: self.panels.styles.clone(),
                    shortcut_bindings: bindings.clone(),
                    ..Default::default()
                };
                panels.layer_actions.shortcut_bindings = bindings.clone();
                let object = self.state["project"]["layers"]
                    .as_array()
                    .and_then(|layers| layers.first())
                    .and_then(|layer| layer["id"].as_u64());
                if name.ends_with("layer-menu") {
                    panels.select_only(object);
                }
                // Let real chrome establish its document context and window owner before opening.
                let mut discarded = paint.child();
                let warmup = panels::chrome(
                    &mut discarded,
                    &mut self.atlas,
                    size,
                    &dock::Node::dock(dock::Panel::Timeline.dock()),
                    &mut panels,
                    &self.state,
                    &Input::default(),
                );
                if !warmup.is_empty() {
                    return Err("menu warmup emitted actions".into());
                }
                if name.ends_with("edit-menu") {
                    panels.menu = Some(1);
                } else if name.ends_with("layer-create-menu") {
                    panels.menu = Some(3);
                } else {
                    let object = object.ok_or("shortcut menu capture requires a fixture layer")?;
                    panels.layer_actions.open(
                        Some(object),
                        [size[0] * 0.5, size[1] * 0.25],
                        &self.state,
                    );
                }
                let actions = panels::chrome(
                    &mut paint,
                    &mut self.atlas,
                    size,
                    &dock::Node::dock(dock::Panel::Timeline.dock()),
                    &mut panels,
                    &self.state,
                    &Input::default(),
                );
                if !actions.is_empty() {
                    return Err(format!("{name} capture emitted an action"));
                }
            } else if name.ends_with("node-menu") {
                let mut editor = node_editor::NodeEditor::default();
                editor.shortcut_bindings = bindings.clone();
                let area = Rect::new(0., 0., size[0], size[1]);
                let language = if self.workspace.locale == Locale::Zh {
                    node_editor::Language::Chinese
                } else {
                    node_editor::Language::English
                };
                let mut discarded = paint.child();
                editor.draw(
                    &mut discarded,
                    &mut self.atlas,
                    area,
                    &Input::default(),
                    language,
                );
                let canvas = editor.layout(area).canvas;
                let pad = styles.metric("padding.panel", 8.);
                let point = [canvas.max[0] - pad * 2., canvas.max[1] - pad * 2.];
                let response = editor.draw(
                    &mut paint,
                    &mut self.atlas,
                    area,
                    &Input {
                        mouse: point,
                        pressed: Some((motion_ui::MouseButton::Right, point)),
                        ..Default::default()
                    },
                    language,
                );
                if response.changed || !response.actions.is_empty() {
                    return Err("opening node capture menu changed the graph".into());
                }
            } else {
                let mut fixture = workspace::Workspace::default();
                fixture.shortcut_bindings = (*bindings).clone();
                fixture.locale = self.workspace.locale;
                let mut dialog = dialogs::Dialog::preferences(&fixture);
                let pad = styles.metric("preferences.margin", 16.);
                let section = styles.metric("preferences.section_gap", 12.);
                let width = styles
                    .metric("preferences.width", 512.)
                    .min(size[0] - pad * 2.);
                let height = styles
                    .metric("preferences.max_height", 492.)
                    .min(size[1] - pad * 2.);
                let x = (size[0] - width) * 0.5 + pad;
                let top = (size[1] - height) * 0.5 + pad + row * 2. + section;
                let compact = width < styles.metric("preferences.sidebar_breakpoint", 440.);
                let nav = styles.metric("preferences.sidebar_width", 100.);
                let point = if compact {
                    let tab = (width - pad * 2. - gap * 2.) / 3.;
                    [x + 2. * (tab + gap) + tab * 0.5, top + row * 0.5]
                } else {
                    [x + nav * 0.5, top + (row + gap) * 2. + row * 0.5]
                };
                let mut discarded = paint.child();
                dialog.render(
                    &mut discarded,
                    &mut self.atlas,
                    size,
                    &Input {
                        mouse: point,
                        pressed: Some((motion_ui::MouseButton::Left, point)),
                        ..Default::default()
                    },
                    self.workspace.locale,
                );
                let body_x = if compact { x } else { x + nav + section };
                let body_top = if compact { top + row + gap } else { top };
                let body_width = width - pad * 2. - if compact { 0. } else { nav + section };
                let record = [
                    body_x + (body_width - gap * 2.) / 6.,
                    body_top + (row + gap) * 3. + row + row * 0.5,
                ];
                if name.ends_with("capture") || name.ends_with("reserved") {
                    dialog.render(
                        &mut discarded,
                        &mut self.atlas,
                        size,
                        &Input {
                            mouse: record,
                            pressed: Some((motion_ui::MouseButton::Left, record)),
                            ..Default::default()
                        },
                        self.workspace.locale,
                    );
                }
                let input = if name.ends_with("reserved") {
                    Input {
                        keys: vec![(
                            Key::F4,
                            motion_ui::input::Modifiers {
                                alt: true,
                                ..Default::default()
                            },
                        )],
                        ..Default::default()
                    }
                } else if name.ends_with("scope-menu") || name.ends_with("command-menu") {
                    let point = [
                        body_x + body_width - gap,
                        body_top
                            + row * 0.5
                            + if name.ends_with("command-menu") {
                                row + gap
                            } else {
                                0.
                            },
                    ];
                    Input {
                        mouse: point,
                        pressed: Some((motion_ui::MouseButton::Left, point)),
                        ..Default::default()
                    }
                } else {
                    Input::default()
                };
                if dialog
                    .render(
                        &mut paint,
                        &mut self.atlas,
                        size,
                        &input,
                        self.workspace.locale,
                    )
                    .is_some()
                {
                    return Err(format!("{name} capture committed a preference action"));
                }
            }
            let mut ui = self.ui_frame(paint, window_size, true);
            // This isolated capture overrides shell geometry; use the single-view frame protocol.
            ui.views = None;
            ui.composition = None;
            ui.composition_image = None;
            ui.node_preview = None;
            let path = report.with_extension(format!("{name}.png"));
            self.capture_presented_ui(ui, path.clone())?;
            let pixels = image::open(&path).map_err(|e| e.to_string())?;
            let width = (size[0] * pixels.width() as f32 / window_size[0]).round() as u32;
            let height = (size[1] * pixels.height() as f32 / window_size[1]).round() as u32;
            pixels
                .crop_imm(0, 0, width, height)
                .save(&path)
                .map_err(|e| e.to_string())?;
            output.insert(name.into(), serde_json::json!(path));
        }
        let after = self.engine.state()?;
        for field in ["project", "revision", "canUndo", "canRedo"] {
            if original[field] != after[field] {
                return Err(format!("shortcut capture changed {field}"));
            }
        }
        Ok(())
    }
}
