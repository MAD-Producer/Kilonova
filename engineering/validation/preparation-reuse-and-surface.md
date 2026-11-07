# 后台准备的管线复用与原生窗口验证

这是 `92ea38df` 阶段的记录；后续曝光入口、最终库回归和完整桌面默认日志
验收见 [运动模糊后台准备](exposure-pipeline-preparation.md)。下面的失败与待办保留当时范围。

继续 #64 / 草稿 #627。开始前重新读取开放 PR：#575 为另一个 agent 的
最终帧缓存/预渲染，#582 为追踪，#626 为深度；本变更只推进效果管线准备。

## 生产行为

- 每个准备任务从当前 Renderer 的实时池及上一有界管线池借出至多 128 个
  原生句柄，WGSL 证明文本合计至多 8 MiB。不保留整个 registry 或资源租约。
  实时证明暂存不可变 CompiledShader 的 Arc，已有退役证明共享 Arc 文本；
  不在 owner 上复制大段 WGSL。需要安装的文本复制发生在准备 worker。
- 只按完整 WGSL、sprite、additive 和目标格式匹配；program 数字编号不作为证明。
  不匹配仍执行原来的严格原生创建及错误作用域。请求安装继续校验 Renderer、
  document generation、revision、取消身份和格式；复用不绕过这些检查。
- 状态分别报告实际新增创建和复用数。复用不增加 native 创建计数或编译时间。
  worker 从实时规划器的 plane-coverage 状态开始，避免暖窗口准备已不再需要的
  初始变体。首次未开启覆盖的路径仍准备实际 preflight 与绘制两个计划。
- 证明文本和句柄数量是保留范围，不是总内存承诺：CompiledShader 同时包含
  GLSL/元数据，驱动的 pipeline 内存、创建中暂存以及 get_data 分配均不由此估算。
  每个在途/排队任务分别受此上限约束，不能将其描述为整个会话只有一个快照。

## 本轮已执行的证据

- `host-full`：完整 Cargo 宿主测试尚未运行即在集成用例编译发生 E0463。
  随后的 `host-metadata-locate` 对原 `panic_boundary` 集成用例 verbose 编译/运行
  通过，未改变源码。原始原因没有确认，保留两份输出，不把复查当作完整回归。
- `host-all-lib`：221 通过、1 失败、0 忽略。失败为 tracking 工程包导出返回
  Windows PermissionDenied；`host-package-access-diagnostic` 对同一原生 EXE、
  同一断言启用回溯独立执行通过。未修改该测试/工程导出行为，原因尚未确认。
- `reuse-native`：324 通过、1 失败、0 忽略。新增容量及变化证明测试通过；
  新增退役复用测试的夹具未实际替换包身份，断言实时池应为空时得到 2。
  内置 registry 共享包 Arc；修正夹具为从真实包字节重新打开，保留全部原断言。
- `reuse-native-corrected`：实际 Cargo 原生测试 1 通过、0 失败、0 忽略。
  实时池及真实退役池均零新增创建、编译时间零，前台 capture 完整像素一致。
  这次窄测试不替代完整 render/host 最终回归。
- `reuse-native-final`：执行上述实际 Cargo 链接的最终原生 render 测试 EXE，
  全部 325 项通过，0 失败、0 忽略，303.53 秒；源码前后保持一致。
- `host-reuse-all-targets`：锁定 Cargo 的宿主/桌面 all-targets 编译检查通过，
  包含 FFmpeg、diagnostics、ai-depth。2457 份源码前后相同；警告涉及的
  34 个文件与基线 `daf806b5` 字节一致，未修改别人的警告代码。
- `host-all-lib-final`：221 通过、2 失败、0 忽略。两项既有 registry 子进程
  分别返回 `Effects(Invalid("out of memory"))` 和 `memory allocation of 81920 bytes failed`。
  初轮 PermissionDenied 用例本轮通过，但其初轮原因仍未确认。完整宿主回归
  仍非绿色，不能用六项准备测试或本轮通过数替代。

上面各命令的原始 stdout、stderr、完整源码前后摘要均保留于
`artifacts/issue-goal-current/native-pipeline-preparation/`，既有缓存阶段证据包不覆盖。

## 原生窗口验证范围

新增真实 Session/surface 测试，受控阻塞生产 worker 的采样边界，检查 Planning
可查询、pending 返回、chrome 呈现、首帧、未来图层无前台新增创建、view 改变时
完整旧画面保留、detach/attach 后像素恢复和单一 worker 身份。
`host-surface-native` 的实际 Cargo 原生链接测试六项通过，0 失败、0 忽略；
源码前后 2457 份一致。受控 panic 为既有故障测试的预期输入，不是逃逸 panic。

窗口测试记录 NVIDIA GeForce RTX 4070 Laptop GPU / Vulkan，执行了 10 次 chrome
回调；首次修订创建五条原生管线，进入后续图层没有新增前台创建。完整工程与
revision 保持一致，view 修订保留完整旧图，detach/attach 后像素一致且 worker
身份不变。`surface-native-evidence/` 包含 adapter、状态 JSON、首帧、后续图层及
重新绑定后的 PNG。画面为 16×16 真实合成输出，这不是桌面面板样式截图。

## 默认日志信道

新增桌面验收用例，使用生产 logger 和真实 Job API，先阻塞采样边界，再提交
空 scene 集合触发真实 SceneCount admission 错误。检查阻塞期间 Planning 可见、
文件已有 Info start；错误原因、请求、generation/revision 和耗时同时到文件与
Error stderr，stdout 没有日志；65 次读取终态不产生重复 Error，新请求真实 GPU
capture 恢复。它不声称覆盖原生非法 WGSL 或 panic hook 的全部错误输出。

`preparation-default-log` 的完整桌面测试程序编译未完成：Rust 映射 libtest 元数据
时返回 E0786 / os error 1455（页面文件太小），随后编译器异常退出；没有测试结果。
同期系统提交内存约 45.8/50.7 GB，另两项宿主子进程也实际分配失败。保留原输出，
不调整系统页面文件、生产内存预算、既有断言或超时。

作为有限范围的补充证据，`compile_sink_acceptance.py` 原样链接生产 logging.rs
和同一份新验收断言，单独执行；仅替换空闲 engine panic-context 读取函数为 None，
不注入/宣称 panic 覆盖，不代替完整桌面编译。首轮入口生成缺少 tempfile extern
及注释位置错误，修正生成器后 `sink-compile-corrected` 原生链接成功。
`preparation-sink-native` 实际运行退出 0：阻塞前 Info 已落文件，一条原始错误
同时在文件和 stderr；child stdout 为 0 字节，后续请求安装并得到不透明 GPU 像素。
源码与已链接依赖的前后身份记录保留。最终完整桌面用例仍待环境允许时执行。

## 审计项与证据包

| 项目 | 类型与处置 | 证据/限制 |
| --- | --- | --- |
| 64-H1 | 功能：有界证明快照、已有管线复用与实时 coverage 状态 | 最终 render 325 项、原生窗口六项；不声明整个 issue 完成 |
| 64-O1 | 新观察：每个 ticket 单独报告实际创建和复用数 | 新 ticket 归零，计数饱和；复用不冒充原生创建 |
| 64-O2 | 既有覆盖：任务 Info start/原始 Error 与生产 file/stderr sink | 独立原生入口通过；空闲 panic-context 替身边界已说明 |
| 64-N1 | 正常分支：缓存未命中、背压、取消、过期任务与放弃回复 | 不作为故障，不重启 worker；既有取消/身份测试保留 |
| 64-B1 | 功能/协议边界：未来失败保留，当前成功；真实进入失败仍报错 | 五项宿主准备用例及新增窗口用例，不清除原始原因 |
| 64-V1 | 待验证：完整桌面及宿主回归、原生 shader 默认信道 | E0786/1455、OOM 与初轮访问拒绝不标成通过 |

新不可覆盖证据包 `pipeline-host-reuse-evidence.zip`：103 个成员，3,728,226 字节，
SHA-256 `53481890bbaa71b99549d3150b86be870071946afc031615030720663ee96669`。
每个成员有长度与 SHA-256 清单；包含原始失败/成功输出、生产/测试源码、
既有基线、窗口像素及日志、运行/链接命令和源码身份。既有 100 项缓存证据包
保持原字节，不将最终通过记录写回旧失败文件。

## 未完成的 issue 要求

仍需完整回归及上述原生访问/元数据/内存异常的归因，实际桌面调用方的完整
默认日志与原生非法 WGSL 错误输出资格、最终可见准备提示、exposure 和
嵌套/时间映射专属近期边界、应用私有目录缓存后台恢复/保存，以及独立进程
cold/warm 与首次/再次添加效果配对计时。DX12/Metal 不支持应用驱动缓存的范围
没有改变。#64 保持开放，#627 保持草稿。
