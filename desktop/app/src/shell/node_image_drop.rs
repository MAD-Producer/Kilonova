use crate::*;

pub(super) fn node_image_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            ["png", "jpg", "jpeg"]
                .iter()
                .any(|supported| ext.eq_ignore_ascii_case(supported))
        })
}
impl Shell {
    pub(crate) fn main_node_image_drop(&mut self, path: &std::path::Path) -> bool {
        if self.home.is_some() || !node_image_path(path) {
            return false;
        }
        let Some(size) = self.size() else {
            return false;
        };
        let Some(frame) = panels::frames(&self.workspace.root, size)
            .into_iter()
            .find(|frame| frame.panel == dock::Panel::Nodes)
        else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };
        let position = self.pointer_position.dropped(window, self.scale);
        if position
            .as_ref()
            .is_ok_and(|point| !frame.content.contains(*point))
        {
            return false;
        }
        self.drop_node_image(path.to_owned(), position, 0)
    }
    pub(crate) fn drop_node_image(
        &mut self,
        path: PathBuf,
        position: std::io::Result<[f32; 2]>,
        window: u64,
    ) -> bool {
        if !node_image_path(&path) {
            return false;
        }
        let point = match position {
            Ok(point) => point,
            Err(error) => {
                log::error!(
                    "node image drop failed window={window} phase=client_position reason={error}"
                );
                self.panels.node_editor.report_error(format!(
                    "{}: {error}",
                    if self.workspace.locale == Locale::Zh {
                        "无法确定图像拖入位置"
                    } else {
                        "Cannot determine image drop position"
                    }
                ));
                self.dirty = true;
                return true;
            }
        };
        if !self.panels.node_editor.canvas_bounds().contains(point) {
            return false;
        }
        if self.node_tasks.importing_image() {
            self.panels
                .node_editor
                .report_status(if self.workspace.locale == Locale::Zh {
                    "图像正在导入，请稍后再拖入"
                } else {
                    "An image is importing; wait before dropping another image"
                });
            self.dirty = true;
            return true;
        }
        let language = if self.workspace.locale == Locale::Zh {
            node_editor::Language::Chinese
        } else {
            node_editor::Language::English
        };
        self.panels.focused = Some(dock::Panel::Nodes);
        if let Some(action) = self.panels.node_editor.image_drop(path, point, language) {
            if let Err(error) = self.node_action(action) {
                log::error!("node image drop failed window={window} phase=enqueue reason={error}");
                self.panels.node_editor.report_error(error);
            }
        }
        self.dirty = true;
        true
    }
}

#[cfg(all(test, target_os = "windows"))]
#[path = "node_image_drop/tests.rs"]
mod tests;
