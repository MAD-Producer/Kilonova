# #257：三维位置手柄的连续空间键身份

接续 `59ccc190`。开工核对开放 PR 与近期本地分支，其他工作集中在跟踪和预渲染，没有占用本轮范围。

## 行为与范围

三维位置轴与平面手柄在按下时捕获空间关键帧稳定 ID。Roving 因编辑距离而重新分配时间后，后续预览仍修改原键，不按旧时间创建新键。命令整批预检成功后才更新拖动状态；目标删除、时间上下文变化时取消手势，迟到松开不重新认领。小曲线选区发布错误补充实际对象与键 ID 上下文。

本轮只覆盖三维 Position 轴／平面手柄。二维／三维自由移动仍使用原 PreviewDrag／ViewerDrag；锚点补偿双轨道、缩放及旋转没有因此获得新的空间绑定能力。没有改动布局、尺寸、颜色、图标、命中范围、工程格式或生产预算，没有引入依赖和素材。

## 验证与原始失败

最终同一二进制 SHA-256 为 `659a0343dd7232277244b016711c388306d379e17410bf5cdefc4b9b545ffcb7`，45 个不同用例通过：小曲线 worker 3、变换控件 41、显式原生用例 1。原生项在普通组中 ignored 后显式通过，仅计一次；另外两个既有手工原生用例未运行。

真实 worker 覆盖普通／正放／倒放 × 固定／Roving × 轴／平面 × 提交／取消共 24 种组合，每次连续三次预览。验证精确目标 ID、独立几何期望、全部键身份与数量、完整工程不变边界、保存加载、一次撤销重做及目标删除后的迟到松开。原生 GPU 用例覆盖中文主窗正放和英文浮窗倒放，独立进程重开并比较完整工程，产生六张截图。已目视检查两个修改后画面；图片不是全平台、全 DPI 或所有面板的验收。

修改前后像素变化分别为 4403 和 4445；修改后与重开图分别差 60 和 0 像素，因此不声称所有重开截图逐像素相同。工程内容相等由原生用例独立断言。源码哈希、逐组结果、用例名、截图和日志见[证据清单](layer-time-spatial-viewport-evidence/manifest.json)及[证据包](layer-time-spatial-viewport-evidence/evidence.zip)。带 FFmpeg 的 workspace all-targets 检查通过；之后仅调整新增原生夹具的手势输入，并成功重新编译和运行上述最终测试。样式消费者、改动小模块格式和 diff 检查通过。

保留的首轮记录与处置：

- 首次运行未指定可写 TEMP，临时目录创建失败；补齐验收环境，没有修改生产临时目录策略。
- 新增选区故障用例原先假定按下只产生 Begin，但指针投影浮点往返还可能产生微小曲线预览。改为断言 Begin、合法的同键预览，并严格比较故障与健康提供者的完整动作序列；完整工程断言保留。既有断言未改。
- 新增夹具 out_frame 的整数类型不匹配，改为受检转换；保留首次构建失败。
- 首次原生手势位移不足以达到生产拖动阈值，动作为空。增大新增夹具输入并显式断言超过原阈值，保留原命令、键身份和工程断言；生产阈值与命中区域未改。

前阶段整组 graph_ranges 压力回归被外层期限中断，不能算作通过；本轮定向测试不替代该验收。

## 观测处置

| 项目 | 分类、证据与限制 |
|---|---|
| VIEW-BEGIN | 新增 transform.spatial_key_begin。真实渲染入口注入重复 ID，拒绝开始手势；180 次交错健康调用验证默认文件日志、Error stderr、stdout 隔离与阈值抑制。 |
| VIEW-CONTINUE | 新增 transform.spatial_key_continue。真实手势中注入重复 ID，错误进入观察信道并取消；正常删除目标仅取消，不报故障。 |
| VIEW-CMD | 新增 transform.spatial_key_command。在独立命令组装边界注入缺失目标，验证拒绝批次、last_value 不变及健康恢复。此证据不表示普通 UI 能绕过更早的继续检查。 |
| MINI-SELECTION | 补齐前阶段缺口：真实小编辑器入口缺失 document_generation 时发布选区失败，完整工程不变、动作序列与健康提供者一致，恢复后能选择原 ID；默认文件／stderr／stdout 和累计阈值均有证据。 |
| OBS-SCOPE | 沿用面板进程生命周期累计计数：首次 Warn、累计第 60 次 Error、之后抑制；成功不重置，退出进程重置。不是连续失败计数，也不将 60 次等同一秒。 |
| WORKER | 复用现有非 Present Info start、在途状态、请求 ID 和错误信道。本轮没有新增 worker 阻塞、超时、重启或 panic hook 实现，不将手势用例充当新的阻塞观测证明。 |
| NORMAL | 取消、目标消失和迟到松开属于正常分支。没有吞掉原始错误、清空错误、修改返回值或增加重试来伪装成功。 |

## CI 与剩余工作

旧头 `59ccc190` 的 [Rust/GPU CI](https://github.com/MAD-Producer/Kilonova/actions/runs/41393506472/job/125110053255) 失败于既有 `expression_references::whole_vectors_separated_axes_and_opacity_use_evaluated_property_units`：执行预算超限。原始日志收录证据包。本地未改表达式代码的 preparation／references／sandbox 三组共 18 项通过，未复现该失败；这不能证明 CI 已修复。未调整执行预算、断言或重试策略，新提交仍需远端验证。

#257 保持打开。自由移动和伴随锚点补偿的连续空间键手势、真实媒体预热／设备／高倍速音频及最终集成仍有工作。空间曲线复制粘贴、空间值方向拖动和 Roving 区间缓动保留原有边界，不宣称已完成。

## DESIGN §0.7 与 §12

| 基线位置 | 状态与原因 |
|---|---|
| path_controls.rs 六处行步进 | retained with reason：未改路径步进。 |
| path_controls.rs 双列分割 | retained with reason：未改列宽。 |
| path_controls.rs 混合按钮高度 | retained with reason：未改按钮和命中范围。 |
| path_controls.rs 自绘数字背景 | retained with reason：未改数字绘制。 |
| path_controls.rs 全宽布尔按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 行高与步进 | retained with reason：未改效果布局。 |
| effect_controls.rs 布尔文本按钮 | retained with reason：未改布尔控件。 |
| effect_controls.rs 全宽枚举按钮 | retained with reason：未改枚举控件。 |
| effect_controls.rs 自绘悬停透明度 | retained with reason：未改悬停绘制。 |
| effect_controls.rs 树层级 | retained with reason：未改效果树。 |
| timeline.rs 十一列坐标 | retained with reason：保留原列坐标。 |
| timeline.rs 吸附线颜色 | retained with reason：未改吸附绘制。 |

§5.2 没有新增属性行或缩小命中区域。§12 仅完成上述 Windows 两种窗口／语言用例；全平台、DPI、全部布局与面板视觉整改仍 pending verification。
