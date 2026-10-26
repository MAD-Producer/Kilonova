# 节点卡片端口标签规格复用验收

基线 `f521593d03df8a6850738859e3664c1fac24eb3f`，本地仅代码提交 `7d5fc80932040676baacae0e4ace5ab67c0711c8`，关联 #134。卡片构建已解析端口类型与标签规格，画布标签原先再次创建同一份 BTreeMap。现在对同一来源节点的非空插口卡片借用这份当帧数据，按原输入/输出排序绘制；空插口或节点身份不匹配时继续使用原解析与观察路径。

## 身份与生命周期

每张临时 Card 增加一个只比较、不解引用的节点身份指针，卡片只在当前绘制过程中使用。编辑、组导航、撤销及替换文档仍走现有卡片重建机制，没有跨帧缓存。指针不会进入工程、历史、绘制命令或日志。

该身份检查保留了畸形重复 ID 的首个匹配节点语义。新增回归对普通及逆序卡片、首节点可用/不支持、中英文，比较缓存标签与真实文档解析的全部字形结果；该回归在原实现和最终补丁上均通过。原有断言不改，两个合成卡片测试只补上空身份字段。

开始前检查开放 PR、588 个引用、15 个 canvas.rs 版本及 117 个包含该文件的工作树；现代实现均存在两次规格调用，没有此复用方案。其他人的 #499 关键帧选择改动未触及节点编辑器，已先进入所用基线；#500 处理 UI 裁剪，与本次源文件不同。

## 实际结果和代价

20 组真实 NodeEditor.draw 工作负载，覆盖中英文、900×600/1600×1000、普通四节点、Gradient 默认值、折叠节点、128 节点及全部不可见视野。每组三帧预热、20 帧计数，共 400 个测量帧；输入构造、文件写入和调试序列化不在计数范围。

共减少 14320 次分配请求，申请字节净减少 3825360。20 个静态快照与 24 个缩放、平移、拖动、撤销及组导航快照逐字节一致，内容包括完整绘制列表、文档、视图、动作、选中对象与历史状态。

身份指针有明确代价：在这次 64 位构建的四组全部不可见场景中，每帧额外申请 1024 字节，80 帧共增加 81920 字节，分配请求次数不变。这是临时卡片存储的申请量，不是常驻内存或峰值；净节省已包含此代价。没有据此推断 RSS 或帧率，也没有宣称所有视野的申请字节都下降。

| 场景 | 请求次数：前 → 后 | 申请字节：前 → 后 |
| --- | --- | --- |
| English-4-defaultsfalse-collapsedfalse-culledfalse-900x600 | 7400 → 6760 | 4880000 → 4700080 |
| English-4-defaultsfalse-collapsedfalse-culledfalse-1600x1000 | 7700 → 7060 | 4985300 → 4805380 |
| English-3-defaultstrue-collapsedfalse-culledfalse-900x600 | 7380 → 6660 | 5267080 → 5106920 |
| English-3-defaultstrue-collapsedfalse-culledfalse-1600x1000 | 7720 → 7000 | 5108140 → 4947980 |
| English-4-defaultsfalse-collapsedtrue-culledfalse-900x600 | 5720 → 5360 | 4379260 → 4273220 |
| English-4-defaultsfalse-collapsedtrue-culledfalse-1600x1000 | 6020 → 5660 | 4484560 → 4378520 |
| English-128-defaultsfalse-collapsedfalse-culledfalse-900x600 | 36980 → 36260 | 11928880 → 11736000 |
| English-128-defaultsfalse-collapsedfalse-culledfalse-1600x1000 | 52960 → 49960 | 21290920 → 20422400 |
| English-128-defaultsfalse-collapsedfalse-culledtrue-900x600 | 32500 → 32500 | 9680280 → 9700760 |
| English-128-defaultsfalse-collapsedfalse-culledtrue-1600x1000 | 35180 → 35180 | 10103820 → 10124300 |
| Chinese-4-defaultsfalse-collapsedfalse-culledfalse-900x600 | 7120 → 6480 | 3936060 → 3756140 |
| Chinese-4-defaultsfalse-collapsedfalse-culledfalse-1600x1000 | 7440 → 6800 | 4118160 → 3938240 |
| Chinese-3-defaultstrue-collapsedfalse-culledfalse-900x600 | 7000 → 6280 | 3681780 → 3521620 |
| Chinese-3-defaultstrue-collapsedfalse-culledfalse-1600x1000 | 7380 → 6660 | 3890200 → 3730040 |
| Chinese-4-defaultsfalse-collapsedtrue-culledfalse-900x600 | 5440 → 5080 | 3322500 → 3216460 |
| Chinese-4-defaultsfalse-collapsedtrue-culledfalse-1600x1000 | 5760 → 5400 | 3504600 → 3398560 |
| Chinese-128-defaultsfalse-collapsedfalse-culledfalse-900x600 | 36620 → 35900 | 10794660 → 10601780 |
| Chinese-128-defaultsfalse-collapsedfalse-culledfalse-1600x1000 | 51860 → 48860 | 17896120 → 17027600 |
| Chinese-128-defaultsfalse-collapsedfalse-culledtrue-900x600 | 32380 → 32380 | 9035060 → 9055540 |
| Chinese-128-defaultsfalse-collapsedfalse-culledtrue-1600x1000 | 35080 → 35080 | 9535400 → 9555880 |

节点编辑器完整相关回归：164 passed、0 failed、0 ignored。最终测试移除了临时计数器模块，所有实际修改文件都与验证时的散列一致。

## 失败路径与审计处置

- NODE-PORT-01：新增观察证据是成功路径的当帧数据复用、400 帧计数和 44 个完整输出对照；没有新增产品日志或错误 UI。
- NODE-PORT-02：空插口、未知操作和身份不匹配沿用原 fallible 解析及原回退，不将失败伪装成缓存成功。首次探针误用不支持默认值的 Math 输入，被既有校验拒绝；该失败保留在本地，最终改用现有 compact_gradient 的 falloff 默认值，未修改产品校验。
- NODE-PORT-03：实际运行原有受控缺陷测试的子进程，默认 Info 文件日志核对 canvas.sockets、canvas.labels、context.ports、inspector.ports、inspector.help_ports、request.field、layout.arrange 七个站点。每站点保留首次 Warn 与第 60 次 Error，Error 同时出现在 stderr，stdout 无日志；文档、修订与既有断言均保留。
- NODE-PORT-04：上述原有计数器按 NodeEditor 实例及固定站点累计并饱和，成功不重置，新实例重新开始；本次不修改该生命周期策略，也不把 60 次解释成一秒。没有新观察键、持久错误原因、重试、线程或协议变更。
- NODE-PORT-05：节点高度、其他布局计算和逐帧分配仍有独立工作；未关闭整个 #134，也未宣称其他 DPI、原生窗口呈现或 DESIGN §12 全部验收通过。

## 复现与来源

在仓库 Windows/FFmpeg 构建环境及锁定依赖下，使用归档 probe.rs、allocator.rs 和 run_phase.py：向 node_editor.rs 临时添加 cfg(test) 探针模块，分别在基线与补丁执行指定的真实绘制探针，比较两个完整输出目录；完成后删除探针模块，再执行 `cargo test -p motion-desktop --bin motion-studio node_editor:: -- --test-threads=1`。实际命令包含本机内存控制构建选项，完整记录在 JSON 中。失败子进程直接复用了原测试，未修改原断言。路径为原机器证据位置，离线复验需重定位。

已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE；没有新依赖、兼容性名称或第三方资产。图形、样例和探针来自本仓库独立实现；没有复制 Adobe 资源。没有新增颜色、尺寸、样式 token、行高或命中区域，保留现有节点样式与布局问题的专项整改边界。

## DESIGN §0.7 逐项处置

下表只记录本次边界，不声称重新证明旧条目仍存在或已修复。

| 基线条目 | 状态 | 原因 |
| --- | --- | --- |
| path_controls 六处行高 | retained with reason | 不改行高或纵向节奏 |
| path_controls 双列宽度 | retained with reason | 不改列宽 |
| path_controls 按钮高度 | retained with reason | 不改按钮尺寸或命中区域 |
| path_controls 数值背景 | retained with reason | 不改字段状态绘制 |
| path_controls 布尔控件 | retained with reason | 不改控件或事件路由 |
| effect_controls 行高与步进 | retained with reason | 不改属性行布局 |
| effect_controls 布尔按钮 | retained with reason | 不改布尔控件 |
| effect_controls 枚举按钮 | retained with reason | 不改下拉控件 |
| effect_controls 悬浮 alpha | retained with reason | 不改 hover 或 token |
| effect_controls 缩进层级 | retained with reason | 不改属性树 |
| timeline 标签列坐标 | retained with reason | 按兼容基线保留 |
| timeline 吸附引导颜色 | retained with reason | 不改颜色，未新增 token |

[结构化结果](node-port-labels-results.json)；[原始证据](data/node-port-labels-f521593d.zip)，SHA-256 `4c4955a293636f7d50d973f4073eb2725b6ff8d8221f348277a72219f702c214`。合并前仍须新提交的五项非跳过 CI 成功。


## F9 协议修复后的真实组合验证

原 #502 的 Windows CI 继承了主分支 F9 验收夹具不兼容，仍记录为失败，PR 暂为草稿。另建独立工作树，把 main `4afb6cfc036393d3ec02062ae7a846a26403b1df`、#503 `d5d82b7e04d7432bb830a3083906f363ea779c4c` 和 #502 原 head `a9947e8ce401dad99f15d9126639c9e0709b5d03` 合并为 `c5e6d0b248f72ec1eb54e387a8cd6966919759c1`，树 `c24bada05e4b2825ace476a1f0355e8c419d3d1b`；无冲突，节点来源与 #502、原生验收来源与 #503 分别按 Git blob 逐文件核对一致（各次运行仍冻结并核对原始文件哈希，Windows 工作树换行可不同）。没有加入另一个高度复用改动。

在该实际组合下，最终源码无探针，节点回归 164 项及原生协议/真实写入故障恢复 3 项全部通过，0 failed、0 ignored。中英文默认/最小窗口四个真实 GPU 属性场景全部通过，共 231.12 秒；采用独立 240 秒本地预算并沿用原尺寸、项目还原与扩大预览检查，不能替代原 40/90 秒完整脚本或 Linux 主分支原生验收。写入故障仍保留原原因、900×600 捕获恢复、项目/revision/历史及默认日志信道断言。

组合证据：[data/node-port-labels-f9-integration-4afb6cfc.zip](data/node-port-labels-f9-integration-4afb6cfc.zip)，2096465 字节、77 项、SHA-256 `f77850e9a0affe763998c905c9dc6d9219ce153f562ef6935bf88641da118510`。结构化结果见 [node-port-labels-integration-results.json](node-port-labels-integration-results.json)。下一步先让 #503 按五项成功检查合入，确认实际主分支原生路径，再同步本 PR 并验证发布头的 CI；上述组合通过不表示原失败头的 CI 已经通过。
