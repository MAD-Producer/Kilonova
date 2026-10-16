# Motion Studio

Motion Studio 是一个面向桌面的开源动效编辑器。它用图层、关键帧、曲线、节点和 GPU 合成组织创作流程，目标是让动态设计、合成效果和实时预览在同一份工程中协同工作。

项目目前处于持续开发阶段，后续开发和自动验证集中于 Windows、Linux 与 macOS 桌面端。桌面端已经具备可用的图层编辑、时间轴、曲线和节点效果流程。Android 端已停止维护，历史源码和手动构建工具保留，自动构建、设备测试及 APK 发布已停用。

## 当前能力

| 领域 | 已提供 | 仍在完善 |
| --- | --- | --- |
| 图层与时间轴 | 图层层级、片段移动、裁剪、分割、父子关系、混合模式、轨道遮罩 | 更完整的媒体管理和高级时间轴工作流 |
| 动画与曲线 | 位置、锚点、缩放、旋转、不透明度和效果参数关键帧；值曲线与速度曲线 | 更多曲线编辑手柄、表达式引用和复杂缓动 |
| 节点效果 | 桌面节点画布、类型检查、连线、分组、编译、实时预览、绑定图层效果 | 多 pass 节点图、自定义 WGSL 工作流和移动端节点图编辑 |
| 效果库 | 内置效果包、分类搜索、参数动画、`.msfx` 导入导出 | 更完整的效果分类、预览和第三方包管理 |
| 摄影机与 3D | 2D/3D 图层、摄影机、父子关系、顶视和侧视 | 更完整的空间控制器与摄影机工具 |
| 媒体 | 图片、视频和音频素材导入；Android 预览与 H.264 MP4 导出 | 桌面音频输出和桌面 MP4 编码 |
| 自动化 | 桌面 MCP、命令批次、revision 检查、撤销和只读会话 | 模型供应商和聊天面板接入 |

效果和节点实现都是 Motion Studio 自己的代码。部分参数和视觉行为仍是近似实现，具体边界见[内置效果与字体 API](engineering/host-api/official-motion-effects.md)和[节点创作方案](desktop/NODE-AUTHORING.md)。

## 快速开始

### 获取桌面预览版

从 [Preview 发布页](https://github.com/MAD-Producer/motion-studio/releases/tag/preview) 按 Windows、Linux、macOS Apple Silicon / Intel 选择桌面包，或在[全部发布](https://github.com/MAD-Producer/motion-studio/releases)中选择版本。预览版适合体验和反馈，发布页中的 `SHA256SUMS` 与 `build-info.json` 可用于核对下载文件。

### 构建桌面端

需要 Rust 1.97.1、Python 3.10+，Windows 还需要 MSVC 工具链。默认构建包含工程编辑和 GPU 预览；桌面视频解码需要额外安装 FFmpeg 开发库并启用 `--ffmpeg`。

```powershell
git clone https://github.com/MAD-Producer/motion-studio.git
cd motion-studio
py tools/build_desktop.py --task build
./target/release/motion-studio.exe --locale zh
```

桌面端的启动参数、布局、MCP 和恢复副本见[桌面使用说明](desktop/README.md)。

### 历史 Android 实现的手动构建

以下工具保留供手动使用，Android 端不再参与 CI 或自动发布。

```powershell
py tools/bootstrap_android.py
py tools/build_android.py --task assembleDebug
```

工具链默认放在 `.tools`，APK 输出到 `android/app/build/outputs/apk/debug/app-debug.apk`。只构建模拟器版本时可以增加 `--abis x86_64`。

开发包需要与正式包同时安装时可加 `--isolated-debug`，包名使用 `.dev` 后缀。直接运行 Gradle 打包前必须先生成所选 ABI 的原生库；NDK 版本、签名与单 ABI 构建说明见 [Android 构建说明](engineering/android/building.md)。

## 工作区

桌面工作区由项目、合成、效果控件、节点编辑器和时间轴组成。面板支持缩放、停靠、拆分浮窗和布局保存。

- `Ctrl+S` 保存，`Ctrl+Z` 撤销，`Ctrl+Shift+Z` 重做。
- `Space` 播放，`Ctrl+左右方向键` 或 `PageUp/PageDown` 逐帧。
- `P/S/R/T` 显示位置、缩放、旋转和不透明度；`U` 显示动画属性。
- `Shift+F3` 在时间轴中切换值曲线；`F9` 应用平滑缓动。
- `Ctrl+Space` 打开 FX Console；`Shift+A` 在节点画布中添加节点。
- 节点画布使用中键平移，`Ctrl+滚轮` 以鼠标位置为中心缩放，`Home` 适配全部节点。

完整快捷键见[快捷键列表](desktop/KEYBOARD-SHORTCUTS.md)，节点交互见[节点编辑器交互说明](desktop/NODE-INTERACTIONS.md)。

## 工程、效果与节点文件

| 扩展名 | 用途 |
| --- | --- |
| `.msproj` | 包含素材、字体和效果依赖的 Motion Studio 工程 |
| `.msgraph` | 可编辑的节点源文件 |
| `.msfx` | 可安装或导出的效果包 |
| `.aem` | 旧版 Motion Studio 工程包，仅用于迁移读取 |

节点图可以单独保存，也可以绑定到图层效果。绑定后的节点效果会参与合成预览，并在保存工程时保留最后一次有效编译结果。节点图的文件格式、编译边界和移动端执行方式见[节点创作方案](desktop/NODE-AUTHORING.md)与[节点学习库](crates/motion-graph/README.md)。

效果开发请从 [插件 SDK](sdk/README.md) 开始。SDK 提供参数协议、WGSL 接口、原生控件声明、模板和 `.msfx` 打包工具；打包时需要 Rust `effect_tool` 校验器。

## 媒体与字体

媒体后端支持常见 MP4、MOV、MKV、WebM 视频和 AAC、MP3、FLAC、ALAC、Vorbis、Opus、AIFF、WAV 音频。实际格式取决于平台解码器、设备能力和构建选项，完整范围见[媒体格式](crates/motion-media/FORMATS.md)。

系统字体可以直接用于编辑，用户导入的 TTF、OTF、TTC 会随工程记录来源和许可证。项目内置 IBM Plex Mono，并保留其 SIL Open Font License 1.1 文本。

## MCP 与自动化

桌面程序提供三种 MCP 入口：

- `--mcp`：无窗口工程会话，适合脚本和自动化任务。
- `--mcp-ui`：打开编辑器，同时共享工程、播放头和撤销历史。
- `--mcp-read-only`：只读取编辑器状态。

编辑命令支持 revision 检查、原子批次、撤销和保存。配置方式和工具参数见[MCP 接入说明](desktop/MCP.md)。

## 开发与验证

主要目录：

| 路径 | 内容 |
| --- | --- |
| `crates/motion-model` | 工程数据、序列化、曲线和父子关系 |
| `crates/motion-core` | 命令执行、撤销、编辑事务和表达式 |
| `crates/motion-effects` | 效果包、WGSL 和参数协议 |
| `crates/motion-render` | 场景采样、GPU 合成、预览和 PNG 输出 |
| `crates/motion-media` | 素材、缓存、音频和媒体任务 |
| `crates/motion-host` | 共享编辑、渲染、效果和媒体会话 |
| `desktop` | Rust 桌面应用、原生面板和节点编辑器 |
| `android` | 已停止维护的 Kotlin、Compose、SurfaceView、JNI 和 MediaCodec 历史实现 |

常用检查：

```powershell
python tools/validate_provenance.py
cargo test --workspace --exclude motion-android --locked -- --test-threads=1
```

桌面专项测试：

```powershell
cargo test -p motion-desktop -p motion-ui -p motion-host -p motion-desktop-media --locked
```

历史 Android 设备验证（手动，不属于 CI）：

```powershell
py tools/build_android.py --task assembleDebug assembleDebugAndroidTest
py tools/validate_android.py --serial <adb-device-serial>
```

CI 覆盖共享 Rust/GPU、Windows/Linux/macOS 桌面构建、桌面原生交互和媒体后端；Rust 工作区验证排除 `motion-android`。桌面预览版和版本标签发布直接验证四个平台的桌面包，不依赖 APK。需要真实设备、长时间运行或特定 GPU 的检查，会在对应验证文档中单独记录。

## 参与开发

请从最新 `main` 创建分支，通过 Pull Request 提交改动。提交前说明：

1. 改动解决了什么问题；
2. 哪些平台和交互受到影响；
3. 运行了哪些验证；
4. 新增依赖、字体、图标、shader 或素材的来源和许可证。

分支规范、clean-room 规则和来源要求见 [CONTRIBUTING.md](CONTRIBUTING.md)、[PROVENANCE.md](PROVENANCE.md) 和 [ATTRIBUTION.md](ATTRIBUTION.md)。界面色板、字号、密度、组件契约与对比度基线见 [DESIGN.md](DESIGN.md)。

## 许可证与商标

Motion Studio 自有代码采用 [MIT License](LICENSE)。第三方依赖、字体和其他素材保留各自许可证。来源记录见 [ATTRIBUTION.md](ATTRIBUTION.md)，法律声明见 [NOTICE](NOTICE)。

本项目独立开发，不隶属于任何第三方软件厂商。第三方产品名称只用于必要的参考或兼容性说明，不代表赞助、认证或授权关系。
