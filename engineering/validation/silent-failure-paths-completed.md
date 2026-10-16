# 静默失败路径处理完成记录

审计源：`silent-failure-paths-exhaustive.md`；辅助定位：`silent-failure-paths.md`。分支：`fix/silent-failure-paths`，基线为引擎观测提交 `4fbba52`。原始清单保留，本文只登记已核实的结论。

全集机械枚举共 4443 行，其中 832 行未标记 no-loss/test-only/guarded。这个数量是待审候选数，不等于已确认缺陷数；既有标签也不能替代源码核对。以下记录未覆盖的条目仍待审，不声明全量完成。

## 01 · worker 响应断开与启动快照

状态：已修复并通过针对性测试。

- `desktop/app/src/main.rs`：attach / present / resize / freeze_movie 的轮询区分 Empty 与 Disconnected，断开时进入既有错误处理并清空等待槽位，避免无限等待
- `desktop/app/src/engine/failure.rs`：集中提供 poll_reply，保留原始成功/失败结果，断开错误携带 operation 及 worker failure
- `desktop/app/src/engine.rs`：启动快照失败直接通过 ready 返回原始错误，避免以 initial=None 继续启动
- 新增测试覆盖四类 operation 的 pending、成功、失败与断开；既有真实 worker panic、恢复副本、资源释放及关闭测试仍通过
- 验证：`cargo test -p motion-desktop --offline engine::failure::tests -- --test-threads=1`，9 passed
- 关联辅助清单：0.6、1.1–1.4、2.31、10.5

## 已核实应保留的行为

| 位置 | 结论与源码证据 |
| --- | --- |
| `crates/motion-render/src/effect_plan.rs:314` | fingerprint 同时写入低 32 位和高 32 位（下一行），属于完整拆分编码，不是丢失高位 |
| `crates/motion-model/src/model.rs:649`（辅助 0.1） | migrate 在升级前 validate，validate 明确拒绝 1..=18 之外版本；4..8 无须重写内容，已有视频/旧工程迁移回归测试。升级后复验可作为额外防线，不支持“未知版本被盖成 8”的断言 |
| `crates/motion-core/src/expressions/mod.rs:15` | 序列化的是 ExpressionTarget 错误定位信息，并非表达式体；该枚举仅含整数、字符串和可序列化枚举。清单所述“表达式体被清空”不成立 |
| `crates/motion-model/src/vector/groups/sampling.rs:188,189,198` | 饱和乘法后与小于 usize::MAX 的严格资源上限比较；溢出会触发拒绝，不能改为溢出回绕或记为静默通过 |
| `crates/motion-core/src/history_accounting.rs` | JSON 长度累加饱和为 usize::MAX 后被预算拒绝，是保守计数，不是漏算 |

## 前置观测改动

`4fbba52` 已完成文件日志、panic hook、worker Flight/排队深度、图片等待与 surface 失败计数及 trim 记账检查。对应 PR 为 #306；它仅补可观测性，不代表清单中的行为问题已全部修复。详见 `engine-observability.md`。

## 验证与样式边界

编译环境：`. .\.tools\msvc-portable\env.ps1`。原始测试输出保存到 `target/silent-failure-validation/`。

本轮不做视觉重排。涉及 panel 错误处理时仍遵循 DESIGN.md；§0.7 的既有视觉整改项按实际情况记录，不将编译成功当作完整 UI 验收。
