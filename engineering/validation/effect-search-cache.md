# 效果搜索目录与查询缓存（#134 子项）

## 范围和协调

基于 main `cc4467b6`，分支 `codex/effect-search-cache`。开始前重新核对开放 PR #425/#426/#428 和所有工作树的 effect_search/types 改动。没有其他 effect_search 缓存改动；旧的 workspace-switch-observability 在 Controls 的效果库缓存及 Panels 计时，其他 types 改动属于快捷键/多选，不涉及 EffectSearch 字段。本批不修改其实现。#428 的节点索引在独立工作树和 PR，根工作树的效果包/网站等未提交内容保留。

原实现每次 render_effect_search 都调用库目录投影、每项检索文本构建、排序和所有候选的 JSON 应用命令生成。现在开放的 EffectSearch 持有一个目录投影和一份匹配索引：

- 目录 Arc 身份或语言变化才重新投影和排序，并重新生成检索文本。
- 查询变化才 trim/lowercase、筛选；静态查询直接借用匹配结果。
- 应用时使用当前可编辑图层生成命令；不会把旧目标对象缓存在结果中。

目录 Arc 在启动/导入目录完成时发布，普通帧和播放头移动不会重建；证据在 main.rs 和 effect_import.rs 的目录赋值/refresh 调用。缓存保留一个强引用，安全的 Arc::make_mut 会发布新身份；不以某个包 hash 推断整份目录不变。新目录/语言替换旧投影，关闭搜索释放缓存，没有目录/查询历史集合或全局缓存。内存与单次开放搜索的目录/匹配项及最长查询容量相关，不随帧数累计。

没有新 UI 行、尺寸、颜色、文案、命令、工程格式、撤销结构、线程、依赖或第三方资产。Entry 的内部重导出供搜索复用元数据；现有 EffectSearch 构造只补充默认缓存。既有断言保留，目录夹具使用 Arc，旧 command 字段断言改为相同 JSON 的 command(42) 调用。

## 测量与语义验证

独立 integration test 直接包含生产 cache.rs，仅该测试二进制安装计数 allocator，正常应用分配器不变。预热 200 项，连续 120 次相同目录、语言和查询：实际缓存命中分配为 **0**，并检查 loader 不被调用、完整匹配数量和校验和。分配 canary 验证计数器能检测真实分配。

受控对照在生产缓存的命中判断前临时增加一次 query.to_lowercase，实际测试以 **120** 次分配失败，之后逐字节恢复原文件；保留失败输出和恢复 SHA 收据。这证明该断言能检测重复查询分配，不冒充原算法的完整每帧分配数字。日志 `.tools/effect-search-cache/negative-allocations.log`、`negative-receipt.json`；正常/最终结果另列。计数窗口不包括初次目录投影、真实 UI 绘制或应用命令生成，不能声称整个搜索帧零分配或 FPS 提升。

独立语义测试覆盖 Arc clone 命中、查询变化复用目录、空/无匹配查询、大小写/空白、多词、Arc::make_mut 更新及旧快照释放、语言更新。真实搜索测试对照原库 Entry::matches 和排序，覆盖两个版本、启用变化、相同包 hash 的名称变化、中英文与目录别名；实际 render 验证预热后应用对象依次为 42/99/无选择/123，连续模式不陈旧。

最终定向验证 **23 通过、0 失败、0 忽略**：恢复后的独立缓存 2，搜索/跨窗口/真实应用与默认失败信道 13，最大化输入优先级 5，退役效果及旧工程错误信道 1，真实添加失败恢复和陈旧快照各 1。首次 cfg(test) 编译漏补一个旧夹具的缓存默认值，保留编译失败；补齐默认构造后按原断言完整重跑，不放宽既有断言或期限。正常应用 cargo check 成功，只有 19 项既有警告。

真实原生验收的中英主窗/浮窗、四种尺寸及 1/1.5 渲染缩放共 16 张截图，PNG 签名、尺寸与收据均通过；人工抽查英文浮窗和中文窄窗，查询、候选和连续模式显示正常。这里是渲染缩放验证，不声称 OS DPI 已被切换。未知效果的真实失败在默认 Info 文件及 Error stderr 各两次，stdout 隔离检查通过；工程与原始错误保持，后续正常应用恢复。

日志在 `.tools/effect-search-cache/final-*.log`、`check.log` 和 `compile-before-cache-default.log`，图像/收据/默认文件和 stderr/stdout 在 `artifacts/effect-search-cache/`。源码缓存 SHA 与受控对照恢复收据再次核对一致。来源检查覆盖 1808 tracked paths，样式消费者、相关格式与差异检查通过。完整远程 CI 仍待本 PR 运行；不把这些定向结果扩大成全桌面/全部平台通过。

## 错误与观测审计

| ID | 分类 | 处理和证据 |
|---|---|---|
| ESC-MISS | 正常分支 | 首次打开、新目录/语言/查询、空匹配是正常失效，不增加日志、重试或故障计数。 |
| ESC-CATALOGUE | 既有边界 | 沿用 effect_controls::entries 的包版本/启用/兼容性过滤；没有隐藏 Err 或 panic。目录投影仍为原来的同步纯数据路径。 |
| ESC-TARGET | 功能边界 | JSON 在实际应用时生成当前 object；可编辑选择仍由原 Panels/shell 路径提供，保留锁定/只读和原子应用的宿主边界。 |
| ESC-FAILURE | 既有观测复验 | 不改搜索动作之后的宿主失败/恢复、默认文件日志、Error stderr、stdout 隔离。真实未知效果、退役旧工程、添加失败/恢复和陈旧 UI 快照测试均通过。 |
| ESC-PRESENT | 正常热路径 | 没有新健康帧日志、文件 IO、worker、start/done、timeout 或 restart；现有观察协议保持。 |

## DESIGN §0.7 与 §12

已检查 PROVENANCE.md、ATTRIBUTION.md、NOTICE 和 DESIGN.md；主线源码一致，不添加资产或兼容名称。没有把数据缓存描述成视觉整改。

| §0.7 基线行 | 本批处置 |
|---|---|
| path_controls 六种步距 | retained with reason：没有路径面板修改。 |
| path_controls 两列硬编码 | retained with reason：没有路径布局修改。 |
| path_controls 按钮/字段高度 | retained with reason：没有路径控件或命中区修改。 |
| path_controls 自绘数值底色 | retained with reason：没有数值字段修改。 |
| path_controls 布尔宽按钮 | retained with reason：没有布尔控件修改。 |
| effect_controls 行高/步距 | retained with reason：只重导出元数据类型，没有效果行修改。 |
| effect_controls 布尔按钮 | retained with reason：没有效果控件修改。 |
| effect_controls 枚举宽按钮 | retained with reason：没有枚举控件修改。 |
| effect_controls hover 底色 | retained with reason：没有绘制状态修改。 |
| effect_controls 缩进/层级 | retained with reason：没有属性树修改。 |
| timeline 标签栏坐标 | retained with reason：保留兼容基线，没有时间轴修改。 |
| timeline snap 指示线 | retained with reason：保留既有表现，没有主题修改。 |

搜索的既有 token、命中和窗口锚点不变。截图、原生输入和相邻回归另据实际结果记录；不声明全部 19 面板、所有平台/DPI 或全部视觉基线已通过。#134 的其他性能工作继续开放。
