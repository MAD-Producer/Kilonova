# #83：明确配置下的 OCIO GPU 处理器准备

## 交付边界

本批继续完整 #83，不关闭 issue。上游为配置身份 PR #644；LUT PR #643
已通过八项必需检查并合入 main。根工作树 UI、深度安装/导出、跟踪和预渲染
各有其他工作树占用，本批没有修改这些实现或评审其 PR。

新增外部 `export_processor.py` 及独立 Rust `color_processor` 编译层，明确绑定
配置、完整选择策略、OCIO 2.6.0、阶段与两端透明度。官方生成的 GLSL、数值表
及三个明确 ACES 配置的测试序列化有归属记录；不添加产品 OCIO/NumPy 运行时。
官方 Naga 30.0.1 增加 GLSL 输入和 WGSL 输出特性，仅新增 pp-rs/unicode-xid
两项锁定间接依赖；没有上游源码补丁。

解析运算保留，2D 查找表改为显式最近邻/双线性数值 buffer。固定内核保留
输入 alpha，按明确选项反关联/关联。通过 Naga 公开 IR 添加输入和输出长度
守卫，再正常校验/输出 WGSL。编译后的身份覆盖原始程序、配置/策略/阶段、
透明度、精确表值位模式、适配器版本和实际 WGSL。

接口信任外部 OCIO 准备器的数学语义，验证字节/选择身份和固定资源接口；
这不是任意 shader 的语义认证或签名。配置和处理器不能从环境隐式替代。

## 实际证据与首次失败

证据目录：`E:/Dev/aem/artifacts/issue-goal-current/color-gpu-processor`。

- 首轮原生探测缺 NumPy；隔离使用官方固定 2.3.5 wheel，并核对 PyPI SHA-256。
  外部包未进入 Rust 产品，也未提交环境目录。只读配置摘要，不输出环境变量。
- 独立最小 Naga 探针仅开 glsl-in/wgsl-out 时遇到 upstream 特性组合编译错误；
  加上产品原有 wgsl-in 后通过，没有修改 registry 或供应商源码。
- 初始 writeonly storage 声明不被 Naga 支持；包装输出改为 read/write。
- OCIO 的 `outColor.rgb.r` 写入在 Naga GLSL 产生 InvalidStorePointer；只展开
  等价、完整标识符边界内的 RGB 分量，保留原始 GLSL 与数值参考。
- 自写 GLSL 包装的 runtime-array `.length()` 产生 InvalidPointerType。保留
  范围检查目标，改用公开 IR 构造真实 ArrayLength，不删除守卫或放宽断言。
- 新增 GPU 非中心采样测试的 `.5` 写法曾被 rustfmt 拒绝。这是新测试语法错误，
  不作为有效负对照，也不计作产品回归；修正为合法字面量后另验。

初始探针在 RTX 4070 Laptop 的 Vulkan/DX12 上分别执行三个显示变换，各
12 个像素；此探针使用硬件表过滤，只用于验证原始路线，不替代本批正式的
数值 buffer 实现验收。正式夹具含 3 个配置、20 个阶段/alpha 组合、240 个
像素；CPU 预期由真实 OCIO 处理器生成。色卡含负值、超 1 高亮、零和小 alpha。
另以独立 2×2 表验证非中心坐标与两种插值，避免只在表项中心证明插值正确。

容差在 GPU 测量前固定：Float32 绝对 0.0001 + 相对 0.0002。高亮编码输入
256 经输入变换会得到很大的线性值，绝对误差不能脱离对应量级报告；日志同时
记录相对于指定容差的比例。输入 alpha 按 Float32 位模式精确比较，超量 dispatch
必须保持两个额外输出哨兵不变。没有改既有测试断言、图像基线或容差来通过验收。

最终冻结检查、负对照、恢复及编译结果在本文件末尾列出。

## 报错与功能边界记录

| 项目 | 类型与处理 | 覆盖及限制 |
|---|---|---|
| CP-01 | 新增配置/策略/运行时不符、过量内容、非法表值的 typed Err | Rust 原生拒绝与失败后重编译；外部工具亦拒绝失配和缺角色/视图，不隐式替代 |
| CP-02 | 新增 JSON/Naga parse/validation/write 原始 source 链 | 实际 JSON、GLSL parse 及 compute 非法导数验证失败；外层诊断有界，不附完整源码；writer 故障路径尚无独立注入 |
| CP-03 | 外部准备器异常及独占写失败 | 原生 CLI 正常生成、重复目标保留原始 FileExistsError、stderr 有原因且失败 stdout 为空；正常摘要不是应用日志 |
| CP-04 | 功能限制：3D 纹理、动态 uniform、外部资源/上下文 | 明确拒绝，无替代或重启；尚未逐项实测所有原生构造，不能宣称一般用户 OCIO 配置已支持 |
| CP-05 | 正常路径：无查找表、无变化的显式配置 | 不记故障，不增加 Present 日志或文件 IO |
| CP-06 | 待接线的业务边界 | 尚无宿主命令/worker、图片纹理准入、工程设置或 UI；因此未声称新增应用日志/错误信道验收，也未声称阻塞准备操作已挂接 start/in-flight |

仍需完成实际工程输入→工作→显示/输出接线、资源预算和在途保留、准备命令及
默认日志/错误信道，分别验收高位深解码、存储精度和 OS 显示链。只有当前已
实测的浮点内核资格；没有 HDR 显示、移动端、Metal 或全体配置的资格声明。

## 冻结验收结果

- `final-effects`：135 项不同 Rust 测试，0 失败/忽略；产品源码与最终版本一致。
  唯一后续差异是本批新增 GPU 测试的 `.5` 语法修复及 rustfmt，原文保存在
  `render-test-before-syntax-fix.rs`，`verify.py` 逐字节重放这项变化。
- `fixed-vulkan`、`fixed-dx12`：每端 2 项原生 GPU 测试通过，同一组测试不重复
  计数，因此共 137 项不同 Rust 测试。20 个 OCIO 参考组合每端均通过，最大
  容差占用分别约 0.103、0.170；另验两种插值的非中心坐标和边界。
- `final-tools`：原工具全集 122 通过、1 项既有条件跳过（没有设置可选
  MOTION_EFFECT_VALIDATOR）。与 effects 同样只早于新增 GPU 测试的语法修复。
- `final-export`/`external-frozen`：7 项真实外部验收通过，含全部 20 个准备
  结果与冻结参考相等、环境显示覆盖隔离、失配/缺失/数据空间拒绝、改动正文、
  运行时、重复/超量 JSON、实际 `-O` CLI 成功和独占写失败。
- `ffmpeg-check`：`cargo check --locked -p motion-render -p motion-host
  -p motion-desktop --all-targets --features motion-desktop/ffmpeg` 通过；使用
  已有固定共享 FFmpeg，原有警告保留。
- 四项负对照分别只修改 `color_processor.rs`，测试过程冻结其他源码且原断言
  实际失败，退出码均为 101：`missing-table-key`、`missing-output-premultiply`、
  `linear-becomes-nearest`、`missing-dispatch-guard`。负对照后源 SHA-256 恢复为
  `5e4d0a2ab43a5c394e85e3d0c32d00bd3863dbc24afce95f8ea80995b86ee2bc`。
  语法编译失败不计入四项负对照。
- `recovered-effects` 和 `recovered-native` 为提交前恢复检查；交付验证器要求
  分别 6 项效果测试、2 项 DX12 原生 GPU 测试通过，且冻结源码和提交内容一致，
  再生成含文件 SHA-256 的本地验收包。

这组验收增加库级能力与测试，不改现有应用渲染语义、工程格式或图片基线；
本批没有用户可见 UI 改动。
