# UI 绘制错误保留原生原因

关联 #138，基于 #619 接续；同时收录已合入时间编辑 #257 的主线关闭审计。
生产错误传播已实现，实际桌面故障与恢复、完整宿主和全目标检查已完成。
最终远端 CI 独立验证，不把本地专项称为最终完整桌面回归。

## 变更与边界

UI Painter 已有 PaintError、原生 source 和 typed capacity refusal；本批修复 Engine
在截图、attach、atlas 上传/保留请求、覆盖层与独立 UI 绘制处把它转成 String 的路径。
宿主以 NativeSource 保存集成方错误，不新增 Host→UI 的依赖；已知宿主错误仍使用已有
原生变体。NativeSource 保留实际错误对象及其 source，不能从相同文案恢复原生身份。

Related 分别保存主错误和附带错误，只从主错误产生 composition_detail；stderr 和
回复继续有两者的原因，不能由附带的合成错误给普通文字主因伪造结构化身份。
双错误的诊断标签由误称 cleanup 改为 UI overlay，其余单一失败的 Display 文案保持。
这是错误分类修正，不是自动重试、关闭/重启或返回成功。

| ID | 分类 | 验证与限制 |
| --- | --- | --- |
| NP-SOURCE | 新原生传播 | 实际外部提供方、文件 NotFound、OS code、共享所有权/最后释放和相同文字负对照已通过。 |
| NP-PRIMARY | 新原生传播 | Related 的主原因、附带原因与主合成详情分别保留；附带合成原因不得提升文本主因，已通过。 |
| NP-WORKER | 新实际故障验收 | 原生 worker 的 CaptureUi/Present 使用无效 atlas 触发真实 PaintError→IO InvalidInput；截图 Painter 创建触发真实预算拒绝，已通过。 |
| NP-CHANNELS | 沿用既有观察 | 新原生用例检查默认 Info 中操作前 start、Error 文件/stderr、stdout 隔离与无健康 Present start；附带错误的手动 Flight 仅验证回复边界，不冒充另一条真实 worker 操作。 |
| NP-RECOVER | 功能保持 | 健康截图前后 64×64 全绿像素、完整工程/revision、拒绝不发布文件与健康 Present 恢复，已通过。 |
| NP-NORMAL | 正常分支 | Ok(true)、Ok(false)、无附带错误保持；图集保留/重试的既有容量协议与生产预算不改，非容量错误不新增重试。 |
| NP-EXISTING | 既有覆盖保持 | 原绘制容量、预算准入用例的文字预期、默认信道、预算与期限不改；适配处只显式 Display 返回的原生错误。 |
| NP-REMAIN | 待完成 | 字体、样式、configure_capture 自身 String 验证提供方以及 #138 其他边界继续开放；没有实际注入全部 GPU/device/driver 错误。 |

没有依赖、资产、可见控件或排版变化。DESIGN §0.7 视觉整改项全部保留，不声称完成
§12 全局视觉验收。健康 Present 不增加 IO、日志或错误分配；新增包装仅在失败时构造。

## 执行记录与首轮失败

稳定 FFmpeg 桌面测试编译成功，绘制 3 项、原生回复 5 项、查看器 4 项、诊断 15 项
实际通过，共 27 项、无失败或忽略。新增宿主集成用例另有 2 项通过；之后完整宿主
286 项、0 失败、0 忽略，268.907 秒。桌面/宿主全目标的 FFmpeg、diagnostics、
ai-depth 组合检查通过，69.391 秒；该检查是编译检查，不是全部 feature 的运行验收。

首次完整宿主回归在新增 `related_failure_keeps_primary_detail_and_both_native_causes`
用例失败：测试误将 `source()` 向下转换为 `Error`。thiserror 对明确声明的 `Box<Error>`
保留该 Box，与现有 Context/Cleanup 契约一致。只将新增测试的目标类型改为
`Box<Error>`，并新增主合成详情指针身份断言；没有删除或放宽任何预期、预算或期限。
修正后这两项专项及完整回归均通过。首轮退出码 101、原测试字节和完整
输出留在证据的 `initial-host-failure/`，不覆盖成成功。

桌面实际执行与修正后宿主的源清单只相差上述独立 host integration test；全部
产品源码和桌面测试字节一致。执行器逐文件证明差异仅为该类型修正与加强断言，
并核对原桌面可执行文件未变化，因此没有因修改宿主独立测试重复编译或重跑桌面。

验收包：[native-paint-error-causes-evidence.zip](native-paint-error-causes-evidence.zip)，
100 个成员、3,957,757 字节，SHA-256
`78ea5b7abc1f9a7b424782ea70b22045728350eec68d8f977b92910e7693a0fb`。
内含命令、原始输出、首轮失败、2,442 个源文件的身份清单、实际故障文件/stderr/stdout、
恢复 PNG、生产/测试修改前后字节及逐成员 manifest。执行器核对源清单、命令终态、
实际测试名和可执行文件身份；压缩包重读逐成员验证，没有覆盖既有验收归档。

全目标检查仍有仓库已有的 unused/dead_code 警告，保留原始 stderr，不作为零警告
或 clippy 验收；#125 继续开放。

其他分支核对仅查看目标文件的未提交状态，没有审阅或修改他人 PR。六个旧工作树
存在 engine.rs 改动，保留原样；没有发现 NativeSource/绘制原因迁移的活跃分支。
本轮复用自己的已完成工作树，在独立 `fix/native-paint-errors` 分支开发。

时间编辑的已合入能力与最终证据见 [主线关闭审计](layer-time-main-closure.md)。它与
本批错误修复的执行范围分开记录；#257 已关闭，#198 路线图的该条已同步，#138 保持开放。
