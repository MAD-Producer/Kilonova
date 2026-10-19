//! A native text transaction; opening or cancelling never changes the graph.
use super::super::naming::RenameTarget;
use super::*;
use motion_ui::widgets::TextEvent;

impl NodeEditor {
    pub(super) fn paint_rename(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        bounds: Rect,
        input: &Input,
        language: Language,
    ) {
        let Some(mut edit) = self.rename.take() else {
            return;
        };
        let styles = list.styles();
        let colors = styles.colors();
        let area = control_popup::popup_rect(
            edit.position,
            bounds,
            [
                styles.metric("node.rename.width", 360.),
                styles.metric("node.rename.height", 172.),
            ],
        );
        let pad = styles.metric("node.rename.padding", 12.);
        let row = styles.metric("node.rename.row", 28.);
        let gap = styles.metric("node.rename.gap", 8.);
        let button = styles.metric("node.rename.button_width", 90.);
        let neutral = Input::default();
        let input = if edit.fresh {
            edit.fresh = false;
            &neutral
        } else {
            input
        };
        if input.focus_lost || input.pressed.is_some_and(|(_, p)| !area.contains(p)) {
            return;
        }
        let mut text_input = input.clone();
        // Retain one excess character so an oversized paste is rejected rather
        // than silently saved as a different name. Bound the temporary payload.
        text_input.text = text_input.text.chars().take(121).collect();
        let previous = edit.editor.text.clone();
        let event = edit.editor.input(&text_input);
        let oversized = edit.editor.text.chars().count() > 120;
        if oversized {
            edit.editor.begin(&previous);
            edit.editor.error = Some(
                language
                    .text("名称最多 120 字节", "Use at most 120 UTF-8 bytes")
                    .into(),
            );
        }
        if event == Some(TextEvent::Cancel) {
            return;
        }
        let mut popup = list.child();
        paint::rect(&mut popup, area, colors.CHROME);
        paint::stroke(&mut popup, area, colors.BORDER, styles.metric("border", 1.));
        let x = area.min[0] + pad;
        let width = (area.width() - pad * 2.).max(0.);
        label(
            &mut popup,
            atlas,
            Rect::new(x, area.min[1] + pad, width, row),
            language.text("重命名", "Rename"),
            colors.TEXT,
            Align::Left,
        );
        edit.editor.draw(
            &mut popup,
            atlas,
            Rect::new(x, area.min[1] + pad + row + gap, width, row),
        );
        label(
            &mut popup,
            atlas,
            Rect::new(x, area.min[1] + pad + (row + gap) * 2., width, row),
            edit.editor.error.as_deref().unwrap_or_else(|| {
                language.text(
                    "仅修改显示名 · ID 和连线保持不变",
                    "Display name only · IDs and wires stay unchanged",
                )
            }),
            colors.TEXT_MUTED,
            Align::Left,
        );
        let y = area.max[1] - pad - row;
        let cancel = ui::button(
            &mut popup,
            atlas,
            Rect::new(area.max[0] - pad - button * 2. - gap, y, button, row),
            language.text("取消", "Cancel"),
            input,
            true,
        )
        .clicked;
        let apply = ui::button(
            &mut popup,
            atlas,
            Rect::new(area.max[0] - pad - button, y, button, row),
            language.text("确定", "Apply"),
            input,
            true,
        )
        .clicked;
        popup.clip_to(area);
        list.append(popup);
        if cancel {
            return;
        }
        if !oversized
            && (apply || matches!(event, Some(TextEvent::Commit(_))))
            && !input.composing
            && !input.ime_consumed
        {
            let result = match &edit.target {
                RenameTarget::Node(id) => self.rename_node(id, &edit.editor.text),
                RenameTarget::Port { node, output, key } => {
                    self.rename_group_port(node, *output, key, &edit.editor.text)
                }
            };
            match result {
                Ok(()) => return,
                Err(error) => edit.editor.error = Some(error.message),
            }
        }
        self.rename = Some(edit);
    }
}
