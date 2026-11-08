# 工程色彩契约、资源身份和准备边界

关联 #83；接续 `feat/color-image-transform`。这是工程和资源接入阶段，
不是完整的桌面 OCIO/ACES 功能交付，不能据此关闭 #83。

## 变更与原有行为

- 工程格式 38，保留版本字段，只接受当前版本。新增必填
  `color_management`，新工程显式选择 `unmanaged`，维持现有工作流。
- `ocio` 保存配置目录、内容 hash、上下文、ACES 版本及角色/空间/
  display/view/输出选择。`declared_key` / `resolve_declared` 仅校验声明，
  只有读取实际字节后的 `VerifiedColorConfig` 才表示内容已验证。
- 信号声明保存 OCIO encoding 类别、RGB xy、白点 xy、范围、scene/display
  reference 和 alpha；配置空间绑定要求 encoding/reference 与目录一致。
  RGB 允许 ACES AP0 一类的虚拟原色，拒绝非有限值、退化原色和非法范围。
- authored/default input 与逐素材覆盖分别保存；图像/视频素材的显式数据
  解释不能夹带 color meaning。数据通道的声明不是自动色彩转换。
- `SetColorManagement` 使用既有原子命令、组合成路由和撤销；配置经 Arc
  分享给历史/合成取样视图。历史按原有紧凑 JSON 预算计入该字段，更新时
  单独刷新其大小，普通图层编辑复用缓存。
- `assets/color/<配置身份>/` 存原始配置和闭合集的外部资源，工程 JSON
  不内嵌文件内容。拒绝大小写碰撞、Windows 保留名称和文件/目录别名。
  load 校验大小和 SHA-256；包清单带入这些文件并在复制时再次校验 hash。
- 宿主编辑准备、Scene 创建和取样现在明确拒绝尚未安装完整 OCIO runtime
  的 managed 工程。该功能边界避免将 ACES 数值默认为已有 linear/sRGB
  标签；不会启动自动重试、替代配置或后台重启。

OCIO encoding 是空间分类，不能替代完整变换或从中推断原色/白点。
display/view 可能跨 scene/display reference，相关声明必须分别保留。
依据：[OCIO 色彩空间](https://opencolorio.readthedocs.io/en/v2.6.0/guides/authoring/colorspaces.html)、
[Displays & Views](https://opencolorio.readthedocs.io/en/latest/guides/authoring/displays_views.html)。
原色/白点声明与 OCIO 变换数学的一致性仍需后续原生准备器验证，当前目录
校验不会冒充这一能力。

## 报错与观察核对

| 项 | 分类 | 当前处置与验收 |
|---|---|---|
| C1 配置角色/hash/ACES 缺失或不匹配 | 新功能错误返回 | 保留原生 `ColorConfigError`，来源绑定另附 asset；核心类型与回滚测试通过 |
| C2 文件缺失/读取失败 | 新 IO 边界 | `ProjectColorError::Io` 保留 path/phase 与原生 io::Error；真实文件缺失及原生 OS 原因测试通过 |
| C3 同大小篡改/打包期间改变 | 新内容验证 | 配置/LUT 同大小篡改被拒绝；快照后修改内容不发布新包；真实文件测试及移除 hash 校验的负对照通过 |
| C4 runtime 未准备 | 显式功能边界 | 编辑准备失败，保留工程、revision、history；宿主与真实 worker 测试、移除 preflight 的负对照通过 |
| C5 worker 开始/错误日志 | 复用既有观察 | 默认 INFO 的真实串行 worker 验收通过：四次失败均开始先于错误、Error 文件+stderr、stdout 隔离，随后普通编辑正常 |
| C6 崩溃/不返回 | 复用既有边界 | 不新增 worker、锁、超时或 panic 捕获；原生 IO 仍受既有 in-flight 观察；验收子进程另设 20 秒看门狗 |
| C7 健康 Present | 热路径 | 仅模式判断，无文件 IO、逐帧日志、计数器或自动重试 |

独立编写的测试 catalogue/content 夹具只验证声明与字节身份，不宣称是可执行
OCIO 配置；原生 OCIO/GPU 固定参考仍由上一阶段的 20 例 fixture 验证。

## 验收记录

首轮 `cargo check --offline -p motion-model -p motion-core -p motion-render
-p motion-host --all-targets` 通过，原有渲染 dead-code 警告保留。
核心库及工程格式、历史、组合成、包事务、发布、精度和新色彩契约合计
89 项测试通过。三项负对照分别移除内容 hash 验证、漏计历史中的色彩
配置、跳过宿主 preflight，均触发原测试失败，未改动测试断言。恢复源码
后新工作流的 11 项核心和 2 项宿主测试通过。

首轮新测试有两个夹具准备错误：context 值超过已有 256 字节限制，以及
试图在文件缺失时调用本来就校验资源的 save。修正为合规长度、成功保存
后显式删除测试文件；原断言保持不变，失败原始记录保留。
后续模型/效果资格测试曾在编译中断开，原始输出和无完成记录的目录
保留为 `model-effects-interrupted`，不计作通过。确认相关进程消失后只
补跑缺少完成结果的项目，没有重启或重复已完成的验收。

模型与效果的 501 项测试全部通过。Vulkan、DX12 各运行 3 项图片处理
测试，分别覆盖 20 个真实 GPU/CPU 对照案例；最大容差占比分别为
0.10252412、0.16935287，均小于 1。真实串行 worker 子进程的 1 项
默认日志信道测试通过，带 FFmpeg 的 render/host/desktop 全目标检查通过。
计入上述核心回归、2 项宿主测试和单后端 3 项 GPU 测试，去除重复执行
后共 596 项通过；DX12 是同一组 GPU 测试的额外后端验证。新色彩文件
格式检查通过。最终证据包还逐文件核对验收时源码、暂存区交付字节和
归档成员 SHA-256；仅允许 Git 的 CRLF→LF 规范化。

本地原始记录位于 `artifacts/issue-goal-current/color-project-workflow/`，
`verification.json` 汇总结果，`archive.json` 记录证据包成员身份。
这些检查不替代完整输入→工作空间→显示/输出链的验收。

## 后续完整目标

尚需实际宿主准备器、逐源输入转换、工作/知觉节点空间与 image contract
身份贯通、预览 display 和导出 output 接入、高位深解码、Float16 与 OS
显示链分别验收。目前不提供 UI 开关，不改变 UI、图像基线或渲染数学。
DESIGN.md §0.7 的面板整改条目保持原状，本阶段未改动面板代码。
