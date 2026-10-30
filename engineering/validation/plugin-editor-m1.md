# 插件编辑器 M1：作者工程、绑定与逻辑编译验收

日期：2026-10-29。分支：`feat/plugin-editor`，基于属性面板 PR #557。
本轮只实现作者数据层与无窗口校验，不包含可见面板改动。
后续施工仍按 [插件编辑器方案](../host-api/plugin-editor.md) 的 M2–M5 推进。
接口、预算与可运行示例见 [作者工程契约](../host-api/plugin-editor-source.md)。

## 已交付与实测

| 项目 | 结论与证据 | 范围限制 |
| --- | --- | --- |
| PE-01 作者工程 | `.msplugin` 保存/重开真实图、内嵌 PNG 与 WGSL；当前版本字段必填、未知字段拒绝；[13 项工程检查](plugin-editor-m1/project-tests.txt) 通过 | 没有旧格式迁移；已有 `.msgraph` 格式规则复用原实现 |
| PE-02 稳定参数绑定 | 图模式复用原编译器参数转换；布局只存参数 ID；多视图/参数重排/布局移动均检验；六种 WGSL 参数类型实际编译 | Curve 对象明确拒绝；不会伪装为支持 |
| PE-03 草稿与失败保存 | 不完整图、错误 WGSL、失效绑定能保存；编译/绑定阶段返回真实错误；NaN/超限拒绝；发布失败保留原文件并清除暂存 | 未新增保存 worker/UI，库 API 同步返回，接 GUI 时必须放到受控 worker |
| PE-04 原生布局数据 | Row/Column/Group/Tabs/Split/Parameter/Preview/Timeline/Note，尺寸与层级预算；[4 项布局检查](plugin-editor-m1/native-layout-tests.txt) 通过 | 数据描述尚无新运行时，不是界面或拖拽验收 |
| PE-05 默认错误信道 | 实际缺文件、JSON、失效绑定、WGSL、缺失图输入和并发拒绝均有请求/阶段/原因；[结果](plugin-editor-m1/default-channel-results.json)、[文件日志](plugin-editor-m1/default-channel-file.log)、[stderr](plugin-editor-m1/default-channel-stderr.txt)、[stdout](plugin-editor-m1/default-channel-stdout.txt) | stdout 只有测试框架信息，没有应用日志；实际 CLI 另外验证 |
| PE-06 阻塞与 panic | 受控停在 read 和 compile 入口时，先有持久化 Info start 与可查询 in-flight；panic 保留原 hook 和回溯、继续向调用者传播；清除状态后真实编译恢复 | 未增加超时、隐式重试或 worker 重启；不声称能终止不返回的处理 |
| PE-07 实际 CLI | 带 FFmpeg 产品程序执行成功/缺文件/语法错误三种情况；退出 0/1/1，成功仅 JSON，失败 stdout 空，无 GPU/工程初始化记录；[结果与二进制 hash](plugin-editor-m1/cli-results.json)、[成功输出](plugin-editor-m1/cli-success-stdout.json)、[缺文件](plugin-editor-m1/cli-missing-stderr.txt)、[源码错误](plugin-editor-m1/cli-invalid-stderr.txt) | `--plugin-check` 只验证逻辑，不是独立可视化窗口，不输出插件包 |
| PE-08 回归 | 完整图 crate 无默认 feature 回归：76 passed、29 ignored；[原始日志](plugin-editor-m1/graph-regressions.txt)。新增三项预算/示例/诊断限长检查后，工程测试最终 13 passed | 图回归里的早期 10 项工程测试与最终 13 项有重叠，不能相加；29 项 GPU 检查明确未运行 |
| PE-09 构建 | Windows 带 FFmpeg 的产品与测试二进制构建通过；[产品](plugin-editor-m1/desktop-build-summary.txt)、[测试](plugin-editor-m1/desktop-test-build-summary.txt) | 原有 desktop 警告分别 29 / 26 项；新增文件未报告警告。尚无此分支 Linux/macOS CI 证据 |

本轮没有修改渲染语义、旧效果包字段或基线；已有导出/registry/图公共参数回归通过。
新 WGSL 已经过真实解析、验证和便携后端生成，本轮未以 GPU 像素证明新示例输出。
全部新增示例源码由本项目独立编写；没有新增依赖、素材或来源许可例外。

## 原始失败与修正

1. 工程错误的最初派生实现把 `Box<graph::Error>` 暴露为 source，无法按原始图错误类型读取。
   [第一轮日志](plugin-editor-m1/initial-native-cause-failure.txt) 为 9 passed / 1 failed。
   改为明确返回 `cause.as_ref()`，保留原断言，最终图像/本机 IO 错误可按原类型读取。
   没有放宽测试或改用“只要有错误就算通过”。
2. 将桌面测试 exe 单独复制到临时目录时，未带 FFmpeg DLL，进程以 `0xC0000135` 退出，
   未进入测试。补齐该构建原本依赖的七个 DLL 后，默认信道子进程测试通过。
   这是本地测试运行目录准备遗漏，不作为通过证据或产品启动成功证据。
3. 手工 CLI 验收脚本对空 stderr 调用 `.Contains()`，出现 null 方法调用错误。
   保留第一次三种真实进程的通道文件及退出码，用 stop-on-error、支持空值的断言重新
   检查所有字节，结果记于 `cli-results.json`；不使用首轮脚本中的 `ok` 作为独立结论。

`Error` 的 Display 与 Debug 均使用受限摘要，避免 Result-returning main 在终止时
通过 Debug 泄漏完整 WGSL 错误片段；原始原因仍由 `Error::source()` 保留。
独立检查验证摘要长度、单行化与原始长原因未丢失。

## 观察边界与待办

- **新增观察**：无窗口 read/compile 的请求、源路径、阶段、耗时、in-flight、Error 与异常退出。
  请求不建立文档键注册表，源路径有独立长度限制；健康完成留在 Debug。
- **复用覆盖**：现有文件 logger、Error stderr、panic 强制回溯与原 hook、同目录暂存文件。
  复用固定站点 poison 观察 helper，范围为进程寿命内的锁恢复，健康访问不重置；
  本轮不宣称新增或重验了该 helper 的 60 次阈值。
- **正常分支**：检查完成后的 `current() == None` 是空闲，不报错；未引入取消、过期请求
  或轮询热路径。重复进入当前单操作 checker 被明确拒绝，保持原 in-flight。
- **功能边界**：库返回类型化错误，应用校验边界记录；新保存入口尚未接 UI，
  因此没有将保存 worker、清理二次失败或 GUI 提示标作本轮完成。
- **待验收**：布局渲染、宽窄/高 DPI、实际主/浮动窗口、IME、拖拽/撤销、参数手势、
  宿主断连、源码位置映射、布局打包与粒子结构化对象均待后续阶段。

`DESIGN.md` §0.7 的 path_controls 尺寸/列/控件、effect_controls 行/枚举/布尔/层级、
timeline 坐标/吸附线各项在 M1 均 **retained**：未修改这些面板，属性面板整改属于
PR #557 独立验收。新增布局没有 paint 代码或样式字面量，现有样式消费者策略检查通过。
`DESIGN.md` §12 的可见界面条目均 **pending verification**，不以编译代替截图。

## 复现与源码身份

```powershell
cargo test -p motion-effects --test native_layout --locked --offline
cargo test -p motion-graph --no-default-features --locked --offline
cargo test -p motion-desktop --bin motion-studio --no-default-features --features ffmpeg --locked --offline plugin_check::tests::actual_source_checks_publish_before_blocking_and_preserve_default_error_channels -- --exact --nocapture --test-threads=1
cargo build -p motion-desktop --bin motion-studio --no-default-features --features ffmpeg --locked --offline
motion-studio.exe --plugin-check crates/motion-graph/examples/parameter-tint.msplugin
```

FFmpeg 运行时按现有桌面构建方式准备，默认信道测试须不设置 `MOTION_LOG`。
相关生产/测试/示例源码的 [SHA-256](plugin-editor-m1/source-sha256.json) 与原始文本证据随本提交保存。
源码 hash 按 Git blob 字节计算，不受本机检出行尾影响。Cargo/测试框架输出末尾的空行
在证据副本中保留；代码与说明的差异空白检查通过，原始输出四处 EOF 空行不作格式修剪。
