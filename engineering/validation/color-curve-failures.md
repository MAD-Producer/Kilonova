# 颜色曲线求值的错误边界（验收进行中）

本批继续处理 #138 的模型不变量及 #135 的真实失败路径，基于本人 #421 的 `ff397cc6` 建立，并同步该 PR 的对齐政策夹具修正；先使用独立 `fix/curve-sampling-errors` 分支完成本地回归，再整合至同一 #421，避免重复 CI 队列。没有修改正在执行 #421 完整回归的工作目录，没有审阅或改动他人的 PR。开工前检查了 133 个本地分支的候选文件及相关提交名称；曲线精度工作也涉及这些文件，因此本批不修改传输算法、插值模式、工程格式或像素语义。

## 原实现与功能变化

公开的 `CurveObject` 可以直接构造。真实原实现探针对短 frozen LUT 的 `lut`、`editable`、`graph`，短通道 LUT 的 `lut` 和空自然三次曲线的 `lut`，共复现五次 panic；同一批输入的现有 `validate` 全部返回错误。探针只在验收程序中 catch_unwind，不改变生产 panic 行为。它证明公开 API 边界缺失，**不能证明正常 GUI 或已通过工程验证的输入会触发这些 panic**。

| 项目 | 分类与处理 | 边界 |
|---|---|---|
| 曲线 LUT、可编辑副本、图表 | 功能修复：先执行现有验证并返回模型 Result，保留原验证原因 | 不制造零 LUT、不忽略坏通道、不替换为默认曲线 |
| 时间曲线 | 功能修复：ColorCurveSampling 携带实际帧和箱内原始模型原因，端点与区间访问显式检查 | 既有 NaN 取首键、无穷取端点及插值算式保留；不声称每次采样重新验证整个工程 |
| 渲染与关闭动画 | 原生传播：采样成功后才发布 LUT 索引，关闭动画前先取得真实 LUT | 不改变 GPU 数据格式、预算、像素断言或原有钳制算式 |
| 宿主图表 | 原有覆盖加类型适配：JSON 解析和模型验证本来已存在，graph 的新 Result 原样转入宿主 Model | 不把宿主原有拒绝列为新修复 |
| 桌面图表与打键 | 功能修复：拒绝失败样本与无效打键，图表协议读取失败不再静默丢点；坏拖拽按既有 History(4) 取消一次 | 健康控件、坐标、控件尺寸和已有输入行为保持 |

## 观察规则

新增固定调用点 `effects.curve_samples`、`effect.curve_key_navigation_sample`，使用既有面板观察器。计数是进程生命周期内按调用点累计、饱和递增；首次 Warn，第 60 次 Error，之后抑制，健康邻居不重置。不是连续计数，也不能把 60 次描述成一秒。上下文仅含可用的阶段、通道、对象/效果/参数和帧，不记录完整曲线载荷。模型正常求值没有新增文件 IO 或每帧警告。

新增桌面用例已在实际 Editor.draw 中交替提交坏曲线与健康曲线，并从真实关键帧菱形入口验证采样拒绝不发布编辑、健康输入恢复精确 LUT。实际检查 59 次只有首次文件 Warn，第 60 次文件和 stderr 的 Error，后续抑制、stdout 隔离及健康恢复；还验证失败不替换健康缓存、坏拖拽只取消一次。它是受控的原生绘制和导航入口，不冒充完整 Engine worker、鼠标设备注入或真实窗口故障复现。

## 已执行与待验证

- 原实现五次公开 API panic、验证拒绝及健康对照已实际运行，原始 stdout/stderr、源码与库 SHA 保存在 `artifacts/pr-ci-followups/model-curve-invalid-input-probe-*`。
- 使用原 Cargo 指纹中的七个精确依赖，在独立工件目录以真实 rustc 编译新模型和实际测试程序，没有写入 #421 的 Cargo 缓存。三项模型测试通过，覆盖全部五个通道、多种错误长度、空/重复点、非有限值、静态与关键帧拒绝、不变性和恢复。
- 原库和新库分别执行同一健康探针：六组曲线、十个时间点，LUT 每个分量的浮点位、图表 JSON 和可编辑对象共 770151 字节完全一致，SHA-256 为 `750b4ba8154a57d69f7ee838ea54f8ebc75c9d50155361059cc19a6bb8fcdf57`。模型源码及精确依赖在执行期间不变。只覆盖这些明确输入，不能声称重跑了全部渲染像素基线。
- 首轮辅助程序未兼容 Cargo 指纹目录中的下划线，在编译前退出；修正后模型编译通过，新测试的 frame 参数误用 u32，实际 API 是 i32，保留编译错误后修正。第三轮及补充帧上下文和原生 source 断言后的第四轮，真实编译、同三个测试和健康比较均通过，不重复计数；没有放宽生产返回、预算、期限、忽略或既有断言。
- 以原核心 Cargo 集成测试的精确依赖，在只读复用缓存的独立目录重新编译真实模型、核心和渲染库，再编译执行 effects 九项、color_curves 五项集成测试，合计十四项全部通过。新增场景用例实际验证坏曲线不发布 LUT 索引、不修改工程、修正输入后恢复完全相同的 LUT。输入和依赖字节在执行期间不变。新用例漏传 Scene::sample 的求值参数在执行前修正；辅助程序首轮误解析第三方 build script 为普通库，在编译前退出，原失败保留，后续仅解析实际需要重新编译的三库依赖。没有改变既有断言。
- 首轮真实 Cargo 全工作区所有目标检查发现一处已有正例比较仍直接比较两个新 Result，Error 没有 PartialEq；仅在两个成功采样上显式解包，保留原来的 LUT 相等断言。失败日志与输入不变收据保留，修正后完整全目标检查通过。
- `c92fa4c3` 的完整工作区所有目标检查（FFmpeg、宿主 diagnostics）通过。实际 Cargo 回归：模型 63、核心 267（1 项既有忽略）、渲染 effects/effect_plan_cache 11、完整宿主 diagnostics 120、桌面曲线 7、效果关键帧入口 3、通用关键帧导航 9，合计 **480 项通过、0 失败、1 项既有忽略**；随后带 FFmpeg 的 Debug 产品构建成功。八阶段源码收据均不变，专项与本轮全套不重复计数。旧动画批次的 1904 项不是新颜色曲线的全套，不能相加。
- 两个实际原生子进程在未设置 MOTION_LOG 的默认级别保存原始文件、stdout、stderr。`effects.curve_samples` 记录 phase/channel，`effect.curve_key_navigation_sample` 记录 object/effect/param/frame，均保留 `invalid frozen curve LUT`；每处首次 Warn、第 60 次 Error，文件 Error 共 2 条、stderr Error 共 2 条，stdout 隔离，抑制及健康恢复断言通过。原始工件在 `artifacts/pr-ci-followups/color-curve-default-output-with-runtime/`，二进制 SHA 为 `424001b79ecd53cc3a81529a2a872706c7d9b24a88f556487732b9fa5e14c0d0`。
- 首次独立留样没有继承既有 FFmpeg DLL 路径，进程未启动测试；无环境的 `--list` 也退出 -1073741515（Windows DLL 未找到），加载仓库既有环境后同一二进制 `--list` 为 0。保留两次输出和首次失败目录，随后使用正确环境在新工件目录完成真实留样，没有重试生产请求或改变业务返回。
- 本批 Rust/Cargo 文件已核对与上述回归输入逐字节一致，再整合到 #421；最终远端检查应核对追加后的 head，前一 head 的绿色不自动覆盖新增代码。原生窗口交互和完整实际截图仍未通过，原动画批次 Debug 90 秒超时、Release LLVM 内存耗尽的原始失败继续保留，不能以单元测试或部分 PNG 代替。

## DESIGN §0.7 与 §12

本批不做视觉整改，所有下列既有项均 retained，不能据此宣称设计规范已经全面达成。保持 §5.2 一行一属性、内联控件、现有尺寸 token 与命中区域；错误分支属于功能变化，与样式变更分开说明。真实截图和交互仍待完成。

| 基线项 | 处置与理由 |
|---|---|
| path_controls 六处纵向步长 | retained：不改属性行推进 |
| path_controls 两列比例 | retained：不改列宽及窄面板布局 |
| path_controls 混合按钮高度 | retained：不改按钮尺寸或命中范围 |
| path_controls 自绘数值背景 | retained：不改数值控件 |
| path_controls 全宽布尔按钮 | retained：不改开关 |
| effect_controls 行高及步长 | retained：错误传播不新增属性行 |
| effect_controls 文本布尔按钮 | retained：不改布尔表示 |
| effect_controls 全宽枚举按钮 | retained：不改枚举交互 |
| effect_controls 手绘 hover alpha | retained：不改 hover 与 token |
| effect_controls 层级缩进 | retained：不改树形缩进 |
| timeline 十一列坐标 | retained：不改时间轴布局 |
| timeline 吸附线颜色 | retained：不改吸附反馈 |

Refs #138, #135。这两个合集的其他模型不变量、CPU 文本错误、DecodeTask 和旧协议边界仍开放；本批尚未达到可合并状态。
