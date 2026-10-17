# 曲线时间尺的 seek 反馈循环

日期：2026-10-17。基于 main 的 e0f1e29，处理曲线编辑器中画面停滞、空格播放无法响应且持续刷 seek 的反馈。不修改布局、绘制样式、吸附阈值、工程格式或依赖。

## 证据与原因

用户提供的 last-stalls.json（SHA256：bc28ba918f754b869d3b513c4054d39c53a4eff82dd7501479c6f3e69f664765）最近 128 条记录全部为 playing=false、frame=25：面板构建、worker present、worker seek、edit_async 回复各 32 条。该窗口内 seek 用时 342–850 微秒、present 1423–2521 微秒，支持快速重复请求的判断，不能据此认定 GPU 或解码器长时间阻塞。文件另有 49 条早期慢操作记录，本修复不解释全部历史卡顿。

复现工程独立创建位置关键帧 27、67。上方时间尺把指针所在第 25 帧吸附到 27，下方曲线时间尺按相同横坐标取整为 25。两处共用 scrubbing 和 last_seek，导致每次面板构建都产生 Seek(27)、Seek(25)。既有连续 seek 合并会留下最后一个请求，但下一次确认帧重绘再次产生同一组动作，排在后面的鼠标释放和空格输入迟迟不能处理。因此一次点击产生两个面板动作，不代表每轮一定执行两次 worker seek；合并动作本身仍无法终止反馈。

新增 ScrubRuler 记录本次拖动起点，只有该时间尺处理后续移动和释放。上方保留吸附，下方保留原有取整；跨过另一时间尺不转移所有权。下方开始新手势时清空上次 seek 去重值，避免同一位置的新点击被跳过。失焦、Escape、切换图表模式继续沿用既有取消逻辑。

## 验证

- 修复前真实 Shell/Engine 回归失败：lower_ruler=false, seeks=[27.0, 25.0]；保留 curve-playback-reproduction.log。
- 最小修复后同一回归通过，保留 curve-playback-fixed.log。
- 最终扩展回归覆盖两条时间尺、跨时间尺拖动、最终释放坐标、连续确认帧不产生 seek、不吞待处理输入、空格启动播放、实际 worker 播放 tick、失焦/Escape 取消、工程数据保持。最终桌面全套 367 项通过、0 项失败，1 项既有真实音频设备测试忽略，用时 176.45 秒；记录 curve-playback-regression-final.log。现有 17 项编译警告未在本次更改。面板 style consumer 检查、独立新测试文件 rustfmt 与 git diff --check 通过。
- 测试移到独立文件时首次编译因平级测试模块无法访问 draw_acknowledged 私有方法失败，随后改为 presentation 的测试子模块；不扩大产品方法可见性，未放宽断言。记录 curve-playback-regression.log。
- 代码提交 921a96d 的带 FFmpeg Release 构建完成（优化 + line tables、thin LTO），用时 10 分 31 秒，构建输出 22 项警告，记录 curve-playback-release-build-final.log。首次构建因本地临时覆盖 release debug 配置导致依赖全量重编译，已主动中断并恢复仓库原有 line-tables-only 配置；首次日志保留为 curve-playback-release-build.log，不算构建通过。
- 用正式打包工具产出包含 7 个 FFmpeg DLL 和许可/来源清单的 Windows 包，在独立目录解压。tools/ci_desktop.py 对该解压版本完成中英文原生窗口、浮动窗口样式、节点 GPU 渲染和共享 MCP 验证，记录 curve-playback-native.log 与 curve-playback-native/；没有用测试可执行文件代替可运行程序。
- 此处记录的本地构建对应代码提交 921a96d；之后的验收文档更新不修改 Rust 或资源。原工程连续播放性能仍需用户复测。完整右侧视图容器及全部输入路由的结构拆分不在此实现。

## 错误处理审计

| 编号 | 分类 | 处理与边界 |
| --- | --- | --- |
| CRF-01 | 功能修复 | 修正事件归属，停止正常操作自己生成重复动作。没有静音日志、丢弃错误、增加重试或调整 worker 队列。 |
| CRF-02 | 既有信道 | seek 与播放仍走 Engine 异步回复和既有日志设施；Info start、原始失败原因、Error 文件/stderr 与 stdout 隔离没有变更。此改动没有引入新的可失败 IO 或锁内日志。 |
| CRF-03 | 正常分支 | 非当前时间尺忽略其他时间尺的拖动、同一帧去重、确认帧无新动作、取消交互均是正常路径，不记为错误或以计数阈值报警。 |
| CRF-04 | 待用户验证 | 修复后的原工程连续播放、音频同步、视频跨层解码延迟及早期慢帧仍需实际复测；没有据此关闭涵盖其他问题的 issue。 |

## DESIGN.md §0.7 基线处置

以下逐项 retained with reason：本次仅处理输入功能，保留既有视觉问题和兼容坐标，没有声称完成 §12 的整体视觉验收。

| 基线项 | 处置 |
| --- | --- |
| path_controls 六处纵向步进 | retained：文件未改，本次无视觉整改。 |
| path_controls 两列宽度 | retained：文件未改。 |
| path_controls 混合按钮高度 | retained：文件未改，命中范围未缩小。 |
| path_controls 自绘数值字段 | retained：文件未改。 |
| path_controls 全宽布尔按钮 | retained：文件未改。 |
| effect_controls 行高与步进 | retained：文件未改。 |
| effect_controls fx/— 布尔按钮 | retained：文件未改。 |
| effect_controls 全宽枚举按钮 | retained：文件未改。 |
| effect_controls hover alpha | retained：文件未改。 |
| effect_controls 缩进/层级 | retained：文件未改。 |
| timeline 十一列坐标 | retained：保留兼容基线，改动仅在时间尺事件处理。 |
| timeline 吸附线硬编码颜色 | retained：按规范保留，本次不新增 token 或绘制常量。 |
