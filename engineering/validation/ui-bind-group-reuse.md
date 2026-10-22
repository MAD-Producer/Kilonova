# UI 绘制器绑定复用验收

日期：2026-10-22（Asia/Shanghai）。对应 #129 的绘制器每帧绑定重建子项；不关闭整个合集。基线 main `8c5a8d09`；分支 `codex/ui-bind-group-reuse`。

开始前检查开放 PR、#129 timeline、所有分支历史中相关绑定修改，以及既有 UI 图集/图标工作区的状态。PR #417 是效果搜索反馈，本次只修改底层 Painter 绑定生命周期，不编辑它的面板、主题或动作。已有蒙版绑定复用属于 motion-render，不能替代此处 motion-ui 的重复创建。

## 交付与边界

| ID | 分类与实际变化 |
| --- | --- |
| UIB-01 | 性能修复：原 render_scaled 每次创建 solid 绑定，有图集时另建 glyph 绑定。现在 solid 随 Painter 创建一次，glyph 随图集纹理生成创建一次；同尺寸图集内容更新和普通绘制不再重建绑定。纹理尺寸变化必须替换 glyph 绑定，避免继续采样旧图集。 |
| UIB-02 | 新计数：bind_group_creations 为每个 Painter 实例生命周期的饱和 u64 总数，包含 solid 与每代 glyph；仅新建 Painter 自动重置。它不代表当前驻留数量、显存峰值、全应用 GPU 分配或耗时，不在健康绘制路径写文件/日志。 |
| UIB-03 | 既有行为：保留每帧 uniform/顶点内容更新、绘制顺序、纹理内容更新、像素格式、采样器、alpha、裁剪、现有返回值与失败观察。没有新缓存键或无界历史；Painter 只持有当前 solid/glyph 绑定。 |
| UIB-04 | 原生结果：组件框架中英文主窗/浮窗、数值事务、撤销和停靠检查通过，实际 GPU 截图文字和控件可见。无面板布局或命中区变化。 |

## 实际验证

工作区 `E:/Dev/aem/.worktrees/ui-bind-group-reuse`。锁文件/离线依赖、已有 MSVC、本地关闭增量并省略链接符号；没有修改仓库编译配置。

- 新 GPU 回归实际通过：4 个空帧只保留 1 个 solid 绑定；上传图集后，16 次交替 1×/2× 比例绘制的绑定总数保持 2，绑定对象身份保持；红色几何和绿色字形逐点读回正确。相同尺寸的全透明图集更新后旧绑定直接读到透明像素；扩容为不透明图集后生成第 3 个绑定，字形像素恢复且 solid 绑定保持。
- 错误对照：临时禁止已有 glyph 绑定在图集扩容时替换，同一回归在真实 GPU 像素断言失败：实际 `[0,0,0,0]`，应为 `[0,255,0,255]`。没有修改断言；恢复正确实现并核对 SHA-256 后运行全套。原对照日志 `.tools/bindings-stale-control.log` 保留，错误实现不提交。
- `cargo test -p motion-ui --locked --offline -j2 -- --test-threads=1`：20 组、139 通过、0 失败、0 忽略，含新 GPU 回归、五档 DPI 的既有像素回归、图集生命周期、输入、布局和文档测试。记录 `.tools/ui-tests.log`。
- 构建真实 framework 示例后执行 `tools/ci_ui_framework.py`：通过。`artifacts/ui-bindings-framework/` 保留中英文主窗/浮窗、实际数值输入/撤销、停靠检查、12 张 PNG 及 JSON；已目检英文主窗和中文浮窗，没有字形乱码、控件叠放或裁剪变化。原生输入为既有归一化事件驱动，不冒充人工物理键盘或系统 IME 服务测试。
- `tools/validate_provenance.py`、新增测试文件 rustfmt 检查、`git diff --check` 通过。没有新依赖、外部算法、字体、图标、图片、shader、兼容名称或其他第三方材料。

## 观察与责任边界

没有新增日志故障点或更改命令/线程协议。无图集的空帧正常返回；有 glyph 却没有图集的退化仍由既有 observation.atlas 观察，现改为检查实际绑定是否缺失。全套既有 paint_observability、failure_boundaries 和 workspace_observability 实际通过，覆盖默认文件、Error stderr、stdout 隔离、限频与恢复；这属于既有覆盖复验，不记为新增日志修复。绑定创建的 GPU/设备错误沿既有 wgpu 观察和调用边界处理，未用重试或吞错掩盖。

DESIGN §0.7 的 12 项均 retained with reason：Path 六处步进、双列、按钮高度、数值背景、布尔控件；Effect 行步进、布尔、枚举、hover、层级；Timeline 固定列与吸附色。本次不编辑任何 panels 文件、dark.json、行几何或命中区。§12 对应的像素、实际窗口、语言和输入回归证据见上；不宣称完成全部设计整改。

剩余范围：#129 的其他子项继续开放；未进行全应用帧耗时/功耗基准、全部 GPU 厂商或长期设备丢失验收，也不把减少绑定创建次数表述为固定百分比的性能提升。未单独证明任意交叠提交的驱动驻留峰值。跨平台检查与合并以 PR 页面为准。
