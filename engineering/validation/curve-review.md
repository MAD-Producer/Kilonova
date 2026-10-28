# 曲线选区适配与锁轴审阅记录（#259，草稿）

功能已实现；性能差异和最终组合验收仍未完成，本记录不关闭 #259。运行源码 `23c0f96c58a97711eadab0ce5ee15f7d05cda35b`、主线 `21093ee76231b76bbeb53055689cae4858c51897`、测试二进制 SHA-256 `ee1bce6d041c505252930e69696f2b0c57c9a3a8b49319d65b050cb8988fea43`。之后合入 `7dfab923a1b4005e2c8b31460bbe766738f78a10` 的透明平面深度，下面的运行结果不能冒充该新组合的完整验收。

## 当前运行结果

- 九组完整曲线回归 136 项通过；其中 1 项原生测试保留 ignored，另行显式执行成功。新增性能测试不属于这九组 filter，没有把它的 ignored 混入计数。
- 实际隐藏主窗/浮窗，属性、效果、对比和迷你图的数值/速度共 32 场景、160 张 PNG；中英文及逻辑尺寸/ratio 的四个实际组合见 JSON，不是所有条件的笛卡尔积。ratio 为 1/1.5/2，不改变系统 DPI，也不冒称物理鼠标键盘输入。
- 生产 Input 路由验证选区适配、锁轴后播放头/滚轮、不改历史、IME/重复键、失焦、另一窗口取消与过期状态。真实 worker revision 的取消另有独立场景。实际保存前后文件字节相同，Core 重开相同；外部编辑的撤销与视图不改历史分别验证。
- 最近曲线像素最大距离 0.667262 物理像素。人工检查当前原生输出的 EN 主窗属性锁轴、ZH 主窗效果速度读数、EN 浮窗对比分组锁轴、ZH 浮窗迷你物理速度读数；其余图像为程序化尺寸/像素校验，未冒称逐张人工审阅。
- 新增 20 个观察站点中 19 个实际验证默认 Info 级别的文件、Error stderr 和 stdout 隔离，并验证首次 Warn、第 60 次 Error、之后抑制、健康调用不重置既有进程生命周期计数、新进程重置。`curve.mini.normalized_readout` 为源码有限域防御处置，22 个合法 Easing/22550 次采样及实际悬停/畸形数据先被 fit 拒绝；不是错误注入或第 20 个已修缺陷。详见旧[防御边界依据](curve-guard-progress.md)和新 ZIP 的 fault-channel-audit。

范围回归实际耗时 655.33 秒。最初监督脚本的 600 秒上限低于历史正常耗时 605.45 秒；在上限到达前取得原测试进程的 Windows 句柄，仅停止 Python 监督程序，保留测试和故障子进程继续运行。读取原进程实际退出码 0 和原日志，再执行剩余 filter，没有重试测试或把超时伪造成通过。`supervision-adjustment.json` 保留过程；工具脚本后续上限改为 1800 秒，因此 ZIP 中当前工具脚本并非原 600 秒版本的逐字快照。

## 性能仍需定位

相同测试源码在基线 `f69b0d81858951911c1105778b5254bd9ade51d5` 与功能源码 `78d341d79c78833105e2225e630aaf64fcca5acb` 上编译。真实 Shell 的 Composition/Info/曲线布局，单个自有 640×360 动画图层和自有效果；时间轴 zoom 值为2/128像素/帧（非相对默认放大128倍），Mini 仅默认倍率。28 场景，每进程 5 预热+60 测量，ABBA 四进程合计 6720 测量帧、560 预热帧、112 PNG。实际工程和效果包 SHA 完全一致。

所有静止测量帧均为 0 渲染/60 复用，连续不同帧均为 60 渲染/0 复用，逐帧 Info 计数核对一致，工程/revision 不变；所有测量的 Present polls 都为 1。没有将常量帧反复 Seek 的早期试验算作静止复用证据。

线程 CPU 使用 [GetThreadTimes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-getthreadtimes) 的 100ns 计数；[QueryThreadCycleTime](https://learn.microsoft.com/en-us/windows/win32/api/realtimeapiset/nf-realtimeapiset-querythreadcycletime) 保留原始周期，不把周期换算成时间。探针本身有成本，CPU 时钟有粒度限制；这些进程没有调整线程优先级或亲和性。实际后端 Vulkan、NVIDIA RTX4070 Laptop GPU。

**仍有未解释差异**：`comparison-value-x2-seek` 的合并 UI p50 为基线12364µs/功能15393.5µs（+24.5%），线程平均周期+27.8%。功能两轮分别43.59M/30.67M周期，基线29.43M/28.68M，其他场景也有较大轮次波动。此前 schema2 的高倍率对比组 +24.68%，schema3 该组为-1.5%墙钟/-0.4%周期；不能据此宣布问题已经修复，或把所有差异归因于调度等待。保留全部四轮原始数据，继续定位。

这是 debug+FFmpeg、小 Composition 视口、直接寻帧的开发机测量；背景开发负载和温度未控制，不能代表默认工作区、大场景、实际播放 FPS 或跨设备性能结论。缺失的本机 Epic Vulkan overlay JSON 诊断原样保留，没有冒称修复。

## 审阅边界与剩余事项

5 个新增尺寸/误差 token 先定义在 dark.json，再在绘制中读取；锁轴使用已有 Checkbox。没有新增第三方依赖/资产、项目格式或历史系统。自有原生截图、独立编写的 Windows ABI 与已有自有效果示例不涉及 Adobe 二进制或素材。

DESIGN §0.7 的既有12项逐条保留及理由见[边界记录](curve-guard-progress.md#design-07逐项责任边界)，不把本轮曲线改动算作 Path/Effect 属性行整改。新增文件通过 rustfmt；既有文件的混合格式检查失败保留在 ZIP，未宣称全库 fmt 通过。git diff --check 通过。完整 DESIGN §12 的范围处置、最新主线构建/原生组合、all-targets、性能差异及五项实际 CI 组合树验证仍待完成。PR 保持草稿，暂不自动关闭 issue。

[结构化结果](curve-review-results.json) · [原始证据](data/curve-review-stage-27.zip)：483 项/22142671 字节，SHA-256 `d3f7a2ee1f28f72a497517dfbaade1683fb575babed60bce3dea623118510a59`。未发布本机 EXE/DLL/PDB；省略文件清单在 ZIP 内。
