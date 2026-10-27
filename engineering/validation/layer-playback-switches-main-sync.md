# 图层输出开关：同步模型图层接入

PR #523 的原提交 `f67f56c6f51a919d87cb3bdf6e5e910dc334344e` 已通过 CI run `40456211615` 的依赖审计、Rust/GPU 和 Windows、Linux、macOS Intel 全部有效检查。尝试合并时，main 已前进到 `650d0a01`，GitHub 拒绝创建合并提交；没有绕过检查或强推 main。

本次同步的唯一文本冲突位于 `crates/motion-model/src/lib.rs`：分别保留 `layer_switches` / `LayerSwitch` 与 `model_assets` / `ModelAsset` / `ModelClip` 的模块和导出。其余 main 变更由 Git 自动合并。未改写其他 agent 的实现，未修改既有测试断言，项目版本仍为本 PR 的当前版本 28。

同步后模型与核心完整回归：484 passed、0 failed、1 个既有 ignored，共 91 个测试二进制/文档测试结果。原始输出保留在本地 `artifacts/pr-ci-followups/layer-playback-main-sync-core-model.log`。`cargo check -p motion-desktop --features ffmpeg --locked --offline` 通过，保留 23 条既有警告，原始输出为 `layer-playback-main-sync-desktop-check.log`。新提交 CI 仍需另行核验；原提交的绿色检查不能代表同步后的提交。

原来的原生窗口、像素输出、PCM 混合和默认错误信道证据仍保留在 `layer-playback-switches.md` 及其结果清单和证据包，指向原提交及当时的源文件/二进制哈希。本次没有改动开关面板的绘制和交互代码，不把旧截图改写为新二进制证据。`DESIGN.md` §0.7 的 12 项处置沿用原验收文档，本次没有新增视觉修复声明。
