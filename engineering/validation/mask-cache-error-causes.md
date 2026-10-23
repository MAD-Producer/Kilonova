# 蒙版三角化错误的原生原因（本地完成，远端待验收）

继续处理 #138、#135。复用本人空闲的 `track-dimension-invariants` 工作目录，创建 `fix/mask-cache-error-causes`，基于已合入的 #434 head `61816ed4`。开工前检查 104 个本地工作目录的目标路径状态与近期提交元数据：mask_plan/vector_mesh 没有其他未提交改动；renderer 存在其他分支的资源工作，因此本批不修改 renderer、effect_plan、资源策略和其他人的 PR。

## 实际边界

| ID | 分类与变化 | 证据或限制 |
| --- | --- | --- |
| M1 | 原生传播：移除生产三角化的 String adapter，MaskCache 返回已有 RenderError，沿 Diagnostic 保留 VectorGeometryError | 真实公开 PlanBuilder 输入 NaN、Infinity、控制点加法溢出；新回归先在旧实现失败，显示原先为 Invalid(String) |
| M2 | 诊断上下文：既有 Diagnostic 带上 layer、mask、mask tessellate、实际 frame | 保留原始原因文字与 GPU validation 前缀，但明确增加上下文；不是完全相同的 Display。没有新增日志、观察键、计数器或健康热路径 IO |
| M3 | 行为保持：顶点运算、预算、缓存插入与淘汰顺序不变 | 原有剩余预算断言不改；公开失败后恢复同一健康几何与原 Arc，输入工程 JSON 和 sampled masks 保持 |
| M4 | 现有信道：真实 planner 失败经 Host/respond/mpsc 返回，默认 Info 文件、Error stderr、stdout 隔离 | 用受控 Flight 验证回复边界；不冒充整个 worker 的开始信号与受控阻塞覆盖 |
| M5 | 正常分支与未完成范围 | 缓存 miss 仍正常；没有重试、重启、catch_unwind 或返回值降级。缓存累计预算的自身验证仍是已有 Invalid 校验，不把它当作丢失的第三方原因；cfg(test) 的旧 String helper 仅供既有几何测试 |
| M6 | 明确未声称 | 三种公开 sampled scene 拒绝不是正常 GUI 可达性证明；共用代码保留 Lyon 错误结构，但没有声称本批复现每一种 Lyon 内部故障；没有完整像素基线重生成 |

## 修改前与工具记录

新增回归在旧产品实现实际执行，0 通过、1 失败；原日志 `mask-cache-before-test.log` SHA-256 `fec4800f7a39c7aed1dace6cfb9e5afff60bead2e1230811e6f74a867bfd3a8a`。独立旧库探针三次返回字符串校验，恢复均正常。

探针覆盖三种 expansion、预览/最终质量及两种输出尺寸，共十二组真实正常蒙版几何。实际新旧库的顶点位置/颜色 bits 逐字节一致，健康行 SHA-256 `6bdf2cf27c74dbbf1a68947600330836409f538d908a3657a657b4e9c89adfb1`；三次失败现在可匹配原生几何枚举，并增加 M2 上下文。工件在 `artifacts/pr-ci-followups/mask-cache-before*-probe/`、`mask-cache-after-v2-probe/` 和 `mask-cache-probe-comparison-verified.json`。

首轮新增测试的检查器漏识别已有的 Box<RenderError> source，实际产品已经保留 VectorGeometry(Invalid)；修正检查器后，原三项几何回归和原一项蒙版预算测试全部通过。保留首轮失败，不改已有断言。独立探针另保留未加载便携 MSVC 导致的链接器缺失，以及两轮收集器对检查函数源码身份的断言失败；实际探针进程均正常退出。最终独立核对确认检查函数仅增加 Box 分支及换行，健康计算与 fixture 不变。工具错误不计为产品缺陷，也不把旧检查器误称为最终相同探针。

最终 Rust/Cargo/WGSL/YAML 输入收据 `mask-cache-errors-v2-source-receipt.json`；所有最终阶段均验证与该输入完全一致。已通过的阶段不重复运行以凑数。几何专项 3、原蒙版预算 1、桌面默认回复信道 5、相关 GPU/渲染 21、完整宿主 120，共 150 项通过、0 失败、0 忽略。全工作区全目标检查和带 FFmpeg 的 Debug 产品构建成功。最终结果由 v2 的前两项和 v4 的后五项组成；v2/v3 的中间探针收集器失败不计为测试通过，`mask-cache-probe-comparison-verified.json` 独立验证实际探针结果。

默认独立留样 `mask-cache-default-output/` 使用实际桌面测试二进制 SHA-256 `b79d04f8ea90e47e0eeaac938c182870cf4bc10e9660862a4261a8394c75f749`，实际文件与 Error stderr 各三条，stdout 没有原因与错误标记；没有重复计为新增三项测试。M4 的受控 Flight 限制保持。

Cargo 产品工件解析确认当前工作目录及 FFmpeg feature；实际新产品 SHA-256 `8b14f0dde50439eb05ee0885ec54f104756e70a0f1b64b477c32f2a3197056b4` 执行 `--command-list` 成功，stdout 为 24475 字节有效 JSON、stderr 为空，产品与源码不变。记录为 `mask-cache-final-product/execution.json`；不是 GUI 像素或视频解码验收。

没有添加依赖、素材或用户可见 UI，不关闭两个合集 issue。远端 CI、实际合入组合与主线发布仍待独立验收。
