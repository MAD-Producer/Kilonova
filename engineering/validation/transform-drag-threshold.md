# 变换手势读取所属窗口的拖拽阈值

Refs #159。现有 `motion.drag_threshold` 的默认值是 3；数值字段、曲线手柄等已从窗口样式读取，但查看器变换控件仍固定比较 `movement < 3`。因此窗口设为 12 时，5 像素运动会意外开始编辑；设为 1 时，2 像素运动仍不生效。

## 改动与原契约

把 `PaintList` 当前窗口的样式借用传到变换手势处理器，读取已存在的 `motion.drag_threshold`。没有添加或改变 token，默认值仍取 `dark.json` 的 3；没有改变命中半径、绘制尺寸、颜色、修饰键、吸附、投影换算、命令或撤销通路。已开始的手势继续编辑，不因指针重新靠近起点而停止。

两个既有吸附测试只补入原默认样式参数，原断言保持。新测试经过真实 `Controls.draw_in_viewport`、窗口 `PaintList` 样式和实际 host 编辑/历史操作，没有伪造成功回复。

## 前后对照与回归

| 阶段 | 证据 |
|---|---|
| 校正后原实现 `0be8d2d4` | 同一三项新测试：1 通过 / 2 失败 / 0 忽略，7.81 秒；默认边界行为通过，大/小阈值均实际复现 |
| 修复 `a23ad8aa` | 3 通过 / 0 失败 / 0 忽略，7.11 秒 |
| 完整变换模块 | 27 通过 / 0 失败 / 0 忽略，15.98 秒；包含上述三项，不累加为三十项 |
| 消费者检查 | FFmpeg 桌面全目标检查通过，99.06 秒 |

前后源码仅生产处理器及两个已有测试的参数不同，新三项测试逐字节相同，运行期间全部受控源码保持。实际覆盖：

- 阈值 12、运动/释放 5 像素时不开始或提交历史，工程和 revision 保持；移动、旋转、缩放、锚点在 2D/3D 各执行。
- 阈值 1、运动 2 像素时核验各工具既有命令，实际修改工程、释放提交并一次撤销还原；同样覆盖四工具与 2D/3D。
- 默认阈值 3 的边界、阈值 12 的边界和累计指针行程；开始后回到阈值内仍编辑且不重复开始历史。
- 已有最近命中、窗口命中半径、缩放修饰键、投影/缩放、吸附、Alt 绕过和取消等用例见 27 项精确名称清单。

最初 `after-01` 为 2 通过/1 失败：新夹具误将锚点的合法 `Edit(Command::Anchor)` 排除。保留原失败；夹具改为逐工具核验确切命令后，先恢复原生产实现、重跑 `before-02`，再重施同一阈值修复。没有为通过测试改变锚点命令或削弱既有断言。

## DESIGN §0.7 逐项处置

下列三个基线文件与已合入 `f2f12514` 的内容逐字节相同，文件哈希在机器回执中。沿用并逐项核对该组合的边界记录；本批仅改变查看器手势启动阈值。

| 基线 | 处置与原因 |
|---|---|
| Path 六处纵向步进 | retained with reason：不修改 Path 行，未重新做其视觉验收。 |
| Path 两列分割 | retained with reason：不修改列宽或布局。 |
| Path 按钮高度 | retained with reason：保持现有按钮与命中范围。 |
| Path 自绘字段背景 | retained with reason：不修改数值字段。 |
| Path 布尔宽按钮 | retained with reason：不修改该控件。 |
| Effect Controls 行高/步进 | retained with reason：不修改效果属性行。 |
| Effect Controls fx 布尔 | retained with reason：不修改效果开关。 |
| Effect Controls 枚举 | retained with reason：不修改枚举和控件。 |
| Effect Controls 手工 hover | retained with reason：不修改颜色或 hover。 |
| Effect Controls 层级 | retained with reason：不修改属性树。 |
| Timeline 标签栏坐标 | retained with reason：保持兼容坐标。 |
| Timeline 吸附线颜色 | retained with reason：保持当前反馈与颜色。 |

变换控件中其他既有绘制字面量也保留，不把本批声称为该文件整体 token 化。

## DESIGN §12 的验收范围

默认行为、窗口样式覆盖、命中容差、手势历史/释放、2D/3D 数据通路由以上实际交互与完整变换回归证明。低于阈值是正常分支，不记录故障。未改属性行、文字/颜色、窗口布局和绘制，因此相关既有视觉基线 retained with reason。没有新增原生 OS 鼠标注入或跨 DPI 截图，不将这些列为已完成验收，也不宣称整个 #159 完成。

[机器回执及用例清单](transform-drag-threshold-results.json)与[原始日志/源码哈希证据](data/transform-drag-threshold-evidence.zip)保留全部前后阶段，包括失败夹具。证据仅为本项目自制测试产生的 JSON/日志，没有第三方依赖、资产、EXE/DLL/PDB/字体。最终 CI 仍待验证：五项有效检查、三个桌面 job 各 27 项实际用例，以及所有 job 完整代码树与最新 main 组合一致；本地编译不代替这些条件。

## 深度边界主线合入后的组合重验

主线合入深度合成边界修复 `67760b8b` 后，组合 `77baa137` 重新编译并实际通过新三项、完整变换模块 27 项和 FFmpeg 全目标检查。三项是二十七项的子集；本轮重复不累加数量。源码全程保持，二进制 SHA256 `2d950e5e1a80556d28e58cac7a02babdb20fa2fbc3bc5d9b2a638aac55c494b1`。

[当前组合回执](transform-drag-threshold-current-main.json)及[本轮原始证据](data/transform-drag-threshold-current-main.zip)保留这次运行；旧 head 的五项 CI 成功不能替代最终新 head 的检查。后续仅添加这些文档，最终完整代码树与逐项 CI 仍单独核验。

## 主线 3865b0fe 合入后的组合重验

组合 `67739b34` 重新编译并实际通过新三项、完整变换模块 27 项和 FFmpeg 全目标检查。三项是二十七项的子集；本轮重复不累加数量。源码全程保持，二进制 SHA256 `12ce69c84b6749a35ab749e781d250db3762ff5cd6b826e9ded7b2fc7d31faea`。

[当前组合回执](transform-drag-threshold-current-main-02.json)及[本轮原始证据](data/transform-drag-threshold-current-main-02.zip)保留这次运行；旧 head 的五项 CI 成功不能替代最终新 head 的检查。后续仅添加这些文档，最终完整代码树与逐项 CI 仍单独核验。
