# Motion Studio 来源说明 / Provenance

Optional WGSL-depth numerical research references the mathematical error-function
approximation in AMS55 equation 7.1.26 and the NIST DLMF 7.6.1 series. Evaluation
and tests are independently authored. No reference implementation, scan, figure
or prose is copied. Attribution records the exact mathematical references and
the distinction between scalar mathematics and unqualified GPU execution.

The #75 WGSL-depth checkpoint contains independently generated operator/shape
and static-memory metadata for the already attributed external Small model.
Its evidence archives contain no model weights, upstream implementation or
image assets. Static inspection and integer shape folding do not qualify a
second inference backend or authorize default model distribution.
The subsequent optional host backend uses independently authored WGSL kernels,
strict typed admission and directly owned child processes. It shares the
existing owned color/letterbox/remap procedure; no reference implementation or
model/weight/runtime binary is copied into the product. Its plan is an explicit
pinned bootstrap profile, not a general SDK compute capability. Numerical and
cancellation evidence does not authorize default model distribution or imply
interactive/video quality acceptance.

The COLMAP camera text adapter references the public format and coordinate
conventions at a pinned upstream revision. Its parser, pose conversion boundary
and numerical fixtures are independently authored. It does not embed COLMAP or
copy upstream code or datasets. Reference author, revision, license and scope
are recorded in ATTRIBUTION.md.

The test-only photographed screen replacement fixture reuses the previously
attributed NASA Destiny laboratory image. Its local source record pins both
the original and resized RGB derivative and credits NASA / the ISS crew.
Authoring and independently calculated compositing references are separate from
the photograph. No NASA image is added to product UI or release resources.

The chroma-key API acceptance fixtures are three explicitly converted SDR
derivatives of Blender Foundation / Mango footage (CC BY 3.0), with source
hashes, license, modifications and technical-only scope in ATTRIBUTION.md and
the fixture directory. They are not product artwork or an HDR import claim.

External photographed planar-tracking qualification uses the Zimmermann/Matas/
Svoboda benchmark's published overview videos, original named images and
independent corner annotations under its
documented custom public-use statement. These materials remain local validation
inputs, not repository/product assets. No upstream MATLAB implementation is
copied or executed. Source revisions, permission limits and citation are recorded
in ATTRIBUTION.md; each acquisition retains exact URLs and SHA-256 identities.

The optional #88 point/planar tracking prototype interoperates with explicitly
selected local FFmpeg tools and separately installed OpenCV/NumPy. It copies no
upstream tutorial implementation or assets, adds no product dependency and
records executable hashes, raw source identity and original frame timestamps.
Authorship, revisions, licenses and modification scope are in ATTRIBUTION.md.
The frozen synthetic video and measured solve under
`desktop/app/tests/fixtures/tracking-prototype` are independently authored MIT
validation outputs, with exact source revision and hashes in their README.
They contain no third-party photographed benchmark data or product artwork.

The external #88 camera calibration qualification uses pinned OpenCV sample
photographs under Apache-2.0, with unmodified license/copyright and file hashes
retained. Derived corner measurements and independently authored replacement
renders are acceptance data, not product assets. Sources, revision, tools, scale
limitations and modification scope are recorded in ATTRIBUTION.md.

插件作者工作区的可选 WGSL 辅助通过公开 LSP 标准连接独立安装的
wgsl-analyzer。客户端及其故障夹具独立编写，不复制上游编辑器控件、图标或截图。
固定验收版本、官方资产 hash、许可及修改范围记录在 ATTRIBUTION.md；
正式发布包尚未捆绑或自动安装语言服务器。

**状态 / Status:** 仓库政策与发布证据 / Repository policy and release evidence
**生效日期 / Effective date:** 2026-10-14

曲线剪贴板采用官方 arboard 3.7.0 的系统文本接口，不修改上游源码，不引入图像或字体资产。曲线 JSON、单位契约、异步命令和界面文案由本项目独立实现。来源、固定 revision 和许可见 ATTRIBUTION.md；随发布包分发的 NOTICE 保留 arboard 的 MIT 许可文本。

The optional desktop depth prototype uses independently authored host/process
code and a separately installed, hash-verified Apache-2.0 Small model. The export
tool requires the pinned official reference checkout and retains its original
license and a model manifest next to the external ONNX artifact. No upstream
implementation is modified by the optional 266/392/518 export selection. Those
independently authored candidates retain distinct shape/processing identities
and require their own original numerical reference gates before publication;
export availability alone does not qualify a product quality tier. No upstream
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

Native depth panic fixtures and the ordered CPU/WGSL crop observer are locally
authored validation work. The observer transforms an existing attributed NASA
fixture; it adds no third-party image or model asset. Public evidence records
source revisions, bounded failure context and numerical results while excluding
raw images, learned tensors, external model resources and optional runtimes.

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

The bounded Cube parser, CPU lookup, WGSL interpolation and synthetic color
fixtures are independently authored. OpenColorIO 2.6.0 is an external numerical
reference only, pinned and credited in ATTRIBUTION.md; its runtime and upstream
assets are not shipped. Linear/tetrahedral references come from its real CPU
processor. Nearest references use a separately identified exhaustive grid oracle,
retaining the actual OCIO output where its CPU processor resolves the hint to
linear. A LUT does not imply an OCIO configuration, ACES version or display role.

The OCIO catalogue preparation tool uses the same separately installed official
runtime to export explicitly versioned built-in configuration bytes and metadata.
Its output remains an explicit external preparation artifact, not a bundled
configuration or automatic project default. Content verification and catalogue
selection are independently authored. They do not parse OCIO syntax, compile
processors or qualify the application's complete display/output chain.

The subsequent explicit GPU preparer exports official OCIO 2.6 GLSL and numeric
tables. Three versioned configuration serializations and their generated GPU/CPU
references are retained in the attributed test fixture only; none is an implicit
application configuration. The separately authored adapter flattens equivalent
RGB component expressions, translates declared 2D table reads into explicit
buffer interpolation, and uses the official Naga public IR to guard dispatch
bounds before normal validation/WGSL output. No upstream library source patch,
arbitrary 3D LUT approximation, or commercial binary content is used. This layer
does not establish the final image, decode or OS display workflow.
