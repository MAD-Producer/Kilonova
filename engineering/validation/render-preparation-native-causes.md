# 合成平面、镜头与颜色输出的原生错误（#138）

基线：`8bd6d245df35f9f1e77c48519620e7822dab5899`。开工核对开放 PR、
各工作树 merge-base 之后的实际代码及未提交修改，避开 UI、深度、跟踪和
预渲染的在途范围。`renderer.rs` 只增加错误变体及保留原因的转换。

## 逐项处置

| ID | 原边界 | 本批改动与实际证据 |
| --- | --- | --- |
| PREP-PLANE | 普通合成路径将 PlaneCompositor::prepare 的 Model 错误转换为 Invalid(String) | 沿用原 Diagnostic 显示，保留原 Model 原因。公开 draw 输入的三维图层投影为零矩阵时，真实 ProjectedGeometry 检查返回包含 layer/phase 的 Invalid；source() 与 Model 变体指向同一对象。先渲染非空红色图层，拒绝后完整 32×16 RGBA 与工程 JSON 不变；显式修正投影后同一 Renderer/目标恢复相同像素。 |
| PREP-LENS | 镜头 uniform 的 plate.inverse/apply 错误被转换为 Invalid(String) | 新 TrackingPlate 变体保留实际 plate::Error，显示前缀不变。公开构造器接受的有限正数极小 crop，使实际 apply 返回 NonFiniteProjection；公开 encode_images 返回该原生对象。失败前后预算快照相等，提交该失败调用的编码器仍保留完整目标像素和源像素；新调用改用正常 crop 后输出与全部源像素相同。 |
| PREP-COLOR | ColorOutput::new 丢弃 preferred_storage 的 ImageFormatError | 沿用原 Diagnostic 文本并保留 ImageContract 原因。真实私有转换入口分别接受反序列化的错误版本契约和非法精度契约，实际 preferred_storage 拒绝，原生对象身份、完整预算快照和契约不变；下一次显式有效构造成功。 |

没有修改验证条件、渲染算法、清理行为、线程、预算、期限、重试或重启。
没有新增日志站点；实际错误通过既有 Render → Host → Engine 通道向上传递。
普通缓存未命中、取消、空结果和健康 Present 的观察行为均不变。

## 覆盖边界

- 平面用例构造的是公开 Scene API 的畸形采样结果，不声称复现了正常编辑
  命令产生奇异相机。原始 `capture` 入口在更早的预检已返回原生 Model；
  新回归改用独立的公开 `draw`，专门验证本批实际修改的转换。
- 镜头实际故障经过 `plate.apply`。`inverse` 同样保留原生原因，但目前没有
  找到通过已验证 LensWarp 触发该分支错误的合法输入；没有伪造私有字段、
  使用 unsafe 或注入模拟 Err 来冒充这项运行时覆盖。
- 公开 ImageCaptureTarget 构造已经验证契约，RenderTarget 入口自动生成
  有效契约。颜色测试只证明实际私有转换器的拒绝边界；不声称正常公开目标
  可以携带错误版本，也不改变构造器的验证语义。
- 本批没有把上述 GPU 故障注入真实桌面 worker，因此没有新增它们的默认
  文件日志 / Error stderr / stdout 隔离端到端证据。既有通道验证见
  `desktop-error-transport-current.md` 与 `host-error-contract.md`；既有覆盖
  不能替代本批未执行的注入。默认 Info 设置保留在各实际运行记录中。

## 原始失败与验证

首轮新增平面测试通过 `capture` 触发更早的原生预检，实际返回 Model，
所以“必须经过 Diagnostic 边界”的断言失败。保留退出码 101 和完整输出；
只调整新夹具的调用入口为公开 draw，全部原因身份、显示和不变性断言保留。
修正后专项的 3 项新增回归及 1 项既有回归均通过。

负对照只恢复三个生产文件中旧的字符串转换，保留测试和全部断言。
第一轮 cargo 在库测试失败后停止，没有运行集成目标，因此不据此宣称
三个回归均被证明。后续使用 `--no-fail-fast` 执行全部选定目标；原始两轮
输出及按 SHA-256 恢复记录均保留，不改变测试通过标准。

最终同份源码验证：

- Windows Vulkan：完整 `motion-render --lib` 343 项、颜色输出 10 项、
  本批公开入口 2 项、镜头集成 18 项，合计 373 项不同测试通过，0 失败、
  0 忽略。新增颜色私有边界包含在完整库中，不重复累加。
- DX12：相同源码专项 4 项通过，含全部 3 项新增回归；属于上述渲染测试的
  重复子集，不增加不同测试总数。
- 宿主 `failure_boundaries` 11 项通过；与上述渲染回归合计 384 项不同测试，
  0 失败、0 忽略。它是既有结构化错误边界覆盖，不冒称本批真实桌面注入。
- 负对照全部目标实际为 1 通过、3 失败；三条新增回归均因原生原因丢失而
  失败。最终生产文件按原 SHA 恢复，后续完整正测、DX12、宿主和编译均通过。
- render / host / desktop 带 FFmpeg 的 `--all-targets` 编译检查通过，
  145.754 秒。保留既有警告，没有运行 cargo fix 修改其他工作树的代码。
- 各最终运行的 2,335 份 Rust/TOML/WGSL/Cargo 文件逐项同 SHA；源码冻结、所有原始
  退出码、负对照恢复和归档成员摘要由本批 `verify.py` 严格检查。

原始证据目录：`artifacts/issue-goal-current/render-preparation-error-causes`。
每次运行保存命令、源码前后 SHA、后端、编译版本、退出码及 stdout/stderr。

## 剩余范围

#138 仍开放。本批不代表所有跨 crate 错误及畸形模型已完成验收。
模型相机、蒙版上下文等剩余文本边界需要继续按真实可达调用核对。
没有 UI、素材、依赖或渲染像素语义变更；DESIGN §0.7 的视觉整改仍由
现有 UI 工作树推进，本批不宣称完成面板、截图或 DPI 验收。
