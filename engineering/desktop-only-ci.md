# 桌面端维护与 CI 范围

后续开发集中于 Windows、Linux 与 macOS。Android 实现停止维护，源码、历史文档和显式手动工具保留；本次不删除 `android/` 或 `crates/motion-android/`，不改写已有历史发布。

## 自动检查

- 保留共享 Rust/GPU、SDK、来源校验、桌面图标、四种桌面构建、原生窗口与共享 MCP、独立 UI 框架及可选 FFmpeg 后端验证。
- Rust 工作区运行 `cargo test --workspace --exclude motion-android --locked --features motion-host/diagnostics -- --test-threads=1`。直接启用共享 host 诊断，保留原先由 Android feature 转发的覆盖。
- 移除 Android 构建、Java/Gradle/SDK/NDK 安装、模拟器交互、签名密钥处理、APK 验证和上传任务。
- Android 图标生成器与构建脚本改为手动工具。`test_android_build.py` 也不再匹配 CI 的 `test_ci_*.py` 发现规则。
- 工具清单默认不审计冻结的 Android 测试树；需要时显式传入 `python tools/validation_inventory.py --include-android`。共享 Python 工具的静态回归仍保留。

## 桌面发布

main 的受信任 push 继续生成预览版，合法 `vMAJOR.MINOR.PATCH[-prerelease]` 标签的 push 继续生成版本发布；普通分支、PR 和手动工作流触发只做验证。发布任务等待 Rust/GPU、桌面矩阵及 FFmpeg 检查完成。

`python tools/ci_release.py publish-desktop` 仅收集 Windows x86_64、Linux x86_64、macOS ARM64 和 macOS x86_64 四个平台包，不读取 APK 或 Android 的签名产物。所有平台的 sidecar 必须匹配当前 commit、ref、kind、run 和 version，并通过尺寸、SHA256、能力记录与 macOS ad-hoc 签名标记检查，之后才允许写入 GitHub。

保留旧 main 预览防覆盖和已发布版本不可替换的校验，生成桌面专用的 `build-info.json`、`SHA256SUMS` 与下载表。已有旧发布中的 APK 不在本次工作中删除；之后的桌面发布不会更新它们。旧的 APK 命令仍是手动入口，CI 不调用。

## 本地验证

```powershell
python -m unittest discover -s tools -p 'test_ci_*.py' -v
python tools/validation_inventory.py
python tools/validate_provenance.py
python tools/generate_desktop_icons.py --check
python -m compileall -q tools
```

本次 Python 套件运行 41 项，40 通过、1 项因未配置 Rust 效果校验器而跳过。覆盖发布触发条件、无 APK 的四平台上传、缺失/过期/损坏/未签名包、过期 main、不可替换版本、稳定版与预发布标签，以及缺少 Android 树时默认清单检查仍能运行。发布测试使用临时文件和模拟 GitHub 调用，没有执行真实发布。

来源、图标、Python 语法和差异空白检查通过。本次仅修改流程、Python 工具/测试和维护说明，Rust 与 UI 源码未改动；跨平台构建和 GPU 验证以该 PR 的远端 CI 为准。
