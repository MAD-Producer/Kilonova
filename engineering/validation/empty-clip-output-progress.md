# 无输出裁剪的缓冲分配验收

基线 `bbb1ad09b7389220d48f9c4ea40f4f38fb7c7f14`，代码提交 `0c0b23b63b5a15b8360b7df3a3e74dbf03177c18`，关联 #129。`clip_triangles` 原先在得知有无输出前按输入顶点数分配 Vec。现在从空 Vec 开始，仅在首个输出三角形到达时 `reserve_exact` 原来的容量；空结果不再为这个输出缓冲分配。裁剪算法、浮点计算、形状颜色和顺序、公开 API 均未更改，已有断言保留。

## 审计与结果

实施前核对 #129 当前状态（开放、无 assignee）、关联 PR、开放 PR、587 个引用和 27 个 paint.rs 版本，并检查 114 个工作树中的此裁剪函数。既有 #493 去掉四边形展开的临时 Vec，#475/#487 等处理可见路径及索引映射；未找到本次按首个输出分配的重复实现。工作从独立 `perf/empty-clip-output` 分支开始。

19 组工作负载，每组 64 个输入形状，直接调用真实 `PaintList::clip_to`。统计范围仅是裁剪调用，输入构建与结果序列化在计数之外。共减少 352 次分配请求、10496 字节申请量；完全在外的四边形和三角形两组分别由 64 次降至 0。其余各组未增加请求次数或申请字节，输出的逐位坐标（含非有限值）、颜色、顺序和保留容量完全相同。该计数不是 RSS、峰值内存或帧率。

| 场景（64 输入/组） | 分配请求：前 → 后 | 申请字节：前 → 后 |
| --- | --- | --- |
| quad_left | 128 → 128 | 9216 → 9216 |
| triangles_left | 128 → 128 | 9216 → 9216 |
| quad_corner | 128 → 128 | 9216 → 9216 |
| triangles_corner | 128 → 128 | 9216 → 9216 |
| quad_outside | 64 → 0 | 3072 → 0 |
| triangles_outside | 64 → 0 | 3072 → 0 |
| quad_rotated | 192 → 192 | 21504 → 21504 |
| triangles_rotated | 192 → 192 | 21504 → 21504 |
| quad_contained | 0 → 0 | 0 → 0 |
| triangles_contained | 0 → 0 | 0 → 0 |
| quad_max_edge | 0 → 0 | 0 → 0 |
| triangles_max_edge | 64 → 64 | 3072 → 3072 |
| empty_input | 0 → 0 | 0 → 0 |
| incomplete_inside | 64 → 0 | 512 → 0 |
| incomplete_tail | 128 → 128 | 6144 → 6144 |
| zero_width | 64 → 0 | 1536 → 0 |
| zero_height | 64 → 0 | 1536 → 0 |
| nonfinite | 128 → 128 | 4608 → 4608 |
| mixed_kept_removed | 33 → 1 | 1792 → 1024 |

原有 UI 回归完整运行，23 个目标合计 179 passed、0 failed、0 ignored，包含真实 GPU 测试及 InputView 的编译失败约束；未改断言来放行。保留输出时的容量与增长逻辑通过前后数据对照；空输入、不完整三角形、零宽/零高、非有限坐标及混合保留/移除均有实际调用记录。

## 复现与边界

在仓库 Windows 构建环境及锁定依赖下，将归档中的独立探针临时放入 `desktop/ui/examples/empty_clip_comparison.rs`，分别在基线及补丁源码运行 `cargo run -p motion-ui --example empty_clip_comparison --no-default-features --locked --offline -j1 -- <结果.json>`，对比分配请求与剔除两个计数字段后的完整结果。之后移除临时探针，执行 `cargo test -p motion-ui --locked --offline -j1 -- --test-threads=1`。探针不进入产品或最终分支，未增加依赖。

审计项 UI-CLIP-EMPTY-01：新增证据是延迟分配及 19 组逐位对照；UI-CLIP-EMPTY-02：既有覆盖是完整 UI/GPU 回归及原断言；UI-CLIP-EMPTY-03：正常剔除和空输出不是故障，不新增日志；UI-CLIP-EMPTY-04：未改错误返回、panic 处理、线程、重试、协议或 GPU 绘制流程；UI-CLIP-EMPTY-05：其余 UI 分配及整个 #129 的验收仍待对应专项实现，不能据此关闭集合 issue。尚需新提交五项非跳过 CI 通过。

已核对 PROVENANCE.md、ATTRIBUTION.md、NOTICE；数据及探针均为本仓库独立编写的测量输出，没有第三方资产、Adobe 资源、兼容性名称或新依赖。没有修改 panel、样式 token、行高或命中区域，不宣称 DESIGN §12 全部完成。

## DESIGN §0.7 逐项处置

以下仅说明本改动边界，不宣称重新证明历史条目仍存在或已修复。

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

[结构化结果](empty-clip-output-results.json)；[原始证据](data/empty-clip-output-bbb1ad09.zip)，SHA-256 `b048f4f7efd79f3554bc318558f672afde4648c5efa66ff821659806cc3adb56`。
