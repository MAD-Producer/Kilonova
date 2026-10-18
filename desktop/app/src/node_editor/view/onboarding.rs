//! Guidance never covers nodes or implicitly replaces a graph.
use super::*;
use motion_ui::style::StyleSheet;

pub(super) struct WelcomeLayout {
    pub area: Rect,
    pub examples: Rect,
    pub dismiss: Option<Rect>,
}

impl NodeEditor {
    fn welcome_visible(&self) -> bool {
        self.parents.is_empty()
            && self.layer_target.is_none()
            && (self.document.nodes.is_empty() || self.welcome)
    }

    pub(super) fn welcome_layout(&self, area: Rect, styles: &StyleSheet) -> Option<WelcomeLayout> {
        if !self.welcome_visible() {
            return None;
        }
        let canvas = self.layout(area).canvas;
        let row = styles.metric("row.height", 22.);
        let padding = styles.metric("padding.panel", 8.);
        let height = row * 5. + padding * 2.;
        // Keep both instructions and the actual canvas usable in small docks.
        if canvas.width() < (row * 16.).max(styles.metric("font.body", 12.) * 30.)
            || canvas.height() < height + row * 5.
        {
            return None;
        }
        let bounds = Rect::new(canvas.min[0], canvas.min[1], canvas.width(), height);
        let examples = Rect::new(
            bounds.min[0] + padding,
            bounds.max[1] - padding - row,
            row * 5.,
            row,
        );
        Some(WelcomeLayout {
            area: bounds,
            examples,
            dismiss: (!self.document.nodes.is_empty())
                .then(|| Rect::new(examples.max[0] + padding, examples.min[1], row * 5., row)),
        })
    }

    pub(super) fn guided_layout(&self, area: Rect, styles: &StyleSheet) -> Layout {
        let mut layout = self.layout(area);
        if let Some(welcome) = self.welcome_layout(area, styles).filter(|_| self.welcome) {
            layout.canvas.min[1] = welcome.area.max[1];
        }
        layout
    }

    /// Read-only hit targets used by the native UI acceptance path.
    pub(crate) fn onboarding_controls(&self, styles: &StyleSheet) -> Option<(Rect, Option<Rect>)> {
        let layout = self.welcome_layout(self.last_area, styles)?;
        Some((layout.examples, layout.dismiss))
    }

    pub(super) fn welcome_input(
        &mut self,
        area: Rect,
        input: &Input,
        styles: &StyleSheet,
        actions: &mut Vec<Action>,
    ) -> bool {
        if self.popup_open() {
            return false;
        }
        let Some(layout) = self.welcome_layout(area, styles) else {
            return false;
        };
        let occupied = input
            .pressed
            .is_some_and(|(_, point)| layout.area.contains(point))
            || input
                .released
                .is_some_and(|(_, point)| layout.area.contains(point));
        let Some((MouseButton::Left, point)) = input.pressed else {
            return occupied;
        };
        if layout.examples.contains(point) {
            match self.request(Request::Examples) {
                Ok(action) => actions.push(action),
                Err(error) => self.notice = Some(error.to_string()),
            }
        } else if layout.dismiss.is_some_and(|rect| rect.contains(point)) {
            self.welcome = false;
            self.view.pan[1] += layout.area.height();
        }
        occupied
    }

    pub(super) fn paint_welcome(
        &self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        language: Language,
    ) {
        let styles = list.styles();
        let Some(layout) = self.welcome_layout(area, &styles) else {
            return;
        };
        let row = styles.metric("row.height", 22.);
        let padding = styles.metric("padding.panel", 8.);
        let colors = styles.colors();
        let mut child = list.child();
        paint::rect(&mut child, layout.area, colors.PANEL);
        let empty = self.document.nodes.is_empty();
        let lines = [
            if empty {
                language.text(
                    "画布为空，从两个节点开始",
                    "Empty canvas: start with two nodes",
                )
            } else {
                language.text("从两个节点开始", "Start with two nodes")
            },
            if empty {
                language.text(
                    "也可以按 Shift+A，自由添加节点。",
                    "Or press Shift+A to build your own graph.",
                )
            } else {
                language.text(
                    "当前是可编辑的动画示例。",
                    "This is an editable animation example.",
                )
            },
            language.text(
                "1 打开示例库　2 打开「纯色入门」",
                "1 Examples   2 Open Solid Color",
            ),
            language.text(
                "3 选择颜色节点，修改颜色查看结果。",
                "3 Select Color and adjust it to see the result.",
            ),
        ];
        for (index, text) in lines.into_iter().enumerate() {
            label(
                &mut child,
                atlas,
                Rect::new(
                    layout.area.min[0] + padding,
                    layout.area.min[1] + padding + row * index as f32,
                    layout.area.width() - padding * 2.,
                    row,
                ),
                text,
                if index == 0 {
                    colors.TEXT
                } else {
                    colors.TEXT_MUTED
                },
                Align::Left,
            );
        }
        ui::button(
            &mut child,
            atlas,
            layout.examples,
            language.text("示例库", "Examples"),
            input,
            true,
        );
        if let Some(dismiss) = layout.dismiss {
            ui::button(
                &mut child,
                atlas,
                dismiss,
                language.text("继续编辑", "Continue"),
                input,
                true,
            );
        }
        child.clip_to(layout.area);
        list.append(child);
    }
}
