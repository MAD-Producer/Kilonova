# 粒子历史缓存保留原生错误原因

关联 #138、#135；承接本人已合入的命令目录与 demo 构造 PR #431。本批另含 Windows CI 任务总时限修正，功能和基础设施验证分别记账。没有修改用户可见 UI、渲染算法、资产、依赖、产品预算、产品/测试期限或忽略规则。

## 失败与边界

| 项目 | 处置 | 证据及限制 |
| --- | --- | --- |
| PH1 原生历史捕获错误被字符串拍平 | 本地最终组合通过；远端待验收 | `Scene::sample` 保留 `Arc<motion_model::Error>`；使用生成器时才拒绝，保持原有延迟时机 |
| PH2 克隆和嵌套错误的原始身份 | 本地最终组合通过；远端待验收 | Model 的共享原因包装明确返回原对象作为 `source()`，避免 Arc 转发 source 时跳过叶错误；实际指针身份、受控 IO 与缓存释放检查均通过 |
| PH3 回复及默认日志信道 | 实际本地失败信道通过 | 真实历史采样错误经过生成器、Host 和实际 `respond`；默认 Info 文件与 Error stderr 各保留一次原因，stdout 隔离，恢复不新增故障。手动 Flight 不冒充完整 worker 调度或阻塞前状态发布 |
| PH4 正常输出、延迟时机及恢复 | 真实新旧库对照通过 | 原样探针唯一差异是原生 Missing 可达标记 false→true；健康 sprite bits、完整 Display、延迟拒绝时机和恢复逐字节保持。渲染专项 33 项含真实 GPU，不冒充全套像素验收 |
| PH5 正常未使用错误 | 保留现有边界 | 捕获阶段不新增日志、不提前失败；正常缓存缺失、取消、无输出不归为新故障 |
| CI1 Windows 主线任务 60 分钟超时 | 只修正任务总时限；远端结果待验收 | Windows 与 macOS Intel 使用 90 分钟，其他桌面平台仍为 60；所有步骤、门禁、触发和发布条件保持。两项原生交互验收本来只在 Linux 执行，Windows 的跳过属于正常平台边界 |

## 已留存的原始证据

`artifacts/pr-ci-followups/particle-history-cause-baseline/execution.json` 对真实旧 Model/Render 编译产物执行独立探针。原始输出 579 字节，SHA-256 `138d100c4dd4417ac8e7d15dd62ae6c9024e8be09a50028c9b6186a32bbea697`。受控草稿把源图层设为缺失的 999，实际 sampler 返回成功并缓存失败；随后生成器拒绝，原生 Missing 身份不可达。健康 sprite bits、Display、生成失败前缀保持与恢复均已留样。受控草稿不代表工程验证接受无效源图层。

`arc-source-semantics.rs` 的独立标准库探针证明 Arc 本身可被 downcast，但其无内层 source 的叶错误不会自动成为 source。首次未加载便携 MSVC 环境导致 link.exe 缺失；加载既有环境后成功，不归为产品缺陷。原始失败不删除、不改写。

最终新库探针输出 578 字节，SHA-256 `9558033241b71e49fb0628217d04d7c0a6de2752249afd5466b6c1bc7be85de2`；探针源码 SHA-256 `e2c8929f8b4e13f14ee4d9c904624932c9237c7869de893260be680b2b7a2b96` 与旧探针相同。编译和运行均退出 0，stderr 为空；其余输出与原始留样相同。`particle-history-causes-final-product/execution.json` 核对实际 Model/Render 库及 FFmpeg 产品 SHA，独立探针不冒称 GUI 产品交互验收。首次收集器因 Windows 路径分隔符取库失败而提前停止，修正仅限收集器路径规范化，原输出保留，未修改探针或预期结果。

## 本地最终组合

`particle-history-causes-results.json` 六阶段均退出 0，原始输入收据为 `particle-history-causes-source-receipt.json`，阶段日志含 SHA-256；不累加父 PR 或其他源码阶段的数字。

| 阶段 | 实际结果 |
| --- | --- |
| 渲染生成器 / 粒子 / 场景 | 33 通过，0 失败，0 忽略 |
| 模型 / 核心 | 334 通过，0 失败，1 项既有人工测量忽略 |
| 宿主 | 120 通过，0 失败，0 忽略 |
| 桌面生成器设置错误信道 | 2 通过，0 失败，0 忽略 |
| 全工作区 / 全目标 FFmpeg 与 diagnostics 检查 | 成功 |
| 带 FFmpeg 的实际桌面产品构建 | 成功 |

合计 489 通过，0 失败，1 项既有忽略。渲染原日志 SHA-256 `66bf6fd687d9297e55a40e940830506fb23153da54be3980dc0e86238a35a7bd`，实际 162.125 秒；既有编译 warning 保留，不声称零 warning。IO 原因是受控非 Clone 缓存输入，不声称历史捕获实际访问文件。

## Windows CI 总时限

已合入 #426 的主线 [run 39881600784 / Windows job 120179991691](https://github.com/MAD-Producer/Kilonova/actions/runs/39881600784/job/120179991691) 终态为 cancelled。检查注解明确报告任务超过 1 小时，不能因原日志没有 `##[error]` 就认定正常取消。单元测试约 31 分 50 秒、FFmpeg Release 编译约 15 分 57 秒，归档和解压后解码成功；gallery 编译及收尾使整个任务触及总时限。FFmpeg 缓存准备仅 11 秒，不归因于冷下载。

`windows-desktop-wall-timeout-evidence.json` 保存实际注解、步骤与源码，确认两个原生窗口/UI 验证步骤具有 Linux 条件，Windows 跳过不归因于超时。修正只把 Windows 整个任务时限从 60 改为 90 分钟，保留 macOS Intel 90、Linux/macOS ARM 60。没有扩大产品执行期限、降低断言、添加重试或跳过测试。

这项 YAML 修正在六阶段完成之后单独实施。`particle-history-windows-ci-only-delta.json` 保留原始工作流 SHA、修正后 SHA 和唯一表达式替换，不改写原六阶段收据。其余 Rust/Cargo/WGSL/工具链输入保持原样。现有 workflow 测试 11 项通过，独立检查 PR/push/手动触发的全部矩阵平台，以及原表达式在 Windows 返回 60 的负面对照；整个工作流结构除这一表达式外完全一致。原日志 SHA-256 `824a5fcdf7f6a9ce5d1da30db9b96157c3f628cc9c18e4d3345ff9b01766e785`，收据为 `particle-history-windows-ci-phase-verified.json`。

系统 Python 首次缺少已安装的 PyYAML 搜索路径；复用既有固定版本环境后，实际测试成功。收集器在写出该成功原始收据后，因把 Windows CRLF 当成 LF 判断结束标记而失败；后续只用 `splitlines()` 核对原日志，没有重跑测试、改写原收据或更改产品/测试预期。远端 CI、实际合入树和新主线 Windows 完整运行仍待各自结果；两个合集 issue 保持开放。
