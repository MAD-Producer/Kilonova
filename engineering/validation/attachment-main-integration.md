# 附件诊断与后台管线缓存的主线衔接（#630）

原附件诊断提交 `22fad1f9` 的八项远端检查已通过，但主线随后新增管线缓存和样式
原生错误。与主线 `99025016` 合并时，Renderer 的相邻公开方法发生文本冲突。
旧提交的绿色检查不作为合并后源码的验证结果。

本次保留两组接口：附件覆盖/消费者诊断 `effect_attachment_intents`，以及主线的
驱动缓存状态、保存、后台任务、恢复、取消和安装接口。没有更改任一方法的内部实现，
不改管线预热流程，不启用候选 Load/Discard，产品仍使用透明 Clear/Store。

## 合并验证

按同一份冻结源码执行：

- `cargo test -p motion-render --lib -- --test-threads=1 --include-ignored`
- `cargo test -p motion-effects --lib -- --test-threads=1`
- `cargo check -p motion-render -p motion-host -p motion-desktop --all-targets --features motion-desktop/ffmpeg`

同一份源码的结果：渲染器库 339 项、SDK 库 37 项，共 376 项不同测试通过，
零失败、零忽略；带 FFmpeg 的渲染器、宿主和桌面全部目标编译通过。
编译输出中已有的 dead_code 等警告保留在 stderr，不宣称零警告。

渲染器库包含两项实际附件策略矩阵/成本采样和四项附件诊断检查；SDK 库包含 IR
discard 分析。真实错误附件策略仍需产生错误像素，候选与 Clear 的完整 RGBA 仍需相等，
未修改任何断言、生产预算、期限或忽略规则。普通构建不包含测试策略开关。

证据目录 `artifacts/issue-goal-current/attachment-main-integration` 保存 stdout、stderr、
命令、源文件运行前后 SHA-256、父提交和合并父提交。`verify.py` 核对三阶段成功、
相同源码、每项实际通过测试、两组接口均存在，以及不存在冲突标记。

第一次 `render-lib` 没有最终测试结果和 run.json；工具会话已不存在，核查也没有剩余
编译/测试进程。该阶段保留为中断，不计成功。重新执行使用独立 v2 目录，未覆盖原日志，
也没有修改测试、延长超时或重启产品线程。

## 范围审计

| 编号 | 归类 | 处置 |
| --- | --- | --- |
| A63-M1 | 冲突修正 | 两个相邻 API 块均保留，方法体不变，实际编译与库回归验证 |
| A63-M2 | 既有错误覆盖 | 本轮没有新增日志点、worker、回退或故障恢复；沿用原生 Result/错误信道 |
| A63-M3 | 既有功能边界 | 原始两个独立进程成本记录保留；合并后的重测不替代 tile/带宽或其他格式资格 |
| A63-M4 | 待验证 | 推送后的精确提交 CI；#63 的完整 admission、精度/格式和 tile/带宽验证仍开放 |

无新增依赖、第三方资产或用户可见 UI 改动。没有因为已有性能采样波动而启用优化，
也不以附件诊断交付代替 #63 全部需求。
