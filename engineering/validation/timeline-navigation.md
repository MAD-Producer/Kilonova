# 时间轴视区导航修复

## 证据与功能边界

用户录屏 `Desktop 2026.10.18 - 14.41.11.01.mp4`（13.85 秒）展示缩放、平移及图层边缘拖动；用户确认问题是缩放/滚动后刻度与图层显示异常，以及向左拖动时视区不跟随。逐秒和时间轴局部帧存于 `artifacts/timeline-video-review/`。不把录屏中的每次截短都推断为模型错误。

| ID | 分类 | 改动 / 验证 |
|---|---|---|
| NAV-01 | 功能修复 | 缩放控件先更新 zoom/offset，再为本帧统一计算刻度、图层与寻帧坐标。真实 PaintList 回归检查点击放大当帧图层条端点。 |
| NAV-02 | 功能修复 | 图层完全离开视区时不画零宽条和边缘手柄；端点必须在时间区内才响应裁剪。实际面板回归点击原本落进左侧属性区的出点，确认没有裁剪事务。保留已有 8 像素命中宽度。 |
| NAV-03 | 新交互 | 时间区底部横向滚动条；沿用 zoom/offset，不增加工程状态。支持抓取滑块、点击槽定位及取消；移位面板回归确认不发 Seek/Edit/History，不改选择。 |
| NAV-04 | 新交互 | 图层移动、裁剪及时间尺拖动到横向边缘时按耗时滚动，最长步长 50ms；移动图层补偿抓取原点以保留时间锚。松开、回到中部、失焦及取消结束。保持现有图形关键帧/手柄的相对坐标协议，本批未扩展它们的边缘滚动。 |
| NAV-05 | 回归兼容 | 底栏占位导致既有曲线下时间尺回归失败，保留原有断言并调整布局：下时间尺不移位，模式文字移至左下角。没有放宽或改写原测试。 |
| NAV-06 | 回执兼容 | 回执重绘会保留已消费的鼠标状态；主窗及浮窗用 navigation_paint_only 禁止回执推进导航，防止连续生成编辑而饿死待处理的释放。空输入重画也不移动或释放滑块。真实 Shell/Engine 回归连续重画 8 次验证 offset 不变、没有重复动作、释放仍排队且最终关闭移动事务。 |
| NAV-07 | 目视发现 / 功能修复 | 首次完整原生检查通过后，目视截图仍发现播放头在视区改变时沿旧像素坐标缓动，导致时间码与刻度错位。记录 `stale-playhead-before.png`。新增实际 PaintList 回归验证当帧位置及视区外隐藏；`playhead-before-valid.log` 在修复前以坐标断言失败。首版测试查找了错误的 Quad 图元，已修正为实际 Procedural 细线，未将夹具失败当作缺陷证明。视区坐标改变时直接定位，普通同视区的寻帧动效保留。 |
| NAV-08 | 视觉改进 | 用户要求美化滚动条。dark.json 先增加细滑块高度、圆角、最小宽度及水平留白 token；panel 底色、border 细分隔线，闲置 disabled、悬停 muted、拖动 focus。视觉高度 6px，保持 18px 整条底栏命中，不缩减抓取容差。实际 GPU 截图待更新。 |
| OBS-01 | 正常分支 | 到达边界、100% 无可滚动范围、取消和普通未命中不是故障，不新增逐帧日志或重试。无新增文件 IO/worker。 |
| OBS-02 | 已有覆盖 | 引擎提交继续走既有错误信道；本批视图操作本身不发引擎命令。原生截图中的编辑及回滚沿用已有返回错误语义。 |
| OBS-03 | 待验证 | 没有声称覆盖所有可观测性缺口；截图与回滚双失败未故障注入。 |

## 验证

- 4 个新增回归首先通过，`tests.log`。首轮完整桌面测试 436 passed / 1 failed / 1 ignored，暴露 NAV-05 的实际下时间尺回归，`desktop-tests.log`。
- 修正布局后的完整测试 437 passed / 0 failed / 1 ignored，42.05 秒，`desktop-tests-final.log`。随后补充 NAV-06 的回执保护，最终完整桌面测试 438 passed / 0 failed / 1 ignored，40.39 秒，`desktop-tests-acceptance.log`；包含默认日志文件、Error stderr、stdout 隔离及阻塞观测的既有真实失败测试。带 FFmpeg 优化构建及原生中英文与样式验收待完成。
- 中间优化构建通过（7 分 44 秒，21 条既有警告）；后续编译因加入主窗/浮窗回执保护而被替代，确认归属后主动停止本任务旧 rustc PID 53724，不把该取消当成功构建。交付以最终构建为准。
- `fd854b2` 的优化构建通过（10 分 59 秒，21 条既有警告），完整中英文、样式浮窗、节点及共享 MCP 原生检查也通过；但目视发现 NAV-07，故该次不作为最终 UI 接受结果。加入播放头修正后的最终测试和原生验收待完成。
- 加入 NAV-07 后的一次默认并发测试为 438 passed / 1 failed / 1 ignored：失败是既有双击重命名测试（`desktop-tests-playhead-final.log`），新播放头回归通过。双击判定依赖 400ms 真实时钟且夹具每次创建系统字体 atlas；隔离重跑通过（`double-click-recheck.log`，0.48 秒）。没有更改原断言或生产双击阈值；完整套件以 2 个线程重跑通过：439 passed / 0 failed / 1 ignored，173.30 秒，`desktop-tests-delivery.log`。
- 新增实际 GPU 截图场景：`timeline-navigation`、`timeline-navigation-end`、`timeline-navigation-graph`、`timeline-navigation-hover`、`timeline-navigation-drag`。悬停与拖动场景输入真实鼠标状态，拖动场景断言成功抓住滑块。截图使用实际滚动条按下/释放并断言滚到合成末端且不发编辑/寻帧动作，不绘制示意图。

## DESIGN §0.7 逐项处置

| 历史基线 | 本批处置 |
|---|---|
| Path 六处纵向步长 | retained：未改属性布局。 |
| Path 两列分割 | retained：未改数值列。 |
| Path 混合按钮高度 | retained：未改字段尺寸与命中区。 |
| Path 数值背景 | retained：未改。 |
| Path 全宽布尔按钮 | retained：未改。 |
| Effect 行高与步进 | retained：未改效果面板。 |
| Effect 布尔按钮 | retained：未改。 |
| Effect 枚举按钮 | retained：未改。 |
| Effect hover alpha | retained：未改。 |
| Effect 层级 | retained：未改。 |
| Timeline 固定列坐标 | retained：保持既有列坐标和宽度，新增底栏不平移左侧属性；前一提交的 COL-01 原点修复继续保留。 |
| Timeline 吸附线颜色 | retained：不改吸附反馈。 |

新增滚动条尺寸先定义于 dark.json；颜色沿用现有语义 token；无新增依赖、资产或兼容名称。多 DPI、其他平台及远端 CI 仍待验证，不能用编译替代目视与交互验收。
