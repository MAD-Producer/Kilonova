# 截图、图像与取色读回的原生原因及清理

关联 #138。基于 `8bd6d245`，开始前核对其他分支的独有源代码和未提交改动。
预渲染分支在 image_capture.rs 新增原生缓存目标接口，未改本批读回函数；
renderer.rs 的其他分支新增字段/方法也不与本批枚举和 read_target 范围重叠。

## 原因传播与功能修正

九处 callback / mapping / mapped range 转换现在使用 ReadbackError，保留实际
RecvError、BufferAsyncError、MapRangeError。NativeReadback 保持原有
`GPU readback failed:` 前缀与三个调用方的阶段文本；source() 可追溯到原生对象。
Readback(String) 仍作为显式无类型文本边界保留，既有“文字不得伪造合成详情”
断言不变。没有从 Display 文本恢复原生身份，也没有扩大资源拒绝分类。

新增真实映射失败用例同时复现清理缺陷：回调已失败后，原 UnmapOnDrop 又执行
unmap，产生第二个 Buffer is not mapped validation 错误，导致 renderer 健康
检查失败。清理器现在区分已确认失败、已成功和仍待回调的映射。已失败时不重复
取消；其他情况继续 unmap。提前返回时可读取尚未消费的回调结果判断状态；
接收者退出后的内部回调属于原有 best-effort 通道，不改变公开回复语义。
没有清空 renderer 的 GPU 错误、放宽健康检查、重试、重启或增加等待。

## 实际故障、负对照与恢复

- 三个公开调用各自执行真实 GPU map_async，再在测试专用调度点调用 buffer.unmap。
  回调返回实际 BufferAsyncError，不伪造 Err。测试调度点仅 cfg(test)，普通构建
  不包含开关或故障入口。首次失败与后续显式调用成功使用同一 renderer。
- PNG 截图全部输出像素恢复一致；Float16 原始读回全部字节、storage 和 contract
  恢复一致；取色恢复同一像素。requested/failed/completed/in-flight 计数验证
  失败只记录一次、完成数不被误增；实际 device 健康检查仍必须通过。
- 关闭真实 channel 得到 RecvError；未映射真实 GPU buffer 得到 MapRangeError。
  两者验证原生原因与历史文字。这里是转换边界验证，不宣称从公开截图 API
  制造了回调永久失联或 mapped range 竞态。
- 单独清理器用例覆盖 pending/success/failed 三态，随后同一 buffer 再次实际映射，
  验证全部零初始化字节并正常 unmap。不是把 pending 当成功或缩短既有等待。
- 负对照只将三条生产 mapping 转换恢复为原 String 转换，新测试与断言保持。
  实际 1 项通过、3 项失败，均报原生 mapping 原因丢失。完整字节及 SHA 恢复后
  再执行最终测试。原首次 2 通过/2 失败也保留：它是真实重复 unmap 的发现证据，
  随后修复生产清理分支，没有修改健康断言来绕过该错误。

## 错误处理处置与限制

| 项目 | 分类与范围 |
|---|---|
| RB-PNG | 原生原因保留；实际公开截图读回映射取消、完整像素恢复 |
| RB-IMAGE | 原生原因保留；实际格式保真读回及 Float16 字节恢复 |
| RB-PICK | 原生原因保留；实际单像素取色读回及显式恢复 |
| RB-CALLBACK/RANGE | 真实底层失败的转换覆盖；不是公开读回的完整竞态注入 |
| RB-CLEANUP | 独立功能修正；失败后不重复 unmap，pending/success 仍清理 |
| RB-LOG | 无新增观察；renderer 返回原生 Err，既有宿主/桌面信道负责默认输出，本批未将这类 GPU 故障注入实际桌面 worker |

既有默认文件/Error stderr/stdout 传输验收见 desktop-error-transport-current.md、
native-paint-error-causes.md；这些是既有覆盖，不计作本批 GPU 故障端到端输出。
健康读回没有新增文件 IO 或日志；测试不会把正常预览变成逐帧读回。
无用户可见 UI、依赖、资产或像素计算语义改动，不据此关闭整个 #138。

原始命令、全部源码摘要、失败与负对照输出位于
`artifacts/issue-goal-current/readback-error-causes/`。

## 最终验证

- Vulkan 完整 motion-render 库回归：347 通过、0 失败、0 忽略。
- DX12 读回过滤 8 项及清理三态用例 1 项通过；均属于上述 347 项的重复后端
  验证，不重复累计。两种后端都实际制造映射取消并验证 renderer 健康与显式恢复。
- 宿主 failure_boundaries / render_composition_detail / resource_composition_details
  三组既有回归：11 + 3 + 4 = 18 通过。它们验证已有原生身份、主错误和文本边界，
  不是把本批 GPU 故障注入桌面信道；与渲染测试合计 365 个不同测试。
- motion-render / motion-host / motion-desktop 全目标及 FFmpeg feature 编译检查通过。
  既有 dead_code 警告仍存在，不宣称仓库零警告或完成 #125。
- 五阶段最终验证前后 2,335 个 Rust/TOML/WGSL/Cargo 文件摘要完全相同；负对照
  仅改变两份生产文件的三处 mapping 转换，恢复后的 SHA 与最终验证源码一致。
  保留首次实际清理失败、负对照失败、最终命令、stdout/stderr 和源码副本。
