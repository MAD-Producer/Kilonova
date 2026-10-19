# 预览版缺字和合成视口错误

证据目录：`E:/Dev/aem/artifacts/property-selection-context/`。本记录对应用户在 `motion-studio.exe --locale zh` 中提供的 U+2194、U+2713、无效视口和 FFmpeg 输出。

## 功能修复与既有观测

| 编号 | 处置和证据 | 限制 |
| --- | --- | --- |
| LOG-01 | 功能修复：时间轴适应宽度按钮改用现有 `Icon::Expand`；视图/诊断菜单和图层遮罩菜单的选中标记改用现有 `Icon::Check`。勾选状态从实际动作/模型获得，保持点击、禁用、键盘路由和命中区域。菜单标记使用 `icon.slot` / `checkbox.size`，不再把 UI 符号送入文本 atlas。 | 未修改字体加载、字体回退和缺字记录；其他用户文本缺字仍由原有观测报告。 |
| LOG-02 | 功能修复：`styled_viewer` 在没有画布空间时返回零尺寸；浮动合成窗口不提交空画布。原实现强行构造 1px 画布，小尺寸主窗/浮窗可能越界。`collapsed_viewers_never_submit_a_fabricated_pixel_outside_the_window` 覆盖 7×10 组窗口尺寸、单合成/默认布局、浮窗及恢复到普通尺寸。 | 确认修复这一可复现路径；没有足够上下文证明它解释用户的每一次错误。没有对任意非法视口裁切或容错成功。 |
| LOG-03 | 新观测上下文：保留主机 `invalid composition viewport` 返回错误，补充原始 viewport、surface 宽高、render/retained 阶段；桌面 Present 的错误补充窗口 ID 和逻辑尺寸。沿用原有请求 ID、错误日志和回复信道，没有新增健康 Present 日志。 | retained 分支新增格式由编译覆盖，未单独制造该分支的失败。 |
| LOG-04 | 实际失败路径验证：已有阻塞解码器/原生 GPU 回归中，提交负坐标、零宽、越界、NaN 视口，要求在 5 秒内返回错误及全部上下文；项目/版本保持原值。父进程在默认 Info 等级检查真实文件日志和 Error stderr，并验证错误不进入 stdout。正常后续 Present、受控阻塞和失败保留原有断言。 | 未改变现有日志阈值、恢复计数、重试策略或错误返回。 |
| LOG-05 | 既有覆盖/正常分支：FFmpeg n8.1.3 `libavformat/mov.c` 在 UDTA 字符串长度超过 atom 时发出 Warning 并走 raw 元数据解析回退；`movenc.c` 的 moov 移动、`aacenc.c` 的 Qavg 是 Info。项目的 `+faststart` 明确请求 moov 整理。`video_frames.rs` 已在实际打开解码器前写入并刷新 Info，包含 object/source/sequence/PTS/path，并在共享状态中发布 open/decode 阶段。图层离开有效范围后退休实例是现有正常分支。 | UDTA 重复出现是否对应不必要的重复解码器打开仍待真实媒体和请求时间关联，不记录为性能缺陷已修复。未屏蔽第三方输出，未添加每帧文件 IO。 |
| LOG-06 | 待办：OpenH264 profile 与关闭跳帧的码率控制警告属于编码参数兼容问题。本轮未更改编码器选择、profile、码率控制或跳帧行为。 | 本地构建报告 `video_encoder=false`，不能用解码器构建声称验证用户预览版导出配置。 |
| LOG-07 | 原生截图：`release/native-log-fixes-final/` 中双语、默认/覆盖样式检查通过；目视检查中文宽/窄遮罩子菜单、视图列菜单和诊断菜单，现有 Check 图标位置及禁用状态正确。 | 只验证所列 UI 符号，不声称字体回退整体完成。 |

`preview-log-full-tests.log`：502 passed、0 failed、1 项原有忽略，154.09 秒。包括 LOG-02 几何回归和 LOG-04 原生 GPU/错误信道回归。后续截图场景配置尚需优化构建及实际原生验收。

工具验证：`preview-log-tools-tests.log` 保留了从仓库根目录按包导入工具测试导致的 3 个导入错误（工具依赖同目录模块）；从 `tools` 目录按现有模块布局重跑，`preview-log-tools-tests-corrected.log` 为 38 项、1 项原有跳过，24.383 秒通过。样式消费者及差异空白检查通过。没有通过修改工具测试断言处理导入失败。

首轮原生截图失败保存在 `typed-native-acceptance.log`：新增形状夹具把 `sampledLayers` 留空，Properties 按已有规则拒绝缺少采样数据的目标，因此没有可点击属性。修正夹具为从已验证的常量轨道采样，不更改产品的缺失采样处理、原有断言或验收超时。

第二轮 `preview-log-native-acceptance.log` 保留音频夹具继承纯色层空间属性导致验证失败。改为音频层合法的零尺寸及默认 Transform；新增真实绘制/点击夹具测试，`native-fixture-tests.log` 为 1 项通过。没有修改产品断言或验收时限。

最终 `preview-log-release-build-final.log` 优化构建通过（8m46s，21 项既有警告）；`preview-log-native-acceptance-final.log` 完整原生验收通过，包括双语、样式覆盖、浮窗、节点和共享 MCP。截图和报告均在 `release/native-log-fixes-final/`，相应 stderr 未出现 U+2194/U+2713 缺字错误。此处验证的是本次 checkpoint，后续路径交互改动需另外验证。

## DESIGN §0.7 处置

| 基线项 | 处置 |
| --- | --- |
| Path 六处纵向步长 | retained：未修改。 |
| Path 两列分割 | retained：未修改。 |
| Path 混合按钮高度 | retained：未修改命中容差。 |
| Path 数值背景 | retained：未修改。 |
| Path 全宽布尔按钮 | retained：未修改。 |
| Effect 行高/步进 | retained：未修改。 |
| Effect 布尔按钮 | retained：未修改。 |
| Effect 枚举按钮 | retained：未修改。 |
| Effect hover alpha | retained：未修改。 |
| Effect 层级 | retained：未修改。 |
| Timeline 固定列坐标 | retained：保留原工具条位置和共享列规划。 |
| Timeline 吸附线颜色 | retained：未修改。 |

没有新增依赖、第三方资产、字体或 token；复用现有独立图标。UI 变化是符号绘制与空画布处理，日志变化只增加错误上下文，二者分别记录。编译通过不能代替截图和实际交互验收。
