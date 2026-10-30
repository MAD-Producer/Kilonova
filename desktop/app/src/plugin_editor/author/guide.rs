//! Offline, in-product help; navigation never modifies the authored document.
use super::*;
#[derive(Default)]
pub struct Guide {
    tab: usize,
    scroll: f32,
}
pub(super) fn wrapped(
    list: &mut PaintList,
    atlas: &mut TextAtlas,
    area: Rect,
    text: &str,
    color: motion_ui::Color,
) -> f32 {
    let row = list.styles().metric("row.height", 22.);
    if area.width() <= 0. {
        return 0.;
    }
    let mut y = area.min[1];
    let mut line = String::new();
    for c in text.chars() {
        let mut candidate = line.clone();
        candidate.push(c);
        if c == '\n' || (!line.is_empty() && atlas.measure(&candidate) > area.width()) {
            ui::text(
                list,
                atlas,
                Rect::new(area.min[0], y, area.width(), row),
                &line,
                color,
                Align::Left,
            );
            y += row;
            line.clear();
        }
        if c != '\n' {
            line.push(c);
        }
    }
    if !line.is_empty() {
        ui::text(
            list,
            atlas,
            Rect::new(area.min[0], y, area.width(), row),
            &line,
            color,
            Align::Left,
        );
        y += row;
    }
    y - area.min[1]
}
impl Guide {
    pub fn draw(
        &mut self,
        list: &mut PaintList,
        atlas: &mut TextAtlas,
        area: Rect,
        input: &Input,
        zh: bool,
    ) -> Option<usize> {
        let sheet = list.styles();
        let row = sheet.metric("panel.header.height", 24.);
        let pad = sheet.metric("padding.panel", 8.);
        let width = area
            .width()
            .min(sheet.metric("plugin.author.guide.width_unit", 380.) * 2.);
        let x = area.min[0] + (area.width() - width) * 0.5;
        if let Some(tab) = ui::tab_strip(
            list,
            atlas,
            Rect::new(x, area.min[1], width, row),
            &[
                &tr(zh, "快速上手", "Quick start"),
                &tr(zh, "布局与交互", "Layout & interaction"),
                &tr(zh, "代码辅助与排错", "Code assistance & errors"),
            ],
            self.tab,
            input,
        ) {
            self.tab = tab;
            self.scroll = 0.;
        }
        let body = Rect::new(
            x,
            area.min[1] + row + pad,
            width,
            (area.height() - row * 2. - pad * 2.).max(0.),
        );
        if body.contains(input.mouse) {
            self.scroll = (self.scroll + input.wheel[1]).max(0.);
        }
        let sections:Vec<(&str,&str)>=match (self.tab,zh){
            (0,true)=>vec![
                ("从示例完成第一份插件","先打开 parameter-workbench.msplugin 示例。它已有可运行的 WGSL、公共参数与布局；另存为自己的 .msplugin 后再修改。作者工程保存源代码与布局，当前还不提供安装包发布。"),
                ("1  认识三个工作区","逻辑：搭节点或编写 WGSL，并定义公开参数。界面：决定参数面板如何排列。预览与诊断：操作示例参数并查看真实 GPU 结果。这三处共用同一份作者工程。"),
                ("2  设计面板","打开“界面 → 设计布局”，选择左侧“组件”。按住一个带缩略图的组件拖到画布，在插入标记处松开。容器示例里的子控件只用于说明，拖入的新容器为空。"),
                ("3  放入真正的参数","切换左侧“参数”，把强度、颜色或位置等拖进刚才的容器。参数类型决定控件；布局只保存绑定 ID。要增加参数，请到逻辑页的公开参数区域。"),
                ("4  调整与验证","在结构树中选择组件，通过右侧检查器调整间距、对齐和尺寸。拖动分栏边界调整比例；拖动选中框右下角调整大小。用“窄面板 / 宽面板”检查布局。"),
                ("5  操作与保存","切到“交互预览”试调控件，再到“预览与诊断”看效果。交互值是临时示例，不会改写参数默认值。Ctrl+S 保存；Ctrl+Shift+S 另存为。重开 .msplugin 可继续编辑。"),
            ],
            (0,false)=>vec![
                ("Your first plugin","Open parameter-workbench.msplugin, a runnable example with WGSL, parameters and a layout. Save as your own .msplugin before editing. Source projects preserve author data; installable package publishing is a later phase."),
                ("1  Three workspaces","Logic authors nodes or WGSL and public parameters. Interface designs the parameter panel. Preview & diagnostics operates sample values with real GPU output. All three share one source project."),
                ("2  Design a panel","In Interface / Design layout, open Components. Drag an illustrated component to the canvas and release at the insertion guide. New containers are empty; thumbnail children are examples only."),
                ("3  Bind actual parameters","Open Parameters and drag a parameter into the container. Its definition determines the control type. Add definitions in Logic; layouts store stable parameter IDs."),
                ("4  Adjust and check","Select a component to edit spacing, alignment and dimensions. Drag split dividers or the selected lower-right resize corner. Check both narrow and wide panel sizes."),
                ("5  Interact and save","Switch to Interact to try controls, then Preview & diagnostics for GPU output. Sample values do not change authored defaults. Ctrl+S saves; Ctrl+Shift+S saves a copy. Reopen the .msplugin to continue."),
            ],
            (1,true)=>vec![
                ("如何选容器","行：同一行横排；列：逐项竖排；分组：带标题并可折叠；标签页：在多个页面之间切换；分栏：两个可调比例区域。先用列组织内容，再按需要加入行、分组和标签页。"),
                ("效果预览与说明","效果预览会占用一个 GPU 视口槽位，缩略图中的棋盘格只表示槽位。说明显示静态提示文字，可在检查器中编辑。它们都不创建新的参数。"),
                ("选择、移动与取消","在结构树或画布选中后拖动，可跨组移动。拖动时看插入标记判断目标。分栏两侧和标签页根是固定容器，请向其内部放置内容。Esc、失焦或窗口尺寸变化取消当前拖动。"),
                ("撤销的范围","设计布局、节点、公共参数和已提交的源码共用作者历史。一次拖动是一条撤销。正在编辑文本时 Ctrl+Z 使用文本历史；交互预览的示例值另有历史，不会同时撤销作者工程。"),
                ("宽窄与滚动","控件保留可用的命中尺寸，内容放不下时滚动。窄窗口中左侧的结构和检查器通过页签切换。先检查窄面板是否可读，再检查宽面板是否被拉得过散。"),
            ],
            (1,false)=>vec![
                ("Choose a container","Row arranges horizontally; Column vertically; Group has a collapsible heading; Tabs switches pages; Split divides two resizable regions. Start with a column and add structure as needed."),
                ("Preview and note","Effect preview reserves a GPU viewport. Its thumbnail checkerboard denotes a slot, not rendered shader output. Note displays editable static instructions. Neither creates parameters."),
                ("Select, move, cancel","Drag from the structure tree or canvas across groups. The insertion guide indicates the destination. Split sides and tab roots are fixed containers. Escape, focus loss or window resize cancels a drag."),
                ("History ownership","Layout, nodes, parameters and committed source share author history. Each drag is one step. Active text has local undo. Sample interactions have separate history."),
                ("Narrow panels","Controls retain their hit targets and scroll on overflow. Narrow windows switch Structure and Inspector via tabs. Check narrow readability before expanding the panel."),
            ],
            (_,true)=>vec![
                ("WGSL 与节点来源","每份工程选择节点或 WGSL 作为权威来源。节点生成的 WGSL 是只读结果；不能把手写源码自动反向转换成节点图。"),
                ("开启语言服务","安装 wgsl-analyzer，将其可执行文件路径设为 MOTION_WGSL_ANALYZER 后启动编辑器。逻辑页显示连接状态。未配置时仍可编辑、保存和编译；本机语言服务不需要网络服务或 API 密钥。"),
                ("补全、提示与跳转","在 WGSL 源码中按 Ctrl+Space 请求补全，使用列表选择插入；符号说明（需服务器支持）和定义定位使用代码区工具。宿主声明和公共参数访问函数与源码一起提供给语言服务。语言分析结果不替代最终 Naga / GPU 校验。"),
                ("编译错误怎么办","编辑暂停后自动检查，也可点“检查预览”。错误显示阶段和可用的源码行列，点“定位源码”返回对应位置。编译失败保留上一份有效画面；画面仍在不代表当前草稿已通过。无效源码也能保存。"),
                ("保存、关闭与限制","未保存时文件名带 *。关闭会询问保存、放弃或继续编辑。保存失败保留草稿和错误。当前为独立示例环境，未连接主工程；宿主实例调试、资源导入和安装包发布尚未接入。"),
            ],
            (_,false)=>vec![
                ("WGSL or nodes","Each project has one authoritative source. Generated WGSL is read only; arbitrary WGSL cannot be converted back to a node graph."),
                ("Enable language assistance","Install wgsl-analyzer and set MOTION_WGSL_ANALYZER to its executable before launch. Logic shows connection status. Without a server, editing, saving and compilation remain available. No online account or API key is needed."),
                ("Completion and navigation","Use Ctrl+Space for completion and select a result to insert. Code tools request symbol information (server support required) and definition navigation. Host declarations and public parameter functions are supplied to the server. Final Naga / GPU validation remains authoritative."),
                ("Compilation errors","Automatic checks run after an editing pause; Check preview runs explicitly. Errors show their phase and source location when available. Locate source returns to the error. Failed checks retain the previous good image. Invalid drafts can still be saved."),
                ("Saving and limitations","A * marks unsaved changes. Closing offers save, discard or keep editing. Failed saves preserve the draft and cause. This standalone sample is not connected to a host project; resource import and package publishing are not yet connected."),
            ],
        };
        let mut draw = list.child();
        let mut y = body.min[1] - self.scroll;
        for (heading, text) in sections {
            ui::text(
                &mut draw,
                atlas,
                Rect::new(x + pad, y, (width - pad * 2.).max(0.), row),
                heading,
                sheet.color("text"),
                Align::Left,
            );
            y += row + pad;
            y += wrapped(
                &mut draw,
                atlas,
                Rect::new(x + pad, y, (width - pad * 2.).max(0.), body.height()),
                text,
                sheet.color("muted"),
            );
            y += sheet.metric("plugin.author.guide.section_gap", 24.);
        }
        let extent = y - body.min[1] + self.scroll;
        self.scroll = self.scroll.min((extent - body.height()).max(0.));
        draw.clip_to(body);
        list.append(draw);
        let mut target = None;
        for (i, page) in [1, 0, 2].into_iter().enumerate() {
            let w = sheet.metric("plugin.author.action.width", 80.);
            let b = Rect::new(x + i as f32 * (w + pad), area.max[1] - row, w, row);
            if ui::button(
                list,
                atlas,
                b,
                &[
                    tr(zh, "编辑界面", "Interface"),
                    tr(zh, "编辑逻辑", "Logic"),
                    tr(zh, "运行预览", "Preview"),
                ][i],
                input,
                true,
            )
            .clicked
            {
                target = Some(page);
            }
        }
        target
    }
}
