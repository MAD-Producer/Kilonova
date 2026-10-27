# Motion Studio 来源说明 / Provenance

**状态 / Status:** 仓库政策与发布证据 / Repository policy and release evidence
**生效日期 / Effective date:** 2026-10-14

曲线剪贴板采用官方 arboard 3.7.0 的系统文本接口，不修改上游源码，不引入图像或字体资产。曲线 JSON、单位契约、异步命令和界面文案由本项目独立实现。来源、固定 revision 和许可见 ATTRIBUTION.md；随发布包分发的 NOTICE 保留 arboard 的 MIT 许可文本。

The optional desktop depth prototype uses independently authored host/process
code and a separately installed, hash-verified Apache-2.0 Small model. The export
tool requires the pinned official reference checkout and retains its original
license and a model manifest next to the external ONNX artifact. No upstream
source, weights, images or inference runtime enter project/effect packages or
the shipped product. Authors, revisions, hashes and modification scope are
recorded in [ATTRIBUTION.md](ATTRIBUTION.md).

## 中文

Motion Studio 是独立的 clean-room 实现。项目可以研究公开行为和公开记录的工作流，但不会复制 Adobe 源代码、二进制文件、资源、图标、指针、字体、音频、预设、模板、截图或专有文件数据。

允许参考公开用户指南、公开脚本/API 文档、公开快捷键说明、公开标准、论文、社区讨论，以及根据公开行为独立编写的测试用例和测量结果。这些资料只用于说明预期行为或交互模式，不构成复制产品素材、源代码、二进制资源或专有实现细节的许可。

本仓库中的 Rust、桌面 UI、Android UI、效果 kernel、schema 和测试均为 Motion Studio 编写。效果库在 `crates/motion-effects/motion-library/SOURCES.json` 中记录适配公式和独立 kernel，其中不包含上游预设素材或上游网页引擎。新功能使用 Motion Studio 自己的名称、图标、颜色、字体、布局 token、文件格式和包标识。

旧版 Motion Studio 数据会标记为 legacy。除非另有独立文档、测试和授权兼容层，项目不宣称提供 Adobe 插件或打开 Adobe 工程文件。

每项新增第三方资产或适配实现，都必须在 [ATTRIBUTION.md](ATTRIBUTION.md) 中记录作者、来源 revision、许可证和改动范围。Pull Request 必须说明新增字体、图标、图像、音频、shader、预设、模板、示例工程和生成数据的来源。没有验证来源的材料不得进入发布产物。

本文件记录仓库的工程流程，不构成法律意见，也不能替代对具体司法辖区、资产或分发协议的审查。

## English

Motion Studio is an independent, clean-room implementation. The project may study public behavior and documented workflows, but it does not copy Adobe source code, binaries, resources, icons, pointers, fonts, audio, presets, templates, screenshots, or proprietary file data.

Permitted references include public user guides, public scripting/API documentation, public shortcut references, public standards, papers, community discussions, and independently created tests or measurements based on documented behavior. These references describe expected behavior or interaction patterns; they do not grant a license to copy product assets, source code, binary resources, or proprietary implementation details.

Rust, desktop UI, Android UI, effect kernels, schemas, and tests in this repository are authored for Motion Studio. Adapted formulas and independent kernels are recorded in `crates/motion-effects/motion-library/SOURCES.json`; upstream preset assets and upstream web engines are not included. New features use Motion Studio names, icons, colors, typography, layout tokens, file formats, and package identifiers.

Legacy Motion Studio data is labeled as legacy. Unless a separately documented, tested, and licensed compatibility layer exists, the project does not claim to ship Adobe plug-ins or open Adobe project files.

Every new third-party asset or adapted implementation must record its author, source revision, license, and modification scope in [ATTRIBUTION.md](ATTRIBUTION.md). Pull requests must explain the provenance of new fonts, icons, images, audio, shaders, presets, templates, sample projects, and generated data. Unverified material is not accepted into release artifacts.

This file records the repository's engineering process. It is not legal advice and does not replace review of a particular jurisdiction, asset, or distribution agreement.

The CPU scene importer follows the public Khronos glTF 2.0 document and retains
owned, content-identified resource bytes. Its implementation and adversarial
fixtures are independently authored. External Khronos validation models are
pinned and credited in `ATTRIBUTION.md`; they are not bundled with the product.

The static OBJ/MTL profile is independently implemented from public format
descriptions listed in `ATTRIBUTION.md`. It does not incorporate an upstream
loader or a commercial application's binary resources. New OBJ/MTL fixtures,
comparison geometry and PNG pixels are locally authored numerical test data.
Wavefront's classic material descriptors remain distinct from glTF PBR, with
unsupported directives explicitly rejected rather than silently translated.
