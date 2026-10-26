//! Command capture is a draft transaction, with its own bounded undo history.
use super::*;
use crate::shortcuts::{
    bindings::{self, Bindings},
    Command, Scope,
};
use motion_ui::input::KeyChord;

#[derive(Default)]
pub(super) struct Shortcuts {
    scope: usize,
    command: usize,
    scope_menu: Dropdown,
    command_menu: Dropdown,
    pub(super) capturing: bool,
    pub(super) error: Option<String>,
    undo: Vec<Bindings>,
    redo: Vec<Bindings>,
}
impl Shortcuts {
    pub(super) fn owns_input(&self) -> bool {
        self.capturing || self.scope_menu.open || self.command_menu.open
    }
    pub(super) fn close(&mut self) {
        self.capturing = false;
        self.scope_menu.open = false;
        self.command_menu.open = false;
    }
    pub(super) fn replace(&mut self, draft: &mut Bindings, next: Bindings) {
        if *draft != next {
            if self.undo.len() == 64 {
                self.undo.remove(0);
            }
            self.undo.push(std::mem::replace(draft, next));
            self.redo.clear();
        }
        self.error = None;
    }
    fn history(&mut self, draft: &mut Bindings, redo: bool) {
        let (source, target) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        if let Some(next) = source.pop() {
            target.push(std::mem::replace(draft, next));
            self.error = None;
        }
    }
    pub(super) fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        bounds: Rect,
        input: &Input,
        locale: Locale,
        draft: &mut Bindings,
    ) -> Option<Event> {
        let sheet = list.styles();
        let row = sheet.metric("row.height", 22.);
        let gap = sheet.metric("auxiliary.gap", 6.);
        let scope = [Scope::Application, Scope::Animation, Scope::Nodes, Scope::Viewer][self.scope];
        let entries: Vec<_> = bindings::catalogue()
            .iter()
            .filter(|e| e.scope == scope)
            .collect();
        self.command = self.command.min(entries.len().saturating_sub(1));
        let entry = entries[self.command];
        let owned = self.owns_input();
        let neutral = Input::default();
        let controls = if owned { &neutral } else { input };
        if self.capturing && !input.composing && !input.ime_consumed {
            for (key, modifiers, repeat) in input.key_presses() {
                if repeat || matches!(key, Key::Shift | Key::Control | Key::Alt) {
                    continue;
                }
                let mut next = draft.clone();
                match next.bind(scope, entry.command, Some(KeyChord { key, modifiers })) {
                    Ok(()) => {
                        self.replace(draft, next);
                        self.capturing = false;
                    }
                    Err(error) => self.error = Some(error),
                }
                break;
            }
        } else if !owned && !input.composing && !input.ime_consumed {
            for (key, m, repeat) in input.key_presses() {
                if key == Key::Z && m.control && !m.alt && !repeat {
                    self.history(draft, m.shift);
                }
            }
        }
        let label_width = sheet
            .metric("preferences.label_width", 180.)
            .min(area.width() * 0.35);
        let field = |y| {
            Rect::new(
                area.min[0] + label_width + gap,
                y,
                (area.width() - label_width - gap).max(0.),
                row,
            )
        };
        let text = |list: &mut PaintList, atlas: &mut TextAtlas, y, label: &str| {
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, area.width(), row),
                label,
                sheet.color("text"),
                Align::Left,
            )
        };
        let mut y = area.min[1];
        text(list, atlas, y, &word(locale, "作用域", "Scope"));
        let labels = [
            word(locale, "应用", "Application"),
            word(locale, "动画", "Animation"),
            word(locale, "节点", "Nodes"),
            word(locale, "查看器工具", "Viewer tools"),
        ];
        let scopes = labels
            .iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect::<Vec<_>>();
        let mut overlay = list.child();
        if let Some(index) = self.scope_menu.draw(
            &mut overlay,
            atlas,
            field(y),
            bounds,
            &scopes,
            self.scope,
            if self.scope_menu.open {
                input
            } else {
                controls
            },
            !self.capturing && !self.command_menu.open,
            false,
        ) {
            self.scope = index;
            self.command = 0;
            self.error = None;
        }
        y += row + gap;
        text(list, atlas, y, &word(locale, "命令", "Command"));
        let labels = entries
            .iter()
            .map(|e| command_name(locale, e.command))
            .collect::<Vec<_>>();
        let items = labels
            .iter()
            .map(|label| DropdownItem {
                label,
                enabled: true,
            })
            .collect::<Vec<_>>();
        if let Some(index) = self.command_menu.draw(
            &mut overlay,
            atlas,
            field(y),
            bounds,
            &items,
            self.command,
            if self.command_menu.open {
                input
            } else {
                controls
            },
            !self.capturing && !self.scope_menu.open,
            false,
        ) {
            self.command = index;
            self.error = None;
        }
        y += row + gap;
        let hint = draft.hint(scope, entry.command);
        text(list, atlas, y, &word(locale, "当前键位", "Current binding"));
        y += row;
        text(
            list,
            atlas,
            y,
            if self.capturing {
                if locale == Locale::Zh {
                    "请按组合键；Esc 取消录入"
                } else {
                    "Press a chord; Esc cancels capture"
                }
            } else if hint.is_empty() {
                if locale == Locale::Zh {
                    "未绑定"
                } else {
                    "Unassigned"
                }
            } else {
                &hint
            },
        );
        y += row + gap;
        let width = ((area.width() - gap * 2.) / 3.).max(0.);
        let button =
            |list: &mut PaintList, atlas: &mut TextAtlas, index: usize, y, label: &str, enabled| {
                ui::button(
                    list,
                    atlas,
                    Rect::new(area.min[0] + index as f32 * (width + gap), y, width, row),
                    label,
                    controls,
                    enabled,
                )
                .clicked
            };
        if button(list, atlas, 0, y, &word(locale, "录入键位", "Record"), true) {
            self.capturing = true;
            self.error = None;
        }
        if button(
            list,
            atlas,
            1,
            y,
            &word(locale, "解除绑定", "Unassign"),
            !hint.is_empty(),
        ) {
            let mut next = draft.clone();
            match next.bind(scope, entry.command, None) {
                Ok(()) => self.replace(draft, next),
                Err(e) => self.error = Some(e),
            }
        }
        if button(
            list,
            atlas,
            2,
            y,
            &word(locale, "命令默认", "Default key"),
            true,
        ) {
            let mut next = draft.clone();
            match next.reset_command(scope, entry.command) {
                Ok(()) => self.replace(draft, next),
                Err(e) => self.error = Some(e),
            }
        }
        y += row + gap;
        let import = button(
            list,
            atlas,
            0,
            y,
            &word(locale, "导入预设…", "Import…"),
            true,
        );
        let export = button(
            list,
            atlas,
            1,
            y,
            &word(locale, "导出预设…", "Export…"),
            true,
        );
        if button(
            list,
            atlas,
            2,
            y,
            &word(locale, "重置键位", "Reset keys"),
            true,
        ) {
            self.replace(draft, Bindings::default());
        }
        y += row + gap;
        if button(
            list,
            atlas,
            0,
            y,
            &word(locale, "撤销改键", "Undo keys"),
            !self.undo.is_empty(),
        ) {
            self.history(draft, false);
        }
        if button(
            list,
            atlas,
            1,
            y,
            &word(locale, "重做改键", "Redo keys"),
            !self.redo.is_empty(),
        ) {
            self.history(draft, true);
        }
        y += row + gap;
        text(
            list,
            atlas,
            y,
            &word(
                locale,
                "应用后生效；取消保留原设置。",
                "Apply activates; Cancel keeps saved keys.",
            ),
        );
        y += row;
        text(
            list,
            atlas,
            y,
            &word(
                locale,
                "文本、输入法和弹层优先。",
                "Text, IME and popups take priority.",
            ),
        );
        y += row;
        if let Some(error) = &self.error {
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, area.width(), row * 2.),
                &word(locale, &format!("无法绑定：{error}"), error),
                sheet.color("error"),
                Align::Left,
            );
        }
        overlay.clip_to(bounds);
        list.append(overlay);
        if import {
            Some(Event::PreferencesImportKeys)
        } else if export {
            Some(Event::PreferencesExportKeys)
        } else {
            None
        }
    }
}

pub(crate) fn command_name(locale: Locale, command: Command) -> String {
    use Command::*;
    let (zh, en) = match command {
        ToolSelect => ("选择工具", "Selection tool"),
        ToolPen => ("钢笔工具", "Pen tool"),
        ToolAnchor => ("锚点工具", "Anchor tool"),
        ToolRotate => ("旋转工具", "Rotation tool"),
        ToolShape => ("循环形状工具", "Cycle shape tools"),
        ToolText => ("文字工具", "Text tool"),
        ToolHand => ("抓手工具", "Hand tool"),
        ToolZoom => ("缩放工具", "Zoom tool"),
        ToolCamera => ("观察摄影机工具", "Observation camera tool"),
        ReloadStyle => ("重载样式", "Reload style"),
        TogglePanelMaximize => ("最大化面板", "Maximize panel"),
        NewProject => ("新建工程", "New project"),
        NewComposition => ("新建合成", "New composition"),
        Open => ("打开文档", "Open document"),
        Save => ("保存文档", "Save document"),
        SaveAs => ("另存文档", "Save document as"),
        Import => ("导入媒体", "Import media"),
        Settings => ("合成设置", "Composition settings"),
        Preferences => ("偏好设置", "Preferences"),
        Export => ("导出视频", "Export movie"),
        Undo => ("撤销", "Undo"),
        Redo => ("重做", "Redo"),
        Play => ("播放切换", "Toggle playback"),
        PreviousFrame => ("上一帧", "Previous frame"),
        NextFrame => ("下一帧", "Next frame"),
        FirstFrame => ("首帧", "First frame"),
        LastFrame => ("末帧", "Last frame"),
        GoToIn => ("跳到入点", "Go to in point"),
        GoToOut => ("跳到出点", "Go to out point"),
        WorkAreaIn => ("设置工作范围起点", "Set work area start"),
        WorkAreaOut => ("设置工作范围终点", "Set work area end"),
        GoToWorkAreaIn => ("跳到工作范围首帧", "Go to work area first frame"),
        GoToWorkAreaOut => ("跳到工作范围末帧", "Go to work area last frame"),
        MoveInToPlayhead => ("入点移到播放头", "Move in to playhead"),
        MoveOutToPlayhead => ("出点移到播放头", "Move out to playhead"),
        TrimInToPlayhead => ("裁剪入点", "Trim in to playhead"),
        TrimOutToPlayhead => ("裁剪出点", "Trim out to playhead"),
        ToggleCompositeColumns => ("切换混合与遮罩列", "Toggle mode/matte columns"),
        ToggleParentColumns => ("切换父级列", "Toggle parent column"),
        CopyNodes => ("复制节点", "Copy nodes"),
        CopySelection => ("复制当前选择", "Copy selection"),
        CutSelection => ("剪切当前选择", "Cut selection"),
        PasteSelection => ("在播放头粘贴", "Paste at playhead"),
        PasteInPlace => ("原位粘贴", "Paste in place"),
        CutNodes => ("剪切节点", "Cut nodes"),
        PasteNodes => ("粘贴节点", "Paste nodes"),
        Duplicate => ("复制选择", "Duplicate selection"),
        Split => ("分割图层", "Split layer"),
        Delete => ("删除选择", "Delete selection"),
        Solid => ("新建纯色", "New solid"),
        Text => ("新建文字", "New text"),
        Null => ("新建空对象", "New null"),
        Adjustment => ("新建调整层", "New adjustment"),
        SelectAll => ("全选", "Select all"),
        DeselectAll => ("取消全选", "Deselect all"),
        FitAll => ("适配全部", "Fit all"),
        FitSelection => ("适配选择", "Fit selection"),
        AddNode => ("添加节点", "Add node"),
        ResetZoom => ("重置缩放", "Reset zoom"),
        ZoomIn => ("放大时间轴", "Zoom in timeline"),
        ZoomOut => ("缩小时间轴", "Zoom out timeline"),
        ToggleFrameView => ("单帧与整段视图", "Toggle frame view"),
        ToggleNodeGrid => ("显示节点网格", "Toggle node grid"),
        ToggleNodeMinimap => ("显示节点小地图", "Toggle minimap"),
        MoveLeft => ("节点左移", "Move node left"),
        MoveRight => ("节点右移", "Move node right"),
        MoveUp => ("节点上移", "Move node up"),
        MoveDown => ("节点下移", "Move node down"),
        SelectLayerAbove => ("选择上方图层", "Select layer above"),
        SelectLayerBelow => ("选择下方图层", "Select layer below"),
        OrderLayersUp => ("图层上移一层", "Move layers up"),
        OrderLayersDown => ("图层下移一层", "Move layers down"),
        OrderLayersTop => ("图层置顶", "Move layers to top"),
        OrderLayersBottom => ("图层置底", "Move layers to bottom"),
        Arrange => ("排列节点", "Arrange nodes"),
        Group => ("节点分组", "Group nodes"),
        Ungroup => ("解包节点组", "Ungroup nodes"),
        EnterGroup => ("进入节点组", "Enter group"),
        LeaveGroup => ("返回父图", "Leave group"),
        AnimatedProperties => ("显示动画属性", "Reveal animated properties"),
        ModifiedProperties => ("显示已修改属性", "Reveal modified properties"),
        PreviousKey => ("上一个关键帧", "Previous keyframe"),
        NextKey => ("下一个关键帧", "Next keyframe"),
        NudgeKeyEarlier => ("关键帧提前一帧", "Nudge keys one frame earlier"),
        NudgeKeyLater => ("关键帧延后一帧", "Nudge keys one frame later"),
        NudgeKeysEarlierTen => ("关键帧提前十帧", "Nudge keys ten frames earlier"),
        NudgeKeysLaterTen => ("关键帧延后十帧", "Nudge keys ten frames later"),
        ToggleGraph => ("切换值曲线", "Toggle value graph"),
        EaseBoth => ("双侧缓动", "Ease both"),
        EaseIncoming => ("入段缓动", "Ease incoming"),
        EaseOutgoing => ("出段缓动", "Ease outgoing"),
        KeyframeTiming => ("关键帧插值与速度", "Keyframe interpolation and velocity"),
        Reveal(property) | ToggleKey(property) => {
            let text = match property {
                motion_core::Property::Anchor => T::Anchor,
                motion_core::Property::Position => T::Position,
                motion_core::Property::Scale => T::Scale,
                motion_core::Property::Rotation => T::Rotation,
                motion_core::Property::Opacity => T::Opacity,
                _ => return format!("{command:?}"),
            };
            return format!(
                "{} {}",
                word(
                    locale,
                    if matches!(command, Reveal(_)) {
                        "显示属性"
                    } else {
                        "切换关键帧"
                    },
                    if matches!(command, Reveal(_)) {
                        "Reveal"
                    } else {
                        "Toggle key"
                    }
                ),
                locale.t(text)
            );
        }
    };
    word(locale, zh, en)
}
