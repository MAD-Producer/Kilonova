# 交叠编码器的资源预算回收

核验日期：2026-10-21。关联 #61；不关闭其整项内存策略与压力验收任务。最初复现基线为已合入 #395 的 main `58abfe01`；最终重新编译基线同步到 `7967ae21`，包含由其他分支合入的 #396／#397。未修改其他任务的工作树，没有重复实现曝光及动态效果资源接入。

## AR-01：功能边界修正

旧实现创建编码器时升级所有尚存的分配引用，包括仅由旧命令、上传日志或视频提交令牌保留的分配。空编码器可再次继承这些分配，在持续交叠期间使已经完成的目标无法退出预算。

公共 API 的实际 GPU 对照依次申请四个 8×8 RGBA 目标，编码清屏、释放应用句柄，创建下一空编码器，再提交旧编码器并等待 GPU 完成。旧实现逐帧占用为 `[260, 516, 772, 1028]` 字节，包含 4 字节系统纹理；最终销毁最后一个空编码器才回到 4 字节。证明的是传递保留和预算假耗尽，不是驱动 VRAM 永久泄漏或完整桌面峰值。

本次区分实际资源所有者与既有工作的保留引用。仅实际资源、视图和绑定所有者参与新编码器的初始快照；旧命令、上传日志和已编码视频令牌继续持有各自分配，直至既有提交完成或放弃。编码中新增的分配仍进入当时存活的 scope，避免提前归还。未增加提交、轮询、重试、超时或线程重启。

临时升级的分配在预算锁外筛选和释放，避免最后一个引用的析构再次获取同一状态锁。所有者计数以实际句柄的克隆／释放增减，上界由 Arc 强引用数约束；不按帧数或累计故障计数。

## AR-02：回归与负面对照

新增 `overlapping_frames_do_not_renew_completed_target_reservations`，每帧在 GPU 完成后要求预算回到 4 字节。定向回归包括库测试及 allocation_budget、allocation_retirement、source_allocation_budget、video_allocation_budget，共 75 项通过。

在相同源码中仅临时去掉所有者筛选，新增测试实际退出 101，报告 `[260, 516, 772, 1028]`；随后原字节恢复，SHA-256 为 `d184896ce62ba6ba81ec989674ef39c6309138826e54c1576baecdfa23411cda`。证据位于开发者工作区 `artifacts/pr-ci-followups/allocation-retirement-negative-control.log` 及同名 JSON，未将失败实现提交。

既有 `unsubmitted_finished_and_submitted_commands_hold_admission_without_diagnostics` 的一处断言有意收紧：GPU 已完成且只有无关空编码器存活时，从允许保留 260 字节改为必须回到 4 字节。原有提交前保留、预算拒绝、放弃与 GPU 完成检查继续保留。该更正定义了本次功能修复后的释放契约，没有删除断言、放宽阈值或像素容差来取得通过。

首次完整 motion-render／motion-host 回归在生命周期断言处失败：GPU 等待返回后仍为 260 字节，快照显示一个 scope 存活。未放宽断言。后续临时所有者追踪显示最后一个实际所有者从 1 归零、新快照看到 0，测试通过；追踪代码已原字节撤销。恢复构建后，两次固定二进制单项及一次整组检查通过，相同双 package 配置下的七项预算和一项交叠回归也通过。临时追踪会改变执行时序，不能据此断言首轮失败只是缓存或驱动问题。

首次失败日志为 `allocation-retirement-full-regression.log`；新增失败上下文后的复现为 `allocation-retirement-callback-diagnostic.log`。固定测试程序 SHA-256 为 `a516393abd9b3e89ef6004c4d2eca4975e3f1da11fba042219c7880ebf7878fa`，三次记录见 `allocation-retirement-immutable-diagnostics.json`。首轮失败的根因尚未证明，不能用后续通过覆盖这份证据。

接续同步 main `7967ae21`，仅清理本任务 target 中的 motion-render 构建产物，重新编译及完整回归：**467 通过、0 失败、1 项既有忽略**，共 84 个测试组（含文档测试），构建与测试合计 1965.45 秒。上述生命周期断言、四帧交叠、#397 的曝光／动态效果、上传拒绝、原始像素与视频提交令牌均通过。原始日志为 `allocation-retirement-clean-full-regression.log`；汇总为 `allocation-retirement-clean-summary.json`。五个变更 Rust 文件在整个过程中保持源哈希不变，收据为 `allocation-retirement-clean-source-receipt.json`。此轮成功不证明首次失败的根因已修复，也不替代跨平台 CI。

唯一忽略为既有 `optional_local_original_image_probe`，需要仓库外的 `MOTION_IMAGE_SOURCE`；未新增忽略或改变其条件。随后 `cargo build -p motion-desktop --features ffmpeg --locked --offline -j1` 通过，94.95 秒，保留 21 条既有桌面警告。退出记录为 `allocation-retirement-clean-results.json`，构建日志为 `allocation-retirement-desktop-build.log`。

复验命令：

```text
cargo test -p motion-render -p motion-host --locked --offline -j1 -- --test-threads=1
```

## AR-03：观察、正常分支与剩余边界

没有新增错误观察、日志事件、静默降级、返回值转换或信道协议。所有者已释放是正常生命周期分支，不应记录故障。预算拒绝、跨预算／队列拒绝、状态中毒及回收不变量失败继续使用既有类型化错误及锁外观察。

上述定向回归包含真实中毒、缺失注册和下溢对照，验证默认 Info 过滤下单次 Error 事件、先解锁再记录和 stdout 隔离；这些测试使用捕获 logger，不能单独证明桌面文件及 stderr 路由。

另在相同源码和带 FFmpeg 的桌面测试程序中运行既有 `engine::failure::composition_tests`，**7 项通过、0 失败、0 忽略**，运行 78.72 秒。子进程清除 `MOTION_LOG`，初始化实际默认 logger，分别触发容量、组合分配、动态效果参数、曝光 uniform、源时间、视频交接预算和视频 uniform 拒绝，检查原始信道回复、Info 文件记录、单次 Error stderr、stdout 隔离及健康恢复。日志为 `allocation-retirement-default-log-channels.log`。这些是既有错误通道的实际故障复验，没有新增观察或改变协议，也没有据此宣称所有错误和未返回路径已审计。

未覆盖完整桌面的反复缩放／seek／切工程、平台显存压力、全部资源类别及整帧峰值，不改变渲染像素语义、工程格式、依赖或第三方素材。#61 保持开放。

未修改 panels、用户可见样式或右键菜单。DESIGN §0.7 的十二项逐项处置沿用 `host-error-contract.md` 的基线表，全部 retained：本次仅修正 GPU 分配的引用生命周期，无对应视觉改动；不声称通过新的视觉验收。
