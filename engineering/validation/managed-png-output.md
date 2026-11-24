# 冻结工程的受管 PNG 输出

本批继续 #83，为现有全分辨率 `FrameFreezePlan` 接入工程的
`WorkingToOutput` 程序。显示截图仍使用独立的显示程序；两种 PNG
不可互相冒充。未修改面板、布局或样式，DESIGN §0.7 的面板项保持原状。

导出计划在 Session 线程捕获模型、帧、来源句柄及颜色配置/表格句柄。
独立导出线程复制冻结来源，验证原生运行时和工程声明，准备原文件
Float32 图片，完成工作合成及输出转换，最后才回读。工作合成和输出
转换各自提交 GPU 命令，中间没有 CPU 像素往返。录制/电影输出仍未
获得这条 PNG 路线的资格，不能借此启用旧颜色采样。

PNG 存储边界显式采用 RGBA8、full range、straight alpha，以及逐分量
clip/round。输出程序若产生预乘结果，写文件前解除预乘；零 Alpha
的 RGB 清零。浮点颜色、Alpha、读取契约和像素长度均核验，不能静默
截掉残余字节或把错误输出写成成功文件。

文件仅接受能够明确表达的信号：线性输出写入 gAMA=1 和声明的 cHRM；
标准 Rec.709 原色、D65 白点的 sRGB 输出写入 sRGB chunk。任意配置
传递函数、limited range 或无法表示的色度必须显式拒绝，不能虚标
sRGB。iTXt `MotionStudioColor` 保存配置和输出程序身份、选择及完整
信号声明；API 回复携带同一描述。该元数据是出处声明，不是 ICC 配置。

`png` 0.18.1 原已作为间接依赖锁定，本批显式使用其标准元数据编码器，
没有升级版本或修改上游；作者、MIT 许可、精确版本及提交已记录在
ATTRIBUTION / NOTICE / PROVENANCE。

| 编号 | 类别及处置 | 证据与限制 |
| --- | --- | --- |
| MPO-01 | 新增消费者 | 原生输出程序直接绑定类型化 Float32 工作及输出目标，最终量化之前不经过 SDR 旧接口；显示选择不替代输出选择 |
| MPO-02 | 冻结及保护 | 配置及引用表与图像共享原有句柄、元数据验证和冻结机制，加入来源路径保护；捕获后原路径重命名并写入无效替代文件，再关闭 Session，导出仍用原字节 |
| MPO-03 | 实际像素资格 | 三份固定配置 × 线性/sRGB 输出 × straight/premultiplied 输出程序，共十二次导出；背景、合法创作颜色、16-bit PNG 源，共 13824 个内部像素对照独立原生 CPU，容差保持一个 SDR8 码值 |
| MPO-04 | 实际文件资格 | PNG 解码逐项核验输出 Alpha、gAMA/cHRM 或 sRGB，以及 iTXt 与回复身份一致；不拿配置目录标签替代数学比较。配置路径覆盖拒绝、未知文件信号拒绝与取消均保留已有完整文件 |
| MPO-05 | 既有观察复用 | 实际桌面冻结/导出 worker 复用 Info 开始、在途状态和原有回复；缺配置及不支持的输出声明在默认文件与 Error stderr 保留原始原因，stdout 无业务日志；没有新增逐帧 IO 或重试 |
| MPO-06 | 阻塞与取消 | 受控阻塞真实原生 provider，先检查独立 PNG flight 与对应 async start，再释放完成导出；取消标志共享到图片解码，取消回复沿原协议且不记 Error。配置准备、原生调用及 GPU 驱动本身不能被强制中断，边界仍检查取消；不引入重启线程 |
| MPO-07 | 原因与发布 | 仅将类型化图像 Cancelled 映射为既有取消回复，不让并发取消覆盖其他首要错误；编码、sync、回读验证及最终发布门继续沿原路径执行，失败不覆盖目标 |
| MPO-08 | CI 门禁 | 原生 PNG 像素和桌面默认信道验证加入所有桌面平台的原生 OCIO 必需步骤，位于打包前，无 continue-on-error；工作流测试固定检查两个命令及显式运行时 |
| MPO-09 | 保留与待完成 | 原有 PNG、冻结资源及取消/发布测试保持原断言；#83 仍未完成，其他消费者、输出格式及完整跨平台资格继续待办，不能关闭 issue |

原始证据位于 `artifacts/issue-goal-current/color-managed-output/`。
`output-compile-first.txt` 保存原有八项 PNG 检查；`output-native-first.txt`
保留新增 fixture 使用错误枚举名的编译失败，第二轮保留非法 RGBA
fixture 被模型正确拒绝，改为合法创作颜色后第三轮保留元数据读取字段
错误。png 0.18 的解码器把文件块存入 `gama_chunk/chrm_chunk`，
`source_*` 是编码输入，不能用于读回断言；修正新测试的实际读取字段，
精确期望不变，第四轮两项通过。没有修改既有 golden、像素容差或旧断言。

`output-worker-first.txt` 和 `compiler-crash.json` 保存 rustc 自身的
STATUS_ILLEGAL_INSTRUCTION，发生在测试运行前，故障模块为 rustc_driver；
未得根因，不宣称修复。相同工具链后续构建成功，`output-worker-recompile.txt`
验证实际失败/恢复/阻塞路径。新增 CI 测试第一轮还检出工作流多余缩进，
修正工作流原文后五项打包门禁通过；未放宽测试去接受错误的命令。

最终 `output-worker-cancellation-final.txt` 验证原生阻塞期间取消，保留完整
文件和工程，正常取消无 Error；`output-media-final.txt` 的十八项包含原有
PNG、冻结来源、持久化和观察回归；`output-ffmpeg-cancellation-final.txt`
另跑六项真实音视频取消、编码后取消、并发发布和预算回归，其中一项
观察测试重复，不重复累计。合计二十四项不同 Rust 测试通过，五项 CI
打包工具测试通过，工具库存七十项，来源门禁覆盖 6420 个跟踪/暂存路径，
差异检查通过。既有编译警告、Epic Vulkan overlay 的失效外部注册及
libopenh264 警告保留，不宣称解决这些独立问题。

绿色旧提交不能替代本批提交的远端结果。
