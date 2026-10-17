# 桌面预览包 FFmpeg 验收

日期：2026-10-17。用户提供的日志连续报告 `this build has no video backend; rebuild with --ffmpeg`，并确认程序来自 GitHub 自动预览构建。

## 原因与改动

原发布矩阵不传 `--ffmpeg`，打包器还要求 `ffmpeg=false`，README 与发布说明也明确不包含视频后端。单独的媒体 CI 测试通过不能证明最终预览程序包含视频支持。

- 四个平台的发布构建明确启用 FFmpeg；未启用该特性的开发构建仍保留原有显式错误。
- Windows 使用 BtbN 月末 LGPL shared 构建，固定来源、版本、build scripts revision 和 SHA-256；Linux/macOS 从固定 SHA-256 的 FFmpeg 8.1.3 源码构建共享库，禁用外部依赖自动探测、GPL、nonfree、编码器和命令行程序。
- 打包前校验运行库架构、文件哈希、解码组件集合和来源/许可证清单。Windows DLL 与 exe 同目录，Linux 使用相对 RPATH，macOS Frameworks 使用相对加载路径，并先签名 dylib 再签名 app。
- 包内附 `ATTRIBUTION.md`、`NOTICE`、FFmpeg 原文许可证、配置参数和 `FFmpeg-SOURCE.json`。没有把 FFmpeg 命令行程序打入产品。
- 上传前解压包并清除构建机的 FFmpeg 环境路径，经实际应用的 `--video-smoke` 接口对恒定帧率、可变帧率和旋转素材执行探测、向前及向后解码、RGBA 转换；不是仅查询编译特性。

## 错误信道审计

| 编号 | 分类 | 处理 |
| --- | --- | --- |
| FFREL-01 | 功能修复 | 发布任务、打包器能力声明及运行库分发保持一致，解决自动预览缺失视频后端。 |
| FFREL-02 | 新入口与原有日志 sink | 无窗口视频验收入口先记录 Info 开始；原始探测/打开/解码/转换错误补充 phase、source、PTS、index、耗时后由 main 的既有 Error 边界记录，JSON stdout 只返回成功结果。 |
| FFREL-03 | 真实错误验收 | 解压后的应用访问不存在的视频，要求非零退出、stdout 为空、默认 info 下文件与 stderr 均包含 Error 和原始原因；输出保存为验收产物。 |
| FFREL-04 | 工具错误 | 下载校验、平台/文件校验、签名、解压、进程超时与解码失败向 CI 返回失败，阻止上传；不使用重试或静默跳过。 |
| FFREL-05 | 正常分支 | FFmpeg 前后 seek、真实无音轨素材和正常 frame cache miss 不是错误。既有解码器行为和严格像素测试保持不变。 |

## 验证与边界

- `preview-video-tools-tests-complete.log`：CI 工具 50 项通过、1 项已有 Rust 效果验证器用例因未指定可执行文件而跳过（共 51 项）；包含下载缓存损坏及校验失败不发布的真实文件用例，以及 Windows 用户链接参数保留、Linux/macOS 相对库加载参数用例。
- `preview-video-package-final.log`：发布工具专项 15 项通过，覆盖 shell 构建拒绝、缺失组件/许可证/来源、哈希不符、路径越界、架构错误、归档内清单复核、macOS 签名失败及上传前验证顺序。
- `preview-video-prepare-monthly.log`：Windows 固定下载 SHA-256 验证、解压、原文许可证、7 个 DLL 清单和实际 `-buildconf` 验证通过；拒绝 enable-gpl / enable-nonfree 的非预期分发包。
- Windows `tools/build_desktop.py --task build --ffmpeg` 的正式 Release 构建通过，保留 thin LTO、单 codegen unit 和行号调试信息，耗时 18 分 13 秒；22 条现有警告仍保留。日志为 `preview-video-release-build.log`。
- `preview-video-build/desktop-build.json` 确认 `release=true`、`ffmpeg=true`。归档 `preview-video-release/MotionStudio-validation-windows-x86_64.zip` 为 83,434,108 字节，SHA-256 为 `9632736f09893e439b782c0a0b666dbb7334d2ab9dcf777079f28daf3b1d0dbd`，包含 7 个运行库 DLL 及来源/许可证记录。
- 实际运行 `verify_desktop_video.py` 通过：从归档解压，清除 FFmpeg 构建路径，三种素材各向前/向后解码 4 帧并转换 RGBA；旋转素材输出 144×256。结果保存在 `preview-video-runtime-verification/video-runtime.json`。
- 同一次归档验收访问真实不存在文件，非零退出、stdout 为空，默认 info 下文件与 stderr 均保留 `video.runtime`、`phase=probe`、源路径及 `cannot open video: No such file or directory` 原因。证据为 `failure.file.log` 和 `failure.stderr.log`。
- 解压后的正式程序还完成实际隐藏窗口 smoke，包含共享设备、浮窗、窗口 resize/redock、工程保存重开和工作区切换，两个报告均为 `ok=true`。传入 WGPU_BACKEND 的两次运行不证明窗口后端受该环境变量控制；只将其记为正式程序的实际窗口验收。DX12 单独 GPU 用例证据见 `wgpu-upgrade.md`。
- Linux 与两种 macOS 原生构建及加载验证需等待四平台 CI，不能用本地 Windows 结果代替。

本次不实现视频编码器，不恢复 Android 发布或 UI issue。已有 Windows BtbN 分发包含的依赖沿用上游 LGPL shared 配置；精确构建脚本与对应 FFmpeg 源码地址均保留在清单中。
