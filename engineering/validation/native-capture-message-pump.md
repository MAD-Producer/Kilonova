# Windows 原生截图测试的消息处理

关联 #135，随 #402 修正阻塞该 PR 的测试驱动边界。没有改用户看到的 UI、生产呈现、GPU 预算、依赖审计要求或任何既有断言。

## NCM-01：保留实际失败

#402 原 head `6653fe5f` 的 run `39614319284`，Windows job `119271159870`：桌面测试 726 通过、2 失败、1 项既有忽略。原生采样在 baseline、frame 109.83501899999999 的 Present request 11 等待超时，在途 10951ms、队列深度零、观察锁可读；路径节点原生验收另在 capture_ui 同步调用处超时。依赖审计、Rust/GPU、Linux 与 macOS 均通过。这些结果不能把 Windows 两项失败抵消或解释为依赖审计失败。

原始 job 日志保存在开发工作区 `artifacts/dependency-audit/ci/windows-job.log`；Actions 实际上传的 artifact `12353735592` 下载原 ZIP 及解包文件均保留在 ci/windows-failure。现场只有原生采样的上层在途状态和调用线程 panic 栈，没有阻塞 worker 的原生栈；因此不能据此认定采样 Present 超时的驱动或 GPU 根因。空 stderr 文件也不能代表完整产品双信道验收已通过。

## NCM-02：测试驱动的功能修正

路径节点验收在窗口所属线程用 recv_timeout 等待 attach／Present，以及同步 capture_ui，期间没有处理窗口消息。已有 native_test_support::receive 用真实 Windows SendMessageTimeoutW 证明：跨线程消息需要窗口所属线程处理，纯接收等待会超时，处理原生事件后成功，并保留 Timeout／Disconnected 边界。

本次将同一个 EventLoop 传到截图和钢笔模式验收，挂接／Present 复用该 helper；截图通过现有 Engine request 提交原有 CaptureUi 命令，继续处理消息直到原回复。挂接 30 秒、Present 10 秒、截图 60 秒与原调用期限完全一致，20 秒呈现完成期限也不变。所有像素、工程、节点、撤销和输入断言保留；不重试失败、不重启 worker、不增加忽略，不把超时改成成功或默认图像。

本机最终路径原生验收 1 通过、0 失败、0 忽略，25.86 秒（编译另 46.08 秒）。这是修正后的正常分支验证，未在本机复现远端这两次超时；对采样测试本身没有任何修改。该补充不能冒充采样 Present 根因已修复，也不能在新提交 CI 返回前合并 #402。

## NCM-03：可观测性与剩余范围

仅修改测试驱动，沿用原有 logger、panic hook、请求及错误结果；没有新生产错误结构、观察键或计数器，不声称新增默认日志双信道证明。同步 #400 前的完整桌面回归为 731 通过、0 失败、1 项既有物理设备忽略，执行 510.19 秒，含编译 511.91 秒；source receipt 核对全部 Rust／Cargo 源文件未变，原采样、路径截图及真实跨线程消息测试均通过。证据为 native-capture-full-desktop.log／-result.json／-source-receipt.json。

期间 #400 合入主线 d60aaea2；上述全套结果不能冒充同步后的最终树已验证。同步后的相关路径复验与新提交远端 CI 待结果。采样 Present 原生阻塞原因仍在 #135 跟踪；若再次出现，继续保留失败而不放宽期限。

没有新增依赖、素材、格式或渲染语义。DESIGN §0.7 十二项 retained，逐项理由沿用 host-error-contract.md；没有面板或 token 改动。
