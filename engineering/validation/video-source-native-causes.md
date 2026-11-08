# 视频源文件准备的原生 IO 原因

关联 #138，未使用关闭关键字。基于主线 `87a2b295`；修改前检查了其他工作树
的独有提交及未提交改动，未发现 `video_frames.rs` 和本批测试范围被占用。

## 修改与行为边界

解码线程打开源文件前的工程根路径解析、素材路径解析、文件元数据读取，以及
创建 worker 的四处 IO 返回曾先转为 String，丢失 `io::Error`。现在使用已有
`motion_media::Error::Io(Arc<io::Error>)` 转换，保留 ErrorKind、原生 OS 错误码
和可共享的原因身份。没有新增错误类型或更改原有 Display 文本。

没有新增日志、重试、线程重启、超时、状态或 UI。现有 required 解码失败的
Error 记录继续在业务锁外输出，已有 open start 位于路径解析之前。重复轮询
仍返回同一失败；显式新 sequence 的恢复沿用原行为。正常 pending、取消、
supersession、缓存未命中及既有包围路径/文件大小检查保持不变。

## 真实故障与恢复

新增 `video_frames::observability_tests::source_errors` 隔离子进程用例：

- 先写入真实源文件及 PTS 索引，创建真实 Stream worker；分别删除源文件，
  以及将整个工程目录移走，再发出请求。错误由实际文件系统产生，不是 provider
  返回的伪造 IO 错误，也没有耗尽系统线程。
- 两种故障都返回 NotFound 和原生 OS 错误码。恢复文件/目录后，连续 65 次相同
  请求仍失败，并以 Arc 指针证明保留同一原生原因。
- 显式新 sequence 后同一 worker 恢复，PTS、尺寸与全部 16 字节测试像素正确；
  源文件及索引字节不变。像素由已有受控 decoder 产生，不冒充真实 FFmpeg 解码。
- 在 Info 阈值下，已有 open start 先于 Error；每个故障在文件与 stderr 各一次，
  包含 object/source/sequence/PTS/phase/elapsed。stdout 没有日志；恢复和轮询
  不增加 Error。测试安装遵循既有默认路由规则的捕获 logger，不宣称启动了桌面
  产品的 logger；产品路由已有单独覆盖。

负对照只恢复 `video_frames.rs` 的原生产代码，新测试与断言不变，实际在
`native IO cause lost: Message(...)` 失败。finally 恢复修复文件全部字节并核对
SHA-256，再运行完整宿主回归。首轮新测试误用要求成功类型具有 Debug 的
unwrap_err，原始编译失败保留；仅更正新测试写法和既有状态值 failed。

## 处置账本

最终验证：Vulkan 后端的完整宿主 lib（启用 gpu-tests）211 项通过，0 失败、
0 忽略；用例执行 183.12 秒。带 FFmpeg 的宿主与桌面 all-targets 检查通过，
耗时 116.63 秒。两阶段全部源码输入摘要相同且运行前后不变。
新增测试文件 rustfmt 与 diff whitespace 检查通过。保留既有 warnings，
未宣称零警告。专项首轮通过的 1 项包含在 211 项内，不重复计数。

| 项目 | 分类与证据 |
|---|---|
| VS-ROOT | 新增原因保留及验证；真实工程目录移走触发 canonicalize 原生 IO |
| VS-SOURCE | 新增原因保留及验证；真实源文件删除触发 canonicalize 原生 IO |
| VS-METADATA | 新增原因保留；走相同 From 转换，未在 canonicalize 与 metadata 间制造实际文件系统竞态 |
| VS-SPAWN | 新增原因保留；未耗尽线程或注入操作系统 spawn 失败，不宣称实测此故障 |
| VS-REPLAY | 既有失败状态/显式恢复补证据；同原因身份、同线程、原始文件与测试像素 |
| VS-LOG | 既有站点补证据；默认等级、锁外输出、双 Error 信道及 stdout 隔离，无新观察点 |

原始命令、源码摘要、stdout/stderr、负对照与最终默认输出位于
`artifacts/issue-goal-current/video-source-error-causes/`。错误类型合集还包括
其他媒体、模型与宿主边界，本批不据这些用例关闭 #138。
