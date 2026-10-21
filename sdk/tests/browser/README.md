# 历史 SDK 编辑页回归

此目录仅验证 `crates/motion-effects/tests/fixtures/scene-library/ui` 的第一方历史编辑页，不是产品 UI 或内置效果入口。

先构建 `cargo build --locked -p motion-render --example plugin_editor_probe`，在已有 Playwright 1.63.0 测试环境设置 `EDITOR_PROBE_BIN` 指向该探针，再执行 `node test.mjs`。探针显式注册测试归档，不修改产品默认插件目录。
