# 项目与媒体面板共享素材操作验收

关联 #222，接续 [底层事务记录](media-asset-actions.md)。底层源码 `782e60a6`，界面源码 `75a989b9`，同步 main 后集成提交 `9913bc2f`。#506 已在五项必需 CI 通过后合入，合并提交 `a9a26d5c`。本记录只覆盖本人负责的素材操作，没有审阅其他人的 PR。

## 实际功能

- Project/Media 共用素材选择和操作。孤立素材不再依赖已有图层才能选中；选择素材不改动时间轴图层选择或工程历史。素材名优先使用引用图层名，孤立素材显示文件名；搜索同时匹配显示名和路径。
- 同一右键菜单提供重链接/替换、文件位置、使用处和从工程中移除。引用检查来自共享模型，覆盖全部合成、隐藏图层、视频音轨和效果图像输入。仍被引用的素材禁止移除；关联音轨提示操作所属视频。
- 移除只删登记，不删文件，保留撤销。使用处列表打开实际合成并选中图层，复用时间轴的自动定位。第一轮真实点击发现漏传合成 API 的 `version=1`，已修复调用，没有放宽 API 校验或测试断言。
- 替换复用现有异步导入任务和单步事务，保留身份、图层及引用。图片/音频失败释放操作槽；取消、过期返回正常反馈，不报告成功。项目 root、document generation 和 revision 共同限制菜单命令，防止复用旧素材 ID。
- 菜单绑定打开它的原生窗口。其他窗口的中性重绘不关闭它，也不在其他窗口绘制；其他窗口有输入时关闭菜单并保留关闭帧的输入所有权。Escape、输入法确认和重复 Enter 不穿透。
- Windows 的文件位置使用 `ShellExecuteW` 打开所在目录并检查 shell 返回值，未将返回值误作进程句柄。缺失源定位到最近的现存父目录；目录访问错误保留原因。macOS/Linux 使用系统工具，并有界保留一个 launcher，轮询退出状态；未添加重试、自动重启或超时。

没有增加依赖、第三方素材、工程格式或渲染语义。原生截图中的项目与 PNG 均为本测试独立生成，不包含第三方产品内容。新增尺寸先写入 `dark.json` 的 `project.composition.height`，保留既有 32px 合成行；菜单沿用既有 context 颜色、行高和 popup token。

## 证据与可观测性

| ID | 分类 | 证据 / 限制 |
|---|---|---|
| U01 | 修改既有界面 | 复用 `ListView` 的行、选择、滚动和裁剪；Project/Media 共用同一个 Controls。模型及显示名缓存按文档/版本替换，不在健康重绘中访问文件、调用引擎或解析工程。 |
| U02 | 新增协议边界 | 正常过期/取消显式反馈；图片回复与媒体任务 commit 检查取消标记，不把取消显示成成功。真实 PNG/WAV 的 Shell 测试覆盖失败、取消、复用操作槽和保留原工程。 |
| U03 | 既有观察 | 替换/媒体/删除/合成请求继续使用引擎的 Info start、in-flight、原始错误和 Error stderr。复用原 panic hook 和 worker，不建立解码线程或绕开串行事务。 |
| U04 | 新增观察 | 资产动作失败记录 kind/ID/revision/耗时和原因；文件位置在文件系统访问前记录 Info start，失败保留阶段和原因。独立子进程清除 `MOTION_LOG`，以真实目录错误验证文件日志、Error stderr 和 stdout 隔离。 |
| U05 | 新增观察 | 无法解析的工程快照显示错误，并按单个缓存版本记录一次 Error，不按每帧重复记录。未引入观察键集合、循环重试或 Present start/done 日志。 |
| U06 | 功能边界 | Windows shell 返回值是“请求已接受”，不证明外部资源管理器最后呈现成功；Unix 工具退出状态会被轮询，stderr 不转发到协议 stdout。没有宣称实际 OS 前台文件选择器/资源管理器完成了自动验收。 |

原始运行文件保留在 `artifacts/pr-ci-followups/media-assets-*`。`media-assets-ui-native-01.log` 是合成 API 版本漏传的真实失败，保留其 stderr 与首轮截图；`-02` 修正后通过。证据打包后的源码哈希、日志清单和最终套件计数以结果 JSON 为准。

同步 main 后的最终回归为：面板 559 项通过、2 项人工专项未默认执行；Shell 素材操作 3 项通过；原生 GPU 专项显式执行 1 项通过并生成 16 张 PNG。原生专项覆盖中英、900×600 / 1600×1000、1 / 1.5 / 2 比例、真实 Media 浮窗 `present_float`、使用处点击、引用删除禁止、孤立素材移除/撤销/重做、图片替换、过期取消及保存重开。使用真实隐藏窗口、GPU 与受控 `Input`，未注入 OS 前台键盘或自动化原生文件选择器。

`cargo build -p motion-desktop --features ffmpeg` 成功；`--command-list` 输出可解析 JSON 并包含 `media_asset`，stderr 为 0 字节。静态主题消费检查与 `git diff --check` 通过。构建中的既有 warning 未作为零 warning 验收。

[结果与源码哈希](media-asset-actions-ui-results.json)、[原始日志和全部截图](media-asset-actions-ui-evidence.zip)、[项目菜单](media-asset-actions-ui/zh-project.png)、[使用处](media-asset-actions-ui/zh-usages.png)、[媒体浮窗菜单](media-asset-actions-ui/zh-media-float.png)。预览程序使用源码 `9913bc2f`，SHA-256 为 `1fe8e635e808f1c8ad5c7ae3a5d7aa383a65c4041c931f93f026f468da24abd8`；FFmpeg DLL 和许可沿用已验证运行时，保留其来源记录。

独立预览位于 `artifacts/media-asset-actions-preview`。仅保留自身目录与 Windows 系统目录的 PATH 后，`--command-list` JSON 通过且 stderr 为零；`--video-smoke` 对独立生成的 `sound-24fps.mp4` 实际解码，报告 `backend=libav`、`decoded=true`、72 源帧，并返回首帧、中间帧、尾帧和再次首帧的 RGBA 数据。视频专项 exit 0、stderr 为零。原始报告及运行时哈希已补入证据包；这证明打包后的 DLL 可加载并解码视频，不替代原生文件对话框和全工作区验收。

## 未完成边界

多份素材同时丢失时，现有全工程资源预检可能阻止逐份重链接；本轮未改变存储校验和 renderer 的完整目录预检。需独立完成分批恢复方案及附带渲染验证，不能通过略过失败或吞掉资源错误解决。原生文件选择器取消、Windows 资源管理器最后呈现、macOS/Linux 文件位置与实际 OS 键盘/IME 交互还需验收。字体仍使用既有字符串身份，未接入数字媒体操作。

因此 #222 继续开放。此阶段不宣称完成 19 面板、五布局及 §12 全部人工验收。

## DESIGN §0.7 逐项处置

12 项均 **retained with reason**：Path 六处行高、两列尺寸、按钮高度、自绘背景、布尔按钮（1–5）及 Effect 行高、布尔 fx、枚举按钮、hover alpha、缩进（6–10）不属于素材面板改动；Timeline 列坐标和 snap guide（11–12）保留既有兼容基线。没有把共享素材菜单误写为这些既有整改项的修复。

§12 的本轮范围为素材列表/菜单的中英、默认/最小窗口、三种比例、裁剪、禁用、窗口所有权、关闭帧和撤销。其他面板/布局、真实 OS 前台操作及跨平台文件位置是待验证项。静态主题消费检查通过，编译成功不替代这些实际交互证据。
