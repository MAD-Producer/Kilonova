//! Real native painting/input and workspace persistence checks for node search.
use crate::*;
use motion_ui::{input::Modifiers, Rect};

impl Shell {
    fn install_search_preview(&mut self) -> Result<(), String> {
        let snapshot = self.panels.node_editor.snapshot();
        let compiled = snapshot.compile_preview().map_err(|e| e.to_string())?;
        let report = compiled.report.clone();
        self.engine.node_expect(snapshot.preview_stamp())?;
        if !self
            .engine
            .node_install(snapshot.clone(), compiled)?
            .recv_timeout(Duration::from_secs(30))
            .map_err(|e| format!("search preview install: {e}"))??
        {
            return Err("search preview installation was superseded".into());
        }
        self.panels
            .node_editor
            .accept_compile(&snapshot, Ok(report));
        self.panels.node_preview.active = Some(snapshot.preview_stamp());
        Ok(())
    }

    pub(super) fn node_search_acceptance(&mut self, floating: bool) -> Result<Value, String> {
        let graph = self.panels.node_editor.snapshot().document;
        let project = self.engine.state()?["revision"].clone();
        let mut scratch = node_editor::NodeEditor::new(graph.clone());
        scratch.view = self.panels.node_editor.view;
        scratch.view_preferences = self.panels.node_editor.view_preferences;
        let original = std::mem::replace(&mut self.panels.node_editor, scratch);
        let previous_recents = self.workspace.node_recents.clone();
        let result = (|| {
            self.install_search_preview()?;
            self.panels.focused = Some(dock::Panel::Nodes);
            self.panels.node_editor.focus_panel();
            self.node_search_frame(floating, Input::default(), "search-initial", true)?;
            let panel = if floating {
                let f = self
                    .floating
                    .values()
                    .find(|f| f.panel == dock::Panel::Nodes)
                    .ok_or("node search floating window missing")?;
                let size = f.window.inner_size();
                Rect::new(
                    0.,
                    30.,
                    size.width as f32 / f.scale,
                    (size.height as f32 / f.scale - 52.).max(1.),
                )
            } else {
                panels::frames(&self.workspace.root, self.size().ok_or("window missing")?)
                    .into_iter()
                    .find(|f| f.panel == dock::Panel::Nodes)
                    .ok_or("node search panel missing")?
                    .content
            };
            let body = Rect::new(
                panel.min[0],
                panel.min[1] + 28.,
                panel.width(),
                panel.height() - 28.,
            );
            let area = node_preview::regions(body).0;
            let layout = self.panels.node_editor.layout(area);
            let point = [layout.canvas.min[0] + 16., layout.canvas.min[1] + 16.];
            let expected_position = self
                .panels
                .node_editor
                .view
                .screen_to_world(point, layout.canvas);
            let key = |key| Input {
                keys: vec![(key, Default::default())],
                ..Default::default()
            };
            let open = || Input {
                mouse: point,
                keys: vec![(
                    Key::A,
                    Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                )],
                ..Default::default()
            };
            let browse = self.node_search_frame(floating, open(), "search-browse", true)?;
            if !self.panels.node_editor.popup_open() {
                return Err("Shift+A did not open node search".into());
            }
            let query = self.node_search_frame(
                floating,
                Input {
                    text: if self.workspace.locale == Locale::Zh {
                        "yuanxing"
                    } else {
                        "circle"
                    }
                    .into(),
                    ..Default::default()
                },
                "search-query",
                true,
            )?;
            let mut ime = key(Key::Enter);
            ime.ime_consumed = true;
            self.node_search_frame(floating, ime, "search-ime", true)?;
            if self.panels.node_editor.snapshot().document != graph {
                return Err("IME confirmation added a node".into());
            }
            self.node_search_frame(floating, key(Key::Enter), "search-added", true)?;
            let document = self.panels.node_editor.document();
            if document.nodes.len() != graph.nodes.len() + 1
                || !document.nodes.last().is_some_and(|n| {
                    matches!(n.operation, motion_graph::NodeOp::SdfCircle)
                        && n.position == expected_position
                })
                || self.panels.node_editor.popup_open()
            {
                return Err("native query did not add Circle at the latched pointer".into());
            }
            self.panels.node_editor.undo();
            self.install_search_preview()?;
            self.panels.node_editor.focus_panel();
            self.node_search_frame(floating, open(), "search-reopened", true)?;
            let styles = PaintList::default().styles();
            let pad = styles.metric("padding.panel", 8.);
            let width = styles
                .metric("node.add_menu.width", 512.)
                .min(area.width() - pad * 2.);
            let height = styles
                .metric("node.add_menu.height", 440.)
                .min(area.height() - pad * 2.);
            let origin = [
                point[0].clamp(area.min[0] + pad, area.max[0] - width - pad),
                point[1].clamp(area.min[1] + pad, area.max[1] - height - pad),
            ];
            let recent_point = [
                origin[0] + pad * 2.,
                origin[1]
                    + pad * 2.
                    + styles.metric("viewer.field.height", 26.)
                    + styles.metric("row.height", 22.) * 1.5,
            ];
            self.node_search_frame(
                floating,
                Input {
                    mouse: recent_point,
                    pressed: Some((motion_ui::input::MouseButton::Left, recent_point)),
                    ..Default::default()
                },
                "search-recent-selected",
                true,
            )?;
            let recent =
                self.node_search_frame(floating, Input::default(), "search-recent", true)?;
            self.node_search_frame(floating, key(Key::Escape), "search-cancelled", true)?;
            if self.panels.node_editor.popup_open() {
                return Err("Escape did not close node search".into());
            }
            // Escape currently releases node focus at the shell boundary (A8).
            // Re-focus before the next independent search probe.
            self.panels.node_editor.focus_panel();
            self.node_search_frame(floating, open(), "search-empty-open", true)?;
            if !self.panels.node_editor.popup_open() {
                return Err("empty-results probe did not open node search".into());
            }
            let empty = self.node_search_frame(
                floating,
                Input {
                    text: "no_such_node".into(),
                    ..Default::default()
                },
                "search-no-match",
                true,
            )?;
            self.node_search_frame(floating, key(Key::Enter), "search-empty-enter", true)?;
            if !self.panels.node_editor.popup_open() || self.panels.node_editor.document() != &graph
            {
                return Err("empty search changed source or closed without a choice".into());
            }
            self.node_search_frame(floating, key(Key::Escape), "search-empty-cancelled", true)?;
            let library = layout.library.ok_or("node search library missing")?;
            let gap = styles.metric("padding", 5.);
            let line = styles.metric("row.height", 22.);
            let field_height = styles.metric("viewer.field.height", 26.);
            let compact_library = library.height()
                < pad * 3.
                    + gap * 2.
                    + line * 2.
                    + field_height
                    + styles.metric("node.search.result_height", 44.)
                    + styles.metric("node.search.detail_height", 76.);
            let field = [
                library.min[0] + pad * 2.,
                library.min[1]
                    + pad
                    + if compact_library { 0. } else { line + gap }
                    + styles.metric("viewer.field.height", 26.) * 0.5,
            ];
            self.node_search_frame(
                floating,
                Input {
                    mouse: field,
                    pressed: Some((motion_ui::input::MouseButton::Left, field)),
                    ..Default::default()
                },
                "search-library-focused",
                true,
            )?;
            let library_query = self.node_search_frame(
                floating,
                Input {
                    text: "sqrt".into(),
                    ..Default::default()
                },
                "search-library-query",
                true,
            )?;
            self.node_search_frame(floating, key(Key::Enter), "search-library-added", true)?;
            if self.panels.node_editor.document().nodes.len() != graph.nodes.len() + 1
                || !self
                    .panels
                    .node_editor
                    .document()
                    .nodes
                    .last()
                    .is_some_and(|n| {
                        matches!(
                            n.operation,
                            motion_graph::NodeOp::Math {
                                operation: motion_graph::MathOp::Sqrt,
                                ..
                            }
                        )
                    })
            {
                return Err("native library query did not add Sqrt".into());
            }
            self.panels.node_editor.undo();
            self.install_search_preview()?;
            self.node_search_frame(
                floating,
                Input {
                    mouse: field,
                    pressed: Some((motion_ui::input::MouseButton::Left, field)),
                    ..Default::default()
                },
                "search-library-refocused",
                true,
            )?;
            self.node_search_frame(
                floating,
                key(Key::Backspace),
                "search-library-cleared",
                true,
            )?;
            let library_recent =
                self.node_search_frame(floating, Input::default(), "search-library-recent", true)?;
            if self.panels.node_editor.document() != &graph
                || self.engine.state()?["revision"] != project
            {
                return Err("search acceptance did not restore graph/project source".into());
            }
            if workspace::Workspace::load(&self.layout_path).node_recents
                != self.panels.node_editor.recent_nodes
            {
                return Err("node search recents did not survive workspace reload".into());
            }
            if serde_json::to_value(&self.workspace.node_recents)
                .map_err(|e| e.to_string())?
                .as_array()
                .map(Vec::len)
                != Some(2)
            {
                return Err("native search did not record exactly two successful choices".into());
            }
            Ok(
                serde_json::json!({"browse":browse,"query":query,"recent":recent,"empty":empty,
                "libraryQuery":library_query,"libraryRecent":library_recent,
                "latchedPlacement":true,"imeConfirmation":true,"emptySearchSafe":true,
                "sourceRestored":true,"projectUntouched":true,"workspaceReload":true}),
            )
        })();
        self.panels.node_editor = original;
        self.workspace.node_recents = previous_recents;
        let save = self.workspace.save(&self.layout_path);
        let install = self.install_search_preview();
        let cleanup = match (save, install) {
            (Err(save), Err(install)) => Err(format!(
                "{save}; restoring original preview also failed: {install}"
            )),
            (Err(reason), _) | (_, Err(reason)) => Err(reason),
            (Ok(()), Ok(())) => Ok(()),
        };
        match (result, cleanup) {
            (Err(reason), Err(cleanup)) => Err(format!(
                "{reason}; restoring search recents also failed: {cleanup}"
            )),
            (Err(reason), _) | (_, Err(reason)) => Err(reason),
            (Ok(record), Ok(())) => Ok(record),
        }
    }
}
