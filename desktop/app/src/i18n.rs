//! UI-only localization. Core command names and MCP wire keys never depend on language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    Zh,
    En,
}
impl Default for Locale {
    fn default() -> Self {
        if let Ok(value) = std::env::var("MOTION_LOCALE") {
            if let Some(locale) = Self::parse(&value) {
                return locale;
            }
        }
        #[cfg(target_os = "windows")]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            // Reads a process-independent Windows language identifier.
            if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x04 {
                return Self::Zh;
            }
        }
        #[cfg(not(target_os = "windows"))]
        if std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .is_ok_and(|v| v.to_ascii_lowercase().starts_with("zh"))
        {
            return Self::Zh;
        }
        Self::En
    }
}
#[derive(Clone, Copy)]
pub enum Text {
    File,
    Edit,
    Composition,
    Layer,
    Effect,
    Animation,
    View,
    Window,
    Help,
    Project,
    EffectControls,
    Timeline,
    Nodes,
    NodeOpen,
    NodeSave,
    NodeExport,
    NodeWorking,
    NodeCancelled,
    NodeCompiled,
    NodeOpened,
    NodeStaleOpen,
    NodeStaleClose,
    NodeUnsaved,
    NodeUnsavedOpen,
    NodePreviewUnavailable,
    Save,
    ImportMedia,
    Importing,
    Imported,
    CancelImport,
    NewNull,
    NewAdjustment,
    AdjustmentLayer,
    PathLayer,
    Pen,
    Mask,
    SelectTool,
    DrawingHint,
    NewCamera,
    NewRectangle,
    NewEllipse,
    NewText,
    CompositionSettings,
    ExportPng,
    ExportMovie,
    CancelExport,
    Exporting,
    ExportFinished,
    AudioUnavailable,
    SplitLayer,
    AddEffect,
    SearchEffects,
    Enabled,
    Disabled,
    ColorCurve,
    SeparateAxes,
    EnableAnimation,
    DisableAnimation,
    AddKey,
    NoKeys,
    Keyframe,
    KeyframeCurves,
    KeyToPlayhead,
    DeleteKey,
    VelocityCurve,
    ProgressCurve,
    CopyCurve,
    PasteCurve,
    LinearCurve,
    BezierCurve,
    QuadraticCurve,
    ElasticCurve,
    Undo,
    Redo,
    NewSolid,
    NewProject,
    OpenProject,
    ExportPackage,
    Duplicate,
    Delete,
    Enable3D,
    Transform,
    Position,
    Rotation,
    Scale,
    Opacity,
    Effects,
    SelectLayer,
    NoEffects,
    Duration,
    Search,
    SourceName,
    Type,
    ParentLink,
    None,
    Play,
    Pause,
    FirstFrame,
    LastFrame,
    Fit,
    Auto,
    Full,
    Smooth,
    Economy,
    Ready,
    Saved,
    ProjectOpened,
    NewComposition,
    ProjectPackage,
    EnterValid,
    EnterFinite,
    SharedProject,
    Dock,
    Default,
    Language,
    Chinese,
    English,
    About,
    AboutDescription,
    Untitled,
    Solid,
    Items,
    Layers,
    Fps,
    AddPositionKey,
    ShowComposition,
    ShowEffects,
    ResetWorkspace,
    FrameStep,
    OperationFailed,
}
impl Locale {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().split(['-', '_']).next()? {
            "zh" => Some(Self::Zh),
            "en" => Some(Self::En),
            _ => None,
        }
    }
    pub fn t(self, key: Text) -> &'static str {
        use Text::*;
        let (zh, en) = match key {
            File => ("文件", "File"),
            Edit => ("编辑", "Edit"),
            Composition => ("合成", "Composition"),
            Layer => ("图层", "Layer"),
            Effect => ("效果", "Effect"),
            Animation => ("动画", "Animation"),
            View => ("视图", "View"),
            Window => ("窗口", "Window"),
            Help => ("帮助", "Help"),
            Project => ("项目", "Project"),
            EffectControls => ("效果控件", "Effect Controls"),
            Timeline => ("时间轴", "Timeline"),
            Nodes => ("效果节点", "Effect Nodes"),
            NodeOpen => ("打开节点图", "Open Node Graph"),
            NodeSave => ("保存节点图", "Save Node Graph"),
            NodeExport => ("导出节点效果包", "Export Node Effect Package"),
            NodeWorking => ("正在处理节点图…", "Processing node graph…"),
            NodeCancelled => ("节点操作已取消", "Node operation cancelled"),
            NodeCompiled => (
                "节点编译完成，实时预览已更新",
                "Graph compiled; live preview updated",
            ),
            NodeOpened => ("节点图已打开", "Node graph opened"),
            NodeStaleOpen => (
                "打开期间节点图已变化，保留当前编辑；请重新打开",
                "Graph changed while opening; current edits kept. Open again to load",
            ),
            NodeStaleClose => (
                "退出确认期间节点图有新改动，已保留编辑，未退出",
                "Graph changed while confirming exit; new edits kept and the window remains open",
            ),
            NodeUnsaved => ("节点图有未保存的修改", "Unsaved node graph"),
            NodeUnsavedOpen => (
                "当前节点图有未保存的修改。继续打开并替换当前节点图？",
                "The current node graph has unsaved changes. Open another graph and replace it?",
            ),
            NodePreviewUnavailable => (
                "节点图可编译和导出；效果预览尚未连接",
                "Compile and export available; effect preview is not connected",
            ),
            Save => ("保存", "Save"),
            ImportMedia => ("导入素材", "Import Media"),
            Importing => ("正在导入素材", "Importing media"),
            Imported => ("素材已导入", "Media imported"),
            CancelImport => ("取消导入", "Cancel Import"),
            NewNull => ("新建空对象", "New Null Object"),
            NewCamera => ("新建摄影机", "New Camera"),
            NewAdjustment => ("新建调整图层", "New Adjustment Layer"),
            AdjustmentLayer => ("调整图层", "Adjustment Layer"),
            PathLayer => ("路径图层", "Path Layer"),
            Pen => ("钢笔", "Pen"),
            Mask => ("蒙版", "Mask"),
            SelectTool => ("选择", "Select"),
            DrawingHint => (
                "点击加点 · 拖动拉手柄 · Enter 完成 · Esc 取消",
                "Click to add · Drag handles · Enter finish · Esc cancel",
            ),
            NewRectangle => ("新建矩形", "New Rectangle"),
            NewEllipse => ("新建椭圆", "New Ellipse"),
            NewText => ("新建文字", "New Text Layer"),
            CompositionSettings => ("合成设置", "Composition Settings"),
            ExportPng => ("导出当前帧 PNG", "Export Current Frame PNG"),
            ExportMovie => ("导出 MP4 视频", "Export MP4 Movie"),
            CancelExport => ("取消视频导出", "Cancel Movie Export"),
            Exporting => ("正在导出视频", "Exporting movie"),
            ExportFinished => ("视频已导出", "Movie exported"),
            AudioUnavailable => ("音频设备暂不可用", "Audio output unavailable"),
            SplitLayer => ("在播放头处分割图层", "Split Layer at Playhead"),
            AddEffect => ("＋ 添加效果", "+ Add Effect"),
            SearchEffects => ("搜索效果", "Search effects"),
            Enabled => ("开启", "Enabled"),
            Disabled => ("关闭", "Disabled"),
            ColorCurve => ("颜色曲线", "Color Curve"),
            SeparateAxes => ("分离 XYZ", "Separate XYZ"),
            EnableAnimation => ("开启动画", "Enable Animation"),
            DisableAnimation => ("关闭动画", "Disable Animation"),
            AddKey => ("添加关键帧", "Add Keyframe"),
            NoKeys => ("尚无关键帧", "No keyframes"),
            Keyframe => ("关键帧", "Keyframe"),
            KeyframeCurves => ("关键帧与曲线", "Keyframes & Curves"),
            KeyToPlayhead => ("移动到播放头", "Move to Playhead"),
            DeleteKey => ("删除关键帧", "Delete Keyframe"),
            VelocityCurve => ("速度曲线", "Velocity Curve"),
            ProgressCurve => ("进度曲线", "Progress Curve"),
            CopyCurve => ("复制", "Copy"),
            PasteCurve => ("粘贴", "Paste"),
            LinearCurve => ("线性", "Linear"),
            BezierCurve => ("贝塞尔", "Bezier"),
            QuadraticCurve => ("二次", "Quadratic"),
            ElasticCurve => ("弹性", "Elastic"),
            Undo => ("撤销", "Undo"),
            Redo => ("重做", "Redo"),
            NewSolid => ("新建纯色", "New Solid"),
            NewProject => ("新建工程", "New Project"),
            OpenProject => ("打开工程", "Open Project"),
            ExportPackage => ("导出工程包", "Export Project Package"),
            Duplicate => ("复制图层", "Duplicate"),
            Delete => ("删除", "Delete"),
            Enable3D => ("启用 3D", "Enable 3D"),
            Transform => ("变换", "Transform"),
            Position => ("位置", "Position"),
            Rotation => ("旋转", "Rotation"),
            Scale => ("缩放", "Scale"),
            Opacity => ("不透明度", "Opacity"),
            Effects => ("效果", "Effects"),
            SelectLayer => ("在时间轴中选择一个图层", "Select a layer in the timeline"),
            NoEffects => ("未添加效果", "No effects applied"),
            Duration => ("时长", "Duration"),
            Search => ("搜索项目", "Search project"),
            SourceName => ("源名称", "Source Name"),
            Type => ("类型", "Type"),
            ParentLink => ("父级与链接", "Parent & Link"),
            None => ("无", "None"),
            Play => ("播放", "Play"),
            Pause => ("暂停", "Pause"),
            FirstFrame => ("第一帧", "First Frame"),
            LastFrame => ("最后一帧", "Last Frame"),
            Fit => ("适应", "Fit"),
            Auto => ("自动", "Auto"),
            Full => ("完整", "Full"),
            Smooth => ("流畅", "Smooth"),
            Economy => ("节能", "Economy"),
            Ready => ("就绪", "Ready"),
            Saved => ("已保存", "Saved"),
            ProjectOpened => ("工程已打开", "Project opened"),
            NewComposition => ("已新建 1920 × 1080 合成", "New 1920 × 1080 composition"),
            ProjectPackage => ("工程包", "Project package"),
            EnterValid => (
                "请输入有效数值，Esc 取消",
                "Enter a valid number; Esc cancels",
            ),
            EnterFinite => ("请输入有限数值", "Enter a finite number"),
            SharedProject => ("共享工程与时间轴", "Shared project / timeline"),
            Dock => ("停靠", "Dock"),
            Default => ("默认", "Default"),
            Language => ("语言", "Language"),
            Chinese => ("简体中文", "简体中文"),
            English => ("English", "English"),
            About => ("关于 Motion Studio", "About Motion Studio"),
            AboutDescription => (
                "Motion Studio · AI 自主驱动的创作工具实验",
                "Motion Studio · An AI-driven creative tool experiment",
            ),
            Untitled => ("未命名工程", "Untitled Project"),
            Solid => ("纯色", "Solid"),
            Items => ("项目", "items"),
            Layers => ("图层", "layers"),
            Fps => ("帧/秒", "fps"),
            AddPositionKey => ("启用位置关键帧", "Enable Position Keyframes"),
            ShowComposition => ("合成面板", "Composition Panel"),
            ShowEffects => ("效果控件面板", "Effect Controls Panel"),
            ResetWorkspace => ("重置默认工作区", "Reset Default Workspace"),
            FrameStep => ("方向键：逐帧", "Arrows: frame step"),
            OperationFailed => ("操作失败", "Operation failed"),
        };
        if self == Self::Zh {
            zh
        } else {
            en
        }
    }
    pub fn panel(self, id: motion_ui::dock::DockId) -> &'static str {
        motion_ui::dock::Panel::from_dock(id)
            .map(|p| {
                if self == Self::Zh {
                    p.title_zh()
                } else {
                    p.title()
                }
            })
            .unwrap_or(self.t(Text::Window))
    }
    pub fn error(self, error: &str) -> String {
        if self == Self::Zh && error.starts_with("project is already open") {
            return "工程已被另一个进程打开，请关闭后再试。".into();
        }
        format!("{}: {error}", self.t(Text::OperationFailed))
    }
}
