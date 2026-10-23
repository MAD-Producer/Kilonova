# 效果搜索的输入法备选快捷键（#158 子项）

## 范围与协调

分支 `codex/effect-search-ime-shortcut`，实现基于 main `cc4467b6`。开始前检查所有工作树和开放 PR #425/#426/#428/#429：没有 Ctrl+Alt+Space 的搜索实现；其他快捷键分支涉及偏好或输入绑定，#429 涉及搜索缓存。独立提交只实现 #158 的备选键子项，合集仍保持开放。根工作树的未提交文件保持原样。

Ctrl+Space 与 Ctrl+Alt+Space 共用结构化搜索处理函数，均可打开或关闭搜索。沿用精确修饰键匹配、自动重复和输入法组合/已消费事件抑制；编辑、菜单和模态优先级保持。关闭帧被消费，不能继续触发 Delete 等工程操作。两个组合均保留为结构快捷键，不作为可重绑定的 Command；现有 resolve 返回 None 的断言继续守护命令层边界，新增断言验证实际搜索路由。中英文快捷键参考更新为两个入口。

没有引入样式、尺寸、颜色、控件、依赖、第三方资产、工程格式或 worker 变化。已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md。

## 验证与证据

最终四组定向测试共 **28 通过、0 失败、0 忽略**：keyboard 15、shortcuts 8、native_input 1、effect_application 4。正常应用 cargo check 成功，19 项既有警告。日志位于 `.tools/effect-search-ime-shortcut/final-*.log` 和 `check.log`。

- 新矩阵检查 128 种按键/修饰键/重复/输入法状态，仅允许精确的两个组合。
- 实际键盘路由检查中英文、主窗/浮窗和节点上下文、两个开启键与两个关闭键，共 32 种组合；关闭帧叠加 Delete，工程动作为空、节点内容与修订保持。
- 重复按键、输入法组合、输入法消费、文本编辑和菜单分别阻止打开；已打开时重复/输入法事件不会意外关闭。备选键不能被其他命令绑定。
- 原生窗口测试通过生产 chrome/floating_chrome 路由备选键，实际捕获中英文主窗 900×600、浮窗 400×300 四张新增截图；关闭帧和工程保持断言通过。原有四种尺寸及 1/1.5 渲染缩放的 16 张截图和断言全部保留，共 20 张。逐张 PNG 签名/尺寸与收据检查通过，人工查看英文主窗和中文浮窗。
- 默认 Info 文件与 Error stderr 中真实未知效果失败各两次，原始原因、操作 start、stdout 隔离检查保持通过；没有修改既有错误断言或期限。

图像、收据和默认日志信道证据在 `artifacts/effect-search-ime-shortcut/`。原生测试通过已归一化输入进入实际窗口绘制；不代表物理键盘到 Windows TSF/第三方输入法的全链路已经实测。操作系统仍须将备选事件交给应用。渲染缩放不等同于切换 OS DPI。远程 CI 待本 PR 运行。

## 错误与观测审计

| ID | 分类 | 处置与证据 |
|---|---|---|
| ESI-INPUT | 正常分支 | 不匹配、重复、组合态、已消费、编辑和菜单是正常抑制，不新增错误或计数。矩阵与路由测试覆盖。 |
| ESI-MODAL | 功能边界 | 两个键共用既有打开/关闭路径，关闭帧消费防止工程动作泄漏。主窗、浮窗、节点与原生窗口证据如上。 |
| ESI-COMMAND | 协议边界 | 保留结构快捷键与 Command resolver/绑定边界；无新宿主请求、阻塞操作、线程、重试或重启。 |
| ESI-FAILURE | 既有观测复验 | 真实应用/未知效果失败与恢复、默认文件/Error stderr、stdout 隔离的原断言通过；没有新增观测项。 |
| ESI-OS | pending verification | 物理输入法/TSF 分发需要目标系统交互验收；当前证据只证明应用收到事件后的处理。 |

## DESIGN §0.7 与 §12

| 基线行 | 处置 |
|---|---|
| path_controls 六种步距 | retained with reason：未修改路径面板。 |
| path_controls 两列硬编码 | retained with reason：未修改布局。 |
| path_controls 按钮/字段高度 | retained with reason：未修改控件或命中区。 |
| path_controls 自绘数值底色 | retained with reason：未修改数值字段。 |
| path_controls 布尔宽按钮 | retained with reason：未修改布尔控件。 |
| effect_controls 行高/步距 | retained with reason：未修改效果行。 |
| effect_controls 布尔按钮 | retained with reason：未修改效果控件。 |
| effect_controls 枚举宽按钮 | retained with reason：未修改枚举控件。 |
| effect_controls hover 底色 | retained with reason：未修改绘制状态。 |
| effect_controls 缩进/层级 | retained with reason：未修改属性树。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，未修改时间轴。 |
| timeline snap 指示线 | retained with reason：未修改主题或拖动反馈。 |

本批键盘路由与实际原生截图有相应证据；没有把编译、定向测试或上述处置扩大成全部面板/平台/视觉基线验收完成。
