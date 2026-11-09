# 临时纹理规划与嵌套曝光原生错误（#138）

基线：`adf529314647e559fad57a7da2107a1a3c4ffc92`。开工核对工作树实际
修改及开放 PR 文件范围，避开深度、跟踪、预渲染和桌面 UI 的在途实现。
`renderer.rs` 仅修改错误枚举及其文本适配，不修改其他分支的帧缓存或上传实现。

## 修复范围

| ID | 原边界 | 处置与证据 |
| --- | --- | --- |
| RP-GRAPH | 临时纹理 def/use 图失败转换成 Invalid(String) | RenderError::ScratchGraph 保留原 GraphError，包括 pass、slot、draw 和 limit 字段。公开错误类型通过 effect_plan 重导出，不公开规划器内部状态。真实未初始化输入和非法 draw 范围触发原检查；source 指向枚举内的原生原因。 |
| RP-PLACE | 物理分配失败转换成 Invalid(String) | RenderError::ScratchPlacement 保留 PlacementError。零尺寸与实际 u64 容量溢出保留 resource/reason，失败不返回部分 Layout；恢复后分配表、绑定、输出与容量和原值一致。 |
| RP-NESTED | 子合成曝光初始化丢失 Model 原因 | 沿用原 Diagnostic 文本，内层保留实际 Model::Composition。有效冻结工程配合调用方不一致的 Scene 子合成 ID，实际触发 composition_missing；拒绝发生在媒体回调之前。随后新曝光任务读回完整 8×8 绿色像素，工程 JSON 不变。 |
| RP-TEXT | 显示及拒绝分类 | 三处历史显示前缀保持，planning_message 不重复添加 GPU validation 前缀。普通文字不能获得原生图错误身份；规划溢出不伪装运行时预算拒绝。 |

这不是新观察系统：没有新增日志、线程、错误清除、重试、预算、期限或重启。
实际错误继续由既有 Render/Host/Engine 通道传递。本批验证的是上述真实
提供方边界；没有把这三个内部规划错误注入真实桌面 worker，因此不宣称本批
新增了它们的默认文件/Error stderr/stdout 端到端验收。该信道的既有覆盖见
`desktop-error-transport-current.md`，不能替代未执行的故障注入。

## 验证与原始失败

- 最终源码完整 `motion-render --lib`：336 通过、0 失败、0 忽略，Windows Vulkan。
- 相同源码 DX12 `error_tests`：13 通过，包含三条新增回归，不与完整 336 重复计数。
- 负对照只恢复旧的三个生产转换，保留测试与全部断言：10 通过、3 失败，
  三条新增回归均实际报告原因已被拍平。最后按 SHA-256 恢复生产文件；
  DX12 的相同测试重新编译并通过。没有改既有测试断言来迎合实现。
- 首次新增测试编译失败：ExposureFrame 构造器需要可变 Renderer 借用。
  原始输出保留；修正新夹具借用后专项通过。它不是产品缺陷负对照。
- 曝光四组集成回归 30 通过；与完整渲染库合计 366 项不同测试通过、0 失败、
  0 忽略。DX12 专项是重复子集，不累加。
- 带 FFmpeg 的 render/host/desktop 全目标编译检查通过，保留既有警告。
  最终源文件身份和原始输出保存在本批 `verified.json` 及对应运行目录。
  首次证据校验脚本将 Windows 路径反斜杠当成清单键而报 KeyError；修正路径
  标准化后仍逐文件严格检查 SHA，没有改变产品、测试断言或通过标准。

本地原始证据目录：`artifacts/issue-goal-current/render-plan-error-causes`。
每阶段记录执行命令、编译版本、后端、退出码、前后源码 SHA 与 stdout/stderr；
负对照另保留原转换和恢复后字节。成功编译不当作全 UI、全后端或故障信道验收。

## 保留范围

#138 仍开放：视频提供方及曝光媒体准备、取消/清理等仍有更早的文字边界，
畸形模型的总体无 panic 验收也未完成。这里不把三个修复扩张为合集完成。
没有 UI、样式、素材、依赖或渲染像素语义改动；DESIGN §0.7 的视觉整改
仍由原 UI 工作树负责，没有据原生 readback 声称完成面板/DPI 视觉验收。
