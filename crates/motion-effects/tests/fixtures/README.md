# 已退役效果的测试夹具

`scene-library` 与 `particle-library` 是仓库自行编写的历史 SDK 回归数据，不再是产品内置库。原始归档、manifest、shader 和编辑页字节保留，用于验证版本身份、打包边界、投影/精灵、粒子历史、编辑事务及故障处理，不能从这里重新注册到产品启动路径。

仅测试和显式运行的 SDK 验收探针通过 `tests/support/scene_packages.rs` 加载这些文件。生产 `builtin::packages()`、默认效果目录和发行程序不包含这些归档。旧用户目录中的副本不删除，但启动时不再自动安装；旧工程仍保留效果及动画数据，尝试使用缺失效果时返回明确错误。

夹具生成器位于 `sdk/tests/support/`，归档身份回归为 `sdk/tests/test_scene_packages.py`，浏览器验收驱动为 `sdk/tests/browser/`。移动不改变已发布归档的哈希或契约。现有宿主 SDK 能力继续由显式注册夹具验证；节点编辑器与 UI 编辑器组合生成效果的产品功能尚未在本批实现。
