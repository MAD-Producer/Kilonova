# 主窗与浮窗的原生输入路由验收

关联 #135 的 presentation / 事件覆盖子项。基于 #389 最终提交 `63869ac1`，在独立分支 `test/native-window-input` 补测试，不修改发布版行为。开工前核对开放 PR、近期分支和工作树：#389 正在验证工程格式与媒体进度；`feat/semantic-resource-contracts` 处理浮点颜色输出及真实导出验收，主工作树保留属性跟进工作。本次不接管这些实现。

## 实测范围

新增 Windows 原生子进程测试创建三个真实隐藏窗口，在 `ApplicationHandler::resumed` 提供的真实 ActiveEventLoop 中，将显式构造的 winit WindowEvent 交给生产 Shell.window_event；浮窗由生产路由进入 floating_event。Shell 使用实际 Engine 和临时工程/偏好目录，不读取真实用户配置。测试不附加原生 GPU surface，也不调用面板绘制。

| 编号 | 分类与证据 | 边界 |
| --- | --- | --- |
| NWI-01 | 新增既有行为覆盖：主窗比例 1 / 1.5 / 2 与浮窗比例 2 分别换算物理坐标；鼠标按下/松开使用接收窗口的最后坐标，松开清除 held/origin。 | 比例是受控字段，不声称发生了真实显示器 DPI 切换或 ScaleFactorChanged/surface 重配。 |
| NWI-02 | 新增既有行为覆盖：行滚轮与像素滚轮得到明确的累计量；中文 IME Preedit/Commit/Disabled 保留提交文本且不污染另一窗口，提交和滚轮不会重复累加。 | 验证输入累积一次；不把尚未执行的面板编辑称为一次渲染副作用。 |
| NWI-03 | 新增既有行为覆盖：Control 与 Super 均映射 control，Shift/Alt 保留每窗口状态；CursorLeft 与 Focused(false) 清除相应的按下、拖动源、组合输入及修饰键状态。 | 本测试没有预先建立面板编辑事务；不能据此声称验证了所有工具的取消/撤销。 |
| NWI-04 | 新增真实路由覆盖：一个窗口先注册为浮窗再从映射移除，随后迟到的移动、松开、失焦、IME、resize 与 dropped-file 事件均不进入主窗，不生成 resize/import 动作。 | 使用真实仍存活窗口 ID 模拟已关闭/重停靠后的迟到队列；不依赖系统恰好产生某一顺序。 |
| NWI-05 | 既有功能保持：输入检查后实际工程内容与 revision 相同，未产生工程编辑；相邻 presentation 的真实手势、确认帧、播放与事务测试保持原断言通过。 | 测试注册被 Windows cfg(test) 限定；不改变发布版事件翻译、日志、超时、重试或状态返回。 |
| NWI-06 | 待验证：真实 OS 键盘输入、平台 IME 服务、原生物理 DPI 切换、Linux/macOS 原生窗口宿主和附加 surface 的事件全过程。 | KeyEvent 不以 unsafe 伪造。#135 继续开放，不把原生窗口 ID/ActiveEventLoop 等同于真实硬件输入验收。 |

没有新增日志观察、异常吞并或产品错误路径，本项不能替代默认日志信道验收。正常失焦、窗口移除及无输入均为正常分支，不添加故障日志。DESIGN §0.7 的十二项既有基线全部 retained：本次只增加测试和条件模块注册，没有修改可见 UI、菜单、尺寸、颜色、命中区或渲染实现，不以编译声称视觉验收完成。

## 验证与错误对照

Windows portable MSVC，既有 FFmpeg 环境，继续使用本工作树的独立 `target/host-error-contract-verified` 缓存；不在源码不同的工作树间交替编译。依赖锁定离线、单编译任务、单测试线程。原生测试通过子进程隔离 winit 的每进程事件循环约束。

- 首次新增测试直接通过，1 项，6.90 秒，`artifacts/pr-ci-followups/native-window-input-first.log`。
- 临时删除主窗 ID 判断：同一新增测试失败，cargo 退出 101；迟到窗口把主窗坐标改为 `[499.5, 499.5]`，与预期 `[10, 20]` 不同。日志 `native-window-input-control-stale-window-guard.log`。
- 临时将浮窗滚轮换算中的 `f.scale` 改成主窗 `self.scale`：同一新增测试失败，cargo 退出 101；实际 `[-64, 128]` 与浮窗预期 `[-32, 64]` 不同。日志 `native-window-input-control-floating-wheel-scale.log`。
- 两项对照均由脚本在 finally 中逐字节恢复源码；`git diff --exit-code -- desktop/app/src/main.rs desktop/app/src/shell/presentation.rs` 通过。没有改动断言让错误对照通过，没有提交故意错误的生产代码。
- 恢复后 `cargo test -p motion-desktop --bin motion-studio --features ffmpeg --locked --offline -j1 shell::native_input_tests:: -- --test-threads=1`：1 项通过、0 失败、0 忽略，5.38 秒，`native-window-input-final.log`。
- 相邻 `shell::presentation::` 全组：14 项通过、0 失败、0 忽略，9.63 秒，`native-window-input-presentation.log`。包括现有节点/属性/蒙版手势、确认帧和播放屏障；不修改其既有断言。

合计 15 个不同测试通过，不将首跑和故意错误对照重复计入。现有桌面编译警告保留。最终提交的 CI 尚需单独运行；#389 的绿色结果不能证明本次新增测试已在 CI 执行。
