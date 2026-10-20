# #251 偏好设置与分类侧边栏验收

## 交付范围

在编辑菜单和 Ctrl+, 接入偏好设置。常规分类包含界面语言、实际引擎预览质量；界面与编辑分类包含节点网格/小地图、时间轴混合/父级/遮罩列，以及已有样式覆盖文件。布尔值使用原生 Checkbox，枚举使用 Dropdown；没有未接线的占位选项。侧边栏使用已有 row 选中态、border 分隔线；窗口变窄时改为顶部分类，正文滚动，底部操作固定。所有新增尺寸先定义在 dark.json。

草稿切换分类保留；取消与 Escape 不提交。恢复默认只改变草稿，应用才生效。语言、样式、列和节点显示配置应用到主窗及原生浮窗；设置独立于工程、撤销历史和布局保存。快捷键改键由后续 #277 堆叠实现，本次仅提供设置入口。

保存先验证样式和字体，再向同目录独占临时文件写入、同步、关闭、重命名；失败保留原设置并清理本次临时文件。实际预览设置先成功后写盘；写盘失败显式恢复原预览，若恢复失败分别保留主错误和恢复错误。不承诺目录同步或掉电事务。此原子保存也覆盖已有工作区保存调用，属于独立功能修复。

## 故障与正常边界

| 编号 | 类型 | 行为和证据 |
| --- | --- | --- |
| PREF-01 | 新增观测 | 应用在样式/字体/引擎/写盘前写 Info start 并 flush；Debug done；失败 Error 带 operation、phase、耗时、路径和原始原因（日志上下文长度有界）。真实坏样式和目标为目录的发布失败同时出现在文件与 stderr；stdout 无错误日志。 |
| PREF-02 | 功能修复 | 真实发布失败、Windows 文件拒绝替换保留旧数据；临时文件清理；新的有效请求可以成功恢复；预览恢复经过实际引擎调用验证。 |
| PREF-03 | 正常分支 | 取消、未应用草稿、关闭枚举、IME 消费、焦点丢失均非错误；不增加日志或写工程。 |
| PREF-04 | 现有覆盖 | Workspace 读取沿用缺失/非法输入回退及有界 Counter；正常缺失重置，连续异常首次 Debug、60 次 Error。样式时间戳监测沿用已有计数，不重复改造。 |
| PREF-05 | 新增有界退化 | popup 样式意外缺失沿用 Counter：首次 Debug、60 次 Error，成功解析后重置；不增加正常帧文件 IO。 |
| PREF-06 | 功能边界 | 原生文件选择器在阻塞前 Info start，完成 Debug；用户取消保持草稿。没有新增线程超时、重试、重启或协议吞错。现有工作线程 start/in-flight 和通道错误策略保留。 |

## 验证结果

- `cargo test --locked -p motion-desktop preferences -- --test-threads=1`：15 通过（含实际工作线程、真实文件失败和默认日志输出对照）。随后增加的主窗/浮窗快捷键路由测试在下述完整回归通过。
- 与 #351 `096a946` 测试隔离改动本地组合运行完整桌面回归：600 通过、1 忽略、1 失败；唯一失败为新增 Ctrl+, 未登记默认键位文档。使用原有 `MOTION_UPDATE_SHORTCUT_DOCS=1` 从真实 resolver 生成文档，未修改断言，再运行该测试通过。未宣称完整套件二次全跑；1 忽略为现有音频设备测试。
- 带 FFmpeg 原生构建成功。GPU 真实渲染中英文各六张偏好截图：常规、质量下拉、界面分类、窄窗、窄窗滚动底部、真实错误；既有属性面板截图检查也通过。目录：`artifacts/editor-preferences/sidebar-final/`。
- 生产程序实际重启读取中文和 Balanced 质量；从真实中文属性浮窗按 Ctrl+,，OS 焦点转到主窗并显示设置，Escape 后工程/revision/undo/redo/质量不变，正常退出。证据：`artifacts/editor-preferences/native-restart-focused/summary.json` 和 `preferences-from-native-float.png`。外部驱动先完成 MCP 初始化，并遵循 Windows 前台焦点限制；此前驱动两处协议/窗口等待错误与一次前台激活限制均保留日志，不计产品验收失败。
- 原生 stderr 仍有本机 Epic Overlay 缺失 JSON 的 Vulkan Loader 既有报错；未屏蔽。设置应用测试的 Error 文件/stderr/stdout 分别保存，不混同正常原生启动的日志。
- provenance 与样式消费者检查、`git diff --check` 通过。没有新增依赖、第三方资源或兼容名称。实际桌面检查为 DPI 1；其他 DPI 和各 OS 人工验收待补，不把编译称作完整 DESIGN 验收。
- 2026-10-20 集成主干 `b31b828f`（包括 #344、#351、#356 等已合并改动）：唯一文本冲突为 Shell 初始化，保留偏好预览恢复和采样列可见性初始化。合并结果运行完整 `cargo test -p motion-desktop --locked -- --test-threads=1`，619 通过、0 失败、1 个既有音频设备测试忽略，436.27 秒。证据：`artifacts/editor-preferences/main-integration-tests.log`。原始提交另有精确 head `4856ffcc` 的远程运行 `39287179639`，六项跨平台检查全部成功；这不替代集成后提交的 CI。

## DESIGN §0.7 逐项处置

本次仅改菜单入口及新的偏好弹层，不改属性面板内部。以下是基线处置，不表示旧缺陷已修复；行号依照 DESIGN 基线，当前主干可能已前移。

| 基线项 | 处置 |
| --- | --- |
| path_controls 六处行推进 | retained with reason：非偏好设置行布局范围。 |
| path_controls 双列分配 | retained with reason：保留现有字段几何。 |
| path_controls 混合按钮高度 | retained with reason：不改变现有命中区。 |
| path_controls 自绘数值背景 | retained with reason：不改路径数值控件。 |
| path_controls 全宽布尔按钮 | retained with reason：不改路径面板。新设置使用原生 Checkbox。 |
| effect_controls 行高和推进 | retained with reason：不改效果属性行。 |
| effect_controls 布尔文本按钮 | retained with reason：不改效果控件。 |
| effect_controls 全宽枚举 | retained with reason：不改效果控件。新设置使用 Dropdown。 |
| effect_controls with_alpha 悬停 | retained with reason：不改该绘制路径。 |
| effect_controls 树缩进 | retained with reason：不重写效果树。 |
| timeline 列坐标兼容基线 | retained with reason：仅切换现有列可见性，不修改坐标。 |
| timeline 吸附线颜色 | retained with reason：本次不改时间轴反馈。 |

DESIGN §12：中英文、选中/悬停、下拉输入所有权、取消/IME、窄窗裁剪滚动、错误保留草稿、主窗/浮窗路由已有测试和实际截图；未执行高 DPI 人工检查，未改右键菜单，不宣称属性树旧问题已经修复。
