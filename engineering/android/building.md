# Android 构建入口

Android 端已停止维护。以下步骤仅保留为历史实现的手动入口，不再由 CI 构建、在模拟器执行或自动发布 APK。后续开发集中于桌面端，见 [桌面使用说明](../../desktop/README.md)。

推荐从仓库根目录运行 `python tools/build_android.py --task assembleDebug`。脚本先从当前 Rust 源码生成原生库，再调用 Gradle；干净克隆不包含 `jniLibs/*.so`。

Windows 可先运行 `python tools/bootstrap_android.py` 安装项目工具链，并准备 Rust MSVC 所需的 C/C++ 构建工具。其他环境需提供 Java 21、Gradle 8.11.1、Android SDK 35、Rust Android targets 与 NDK。

NDK 默认版本为 `27.0.12077973`，与 CI 一致。共享 `.tools/environment.json` 保存已安装工具的位置；`ANDROID_NDK_HOME` 可覆盖 NDK 路径，使用非默认版本时需自行验证兼容性。脚本同时设置交叉编译器、链接器及 16 KiB ELF 页面对齐。

## 单 ABI 与直接 Gradle

```sh
python tools/build_android.py --abis x86_64 --task assembleDebug
```

脚本将所选 ABI 传给 Gradle 的 `motionAbis`。直接 Gradle 默认要求 `arm64-v8a,x86_64`，若只生成了一种 ABI，必须明确指定相同集合：

```sh
python tools/build_android.py --abis x86_64 --rust-only
cd android
gradle --no-daemon -PmotionAbis=x86_64 assembleDebug
```

`verifyMotionNativeLibraries` 检查每个所选 ABI 的库文件与 ELF 类型/架构，所有 APK 变体的 native merge 都依赖它。缺少库、空文件或架构错误会在打包前给出标准构建命令。Kotlin 编译和 JVM 单元测试不因此要求 JNI 库。该检查不证明库来自当前提交；正式交付仍必须使用先编译 Rust 的标准脚本及 CI，不能拿旧库冒充新构建。

## 包名、签名与诊断

- 默认 debug 保持现有包名，是否能替换已安装包取决于签名、版本与安装参数
- `--isolated-debug` 使用 `com.motionstudio.editor.dev`，供开发包与正式包同时安装；它仅用于 debug，不能与 `--effects-acceptance` 混用
- `--effects-acceptance` 保持既有 `com.motionstudio.editor.effectsacceptance` 和自动化测试 runner
- release 使用显式配置的自动化签名；未配置时 Gradle 产出未签名 release，不应作为可安装交付
- preview 优先使用自动化签名，无该配置时使用 debug 签名，供测试分发
- benchmark 使用 debug 签名，仅用于性能测试
- release、preview、benchmark 禁止脚本启用 GPU 故障注入诊断；R8 当前关闭，未来开启之前需要 JNI keep 与运行回归

原生库、构建缓存、签名材料保持 Git 忽略，不能加入提交。
