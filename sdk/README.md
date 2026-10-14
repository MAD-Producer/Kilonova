# Motion Studio 插件 SDK

面向第三方开发者的效果包契约、WGSL 接口、原生 UI 声明、模板和打包工具。宿主 JNI、前端接入与历史验收说明位于 [engineering](../engineering/README.md)。

SDK 提供可复制的 [motion-studio-plugin skill](motion-studio-plugin/SKILL.md)。将整个目录复制到所用工具的 skills 目录，即可用 `$motion-studio-plugin` 创建或修改效果，参考资料、脚本和模板随技能提供。

复制 `motion-studio-plugin/assets/effect-template`，修改插件 ID、版本、参数和 shader，然后运行：

```sh
python sdk/motion-studio-plugin/scripts/pack_msfx.py ./my-effect ./out/my-effect.msfx
```

需要 Python 3.10+ 和与宿主 SDK 匹配的 `effect_tool` 校验器。打包器先从 Rust 读取 manifest 的声明文件和大小限制，再验证生成包中的参数、PNG 和 WGSL/GLSL ES；任何校验失败都不会覆盖已有输出。修正算法或资源时提升版本，例如 0.2.0 → 0.2.1；同 ID/版本下字节或哈希不同的包会被安装器拒绝。安装新版本后显式升级测试实例，旧工程仍保留原版本和哈希引用。普通 `.msfx` 不要求重编译 App、JDK 或 Android 工程

指定 `--validator /path/to/effect_tool` 或 `MOTION_EFFECT_VALIDATOR`；也会自动查找 PATH 和仓库的 target/release、target/debug。如果本机没有二进制，在仓库中先运行：

```sh
cargo build --locked -p motion-effects --bin effect_tool
```

校验器缺失时打包会失败，不再退化为结构检查。复制 skill 到仓库之外时，也要提供校验器路径。新增宿主编译器、渲染器或原生控件实现仍需宿主构建和 CI

- [包与参数](motion-studio-plugin/references/package-format.md)
- [WGSL 接口](motion-studio-plugin/references/wgsl.md)
- [原生 UI 与共享组件](motion-studio-plugin/references/native-ui.md)
- [开发、测试与发布](motion-studio-plugin/references/development.md)

当前主分支支持 SDK 1–6；原生编辑页使用 SDK 5 / 协议 1。SDK 6 提供图层/素材输入（`image_input`），使用它时保留参数 slots 30/31。声明所需的最低 SDK 和能力，不依赖未发布接口；接口与字体复用见 [内置创作效果 API](../engineering/host-api/official-motion-effects.md)。
