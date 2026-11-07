# 曝光预览的原生错误与终止请求身份（#138）

基于 main `adf52931`。开始前核对实际工作树和开放 PR 的文件：#575 正在推进
预渲染、最终帧缓存和 viewer.rs，#582 正在推进跟踪，深度任务有独立工作树。
本批不编辑这些调度实现；没有把 #633 的重复缓存基础重新带入。

## 修正

曝光预览先前在场景初始化和失败请求缓存中把 RenderError / 模型错误变成
String，下一次 Present 只能重建 Invalid/Resource。现在失败请求保留一个
Arc<RenderError>，首个回复和随后轮询共享原始错误；上下文用现有 Diagnostic
包装，资源拒绝仍可通过 resource_rejection() 提取，结构化合成错误继续通过
宿主 composition_detail() 传递。请求替换、取消或显式丢弃会释放该引用。

状态 JSON 仍只保留原先最多 2048 字符的展示原因；原始错误对象不会随之截断。
没有按轮询积累错误列表、重新执行失败操作、扩张预算、改变内存降级策略或
新增 worker。既有 pending、取消、缓存 miss 和健康 Present 不增加日志。

近期嵌套边界的帧率转换保留 RateError::NumericRange 的模型变体，继续使用
原有诊断文字。损坏 PNG 的实际测试还暴露了 source() 跳过原生叶错误的问题：
Png 变体改为显式 source，保留同一个 png::DecodingError 和原有 Display。
不改变解码、图片像素、工程格式、包、依赖或可见 UI。

## 审计处置

| ID | 分类 | 证据与边界 |
|---|---|---|
| EXP-INIT | 功能修正 | 实际场景初始化拒绝越界时间；65 次终止轮询保留同一 Arc、合成详情、请求及 CPU 成本，新请求正常产出非空像素 |
| EXP-PNG | 功能修正 | 实际 PNG IDAT CRC 损坏，保留解码器变体和 source；65 次轮询保持原始身份，不修改工程或重放任务 |
| EXP-RATE | 功能修正 | 实际 FrameRate::map_frame 浮点溢出保留原生 RateError，文字与旧诊断相同；下一次合法转换正常执行 |
| EXP-BUDGET | 既有覆盖 | 原资源拒绝、显式内存策略处置、保留 front、receiver 像素 oracle、请求替换和命令身份测试原断言保留 |
| EXP-CHANNEL | 既有观察验证 | 新原生窗口用例通过真实 Engine / Session / Present、生产默认日志和回复；损坏 PNG 连续两次回复须保留同一原生原因，Error 文件与 stderr 一致且不进入 stdout；明确移除坏图层后呈现纯绿并捕获全像素 |
| EXP-NORMAL | 正常分支 | 沿用待准备、未完成采样和显式替换协议，不报警、不重试、不重启 |
| EXP-REMAINING | 待完成 | 视频准备/上传和先前请求取消仍存在文本提供者；清理错误的独立原生身份不由本批解决。#138 的其他提供者、不变量及畸形输入整体验收仍开放 |

本批没有新日志点或耗时阈值，复用 Engine 的默认错误信道和已有请求上下文。
Present 不要求 Info start；非 Present 的开始和飞行状态沿用原实现。本批不把
成功回退或普通取消当成故障，也不把编译通过当成错误信道验收。

## 验证记录

原始命令、退出码、stdout/stderr 和运行前后全部 Rust/TOML/WGSL 源码 SHA-256
位于 `E:/Dev/aem/artifacts/issue-goal-current/exposure-error-causes`。
最终测试范围与去重计数由该目录的验证清单记录；远端 CI 单独跟踪。

- `sampling-v1`：新帧率测试夹具忘记声明可变 Renderer，编译拒绝，尚未执行测试。
- `sampling-v2`：6 通过、1 失败，新 PNG 断言误以为 ImageResource 紧跟最外层；
  实际还经过既有任务 Diagnostic 和共享图片缓存包装。修正新夹具的已知包装遍历，
  保留具体 Png 类型、source、原始身份和不重放断言，不放宽既有测试。
- `sampling-v3`：6 通过、1 失败，实际 Png 叶错误的 source 为 None；保留断言，
  修正生产 Png 的原生来源声明。
- `sampling-v4`：7 通过、0 失败、0 忽略；`lookahead-v1`：1 通过、0 失败、0 忽略。
- `desktop-native-v1`：原生错误身份与重复回复检查已通过，但新夹具直接保存尚未
  创建的 viewer snapshot，正常得到 source 不可用。按既有 API 先捕获 snapshot
  再保存，并将 attach 后的实际 view=1 / surface=1 作为上下文检查；没有改产品
  或删除恢复像素断言，原失败的默认日志和退出记录保留。
- `desktop-native-v2` / `desktop-native-dx12`：Vulkan 和 DX12 各实际通过该窗口
  用例，默认文件/Error stderr/stdout、重复错误身份、编辑恢复和纯绿全像素均通过；
  同一个测试不按两个不同测试计数。
- `host-lib-v1`：完整宿主库 210 项通过、0 失败、0 忽略。

完整渲染器库、图片集成、全目标编译及文本缓存退化负对照使用独立记录，结果与
去重计数以 `verified.json` 为准。未在这里把阶段性成功冒充全工作区或跨平台
验收；不改变旧断言、期限、预算或忽略规则。

没有编辑面板和 token。DESIGN.md §0.7 各项既有呈现问题保留，原因是本批只改
错误对象与验证，不把这些保留项描述为完成。
