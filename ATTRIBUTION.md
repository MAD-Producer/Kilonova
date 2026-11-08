# 第三方归属说明 / Third-party attribution

## Windows 私有驱动缓存目录的系统接口

- 作者：Microsoft Corporation 与 windows-rs contributors。
- 来源：官方 `microsoft/windows-rs`，`windows-sys` 0.61.2，revision
  `32c3144490c016fe496a0aed769bce60987a2e9d`，目录 `crates/libs/sys`。
  crates.io 校验和为 `ae137229bcbd6cdf0f7b80a31df61766145077ddf49416a728b02cb3921ff3fc`。
- 许可：MIT OR Apache-2.0；本次使用 MIT 选项，完整许可保留在 NOTICE。
- 修改范围：上游绑定不作修改。将锁文件已有的间接依赖显式用于 Windows
  目录创建、DACL/所有者验证及原生资源释放；策略、状态机和验收独立编写。
  不新增图像、图标、字体或上游示例实现。

## Mathematical references for optional WGSL depth research

- Authors: Milton Abramowitz and Irene A. Stegun (editors), US National Bureau
  of Standards; the original approximation credits C. Hastings, Jr. NIST DLMF
  and its contributors provide the separate power-series reference
- Sources/revision: AMS55, *Handbook of Mathematical Functions*, printed page 299,
  equation 7.1.26, identified by the
  [primary handbook facsimile](https://digital.library.unt.edu/ark:/67531/metadc40302/m1/315/ocr/);
  [NIST DLMF equation 7.6.1](https://dlmf.nist.gov/7.6.E1), accessed 2026-11-06
- Rights/scope: mathematical formulas and coefficient facts are references only.
  No upstream program, book prose, scan, figure, font or other asset is copied
  or distributed. The independently authored evaluation and tests use the
  repository MIT license; no broader rights in the reference works are claimed
- Modification scope: independent FP32 coefficient evaluation, a small-argument
  series and scalar numerical controls. The mathematical approximation bound
  does not establish actual WGSL/GPU rounding, model quality or performance

## COLMAP camera text interchange reference

- Authors: ETH Zurich, UNC Chapel Hill and the COLMAP contributors
- Sources: [public output format](https://colmap.github.io/format.html) and
  [pixel-coordinate FAQ](https://colmap.github.io/faq.html), pinned to
  [revision d0a411f2a9af139b5ea8118d64e5da5b70c6a890](https://github.com/colmap/colmap/tree/d0a411f2a9af139b5ea8118d64e5da5b70c6a890)
  (4.3.0.dev0, 2026-11-01), accessed 2026-11-04
- License: the pinned [upstream LICENSE](https://github.com/colmap/colmap/blob/d0a411f2a9af139b5ea8118d64e5da5b70c6a890/LICENSE)
  states the new BSD license with three conditions; upstream dependencies have
  their own licenses and are not used by this adapter
- Scope: independently authored bounded text parser, pose/calibration conversion
  and numerical fixtures; no upstream code, documentation prose, photographs,
  reconstruction, executable or extra dependency is copied or distributed
- The compatibility name identifies an interchange format, with no affiliation
  or endorsement claim. Unsupported camera models are rejected explicitly

## Photographed chroma-key API test fixtures

- Authors: Blender Foundation / Mango Open Movie Project contributors
- Source: [Tears of Steel raw footage announcement](https://mango.blender.org/production/4-tb-original-4k-footage-available-as-cc-by/),
  [upstream sharing terms](https://mango.blender.org/sharing/), and the Xiph mirror
  `https://media.xiph.org/mango/tearsofsteel-footage-exr/03_2a/linear/`
- Revision: numbered original EXR frames `03_2a_00500`, `03_2a_00501` and
  `03_2a_00502`; upstream supplies no commit revision. Per-original and derived
  SHA-256 identities are pinned in
  `crates/motion-desktop-media/tests/fixtures/chroma-key/source.json`
- License: [CC BY 3.0 Unported](https://creativecommons.org/licenses/by/3.0/),
  separate from the repository MIT license. Attribution and the legal-code link
  remain beside the fixtures. Upstream separately reserves actor portrait and
  privacy rights; the fixtures are technical validation, not advertising artwork
- Modification scope: read the unchanged uncompressed BGR HALF scene-linear
  Rec709 sources exactly, average 8x8 linear blocks from 4096x2160 to 512x270,
  clip to [0,1], encode IEC sRGB and round to 8-bit PNG. No other image edits.
  The originals and exact unclipped decoded channels remain external acceptance
  data. The three small derivatives are test inputs, not installed product assets
- Limits: source capture rate and PTS are unknown; any test composition clock is
  explicitly authored. These SDR derivatives do not qualify HDR import/output,
  full-shot temporal matte quality, or physical alpha ground truth
- The example chroma classifier, shader, core-region annotations, operation
  scripts and tests are independently authored under the repository MIT license

## External photographed camera calibration acceptance data

- Authors: OpenCV contributors; the sample photographs are distributed in the
  official OpenCV repository. Individual photographers are not named upstream.
- Source: `opencv/opencv`, tag `4.10.0`, commit
  `71d3237a093b60a27601c20e9ee6c3e52154e8b1`,
  [sample data](https://github.com/opencv/opencv/tree/71d3237a093b60a27601c20e9ee6c3e52154e8b1/samples/data):
  `left01.jpg` through `left09.jpg` and `left11.jpg` through `left14.jpg`.
- License: Apache-2.0, as distributed in the pinned repository's `LICENSE`;
  retain the unmodified license and any applicable upstream notices beside
  downloaded acceptance data. Per-file SHA-256 identities are recorded by the
  calibration validation tool.
  The unmodified upstream `COPYRIGHT` is also retained beside the license;
  copyright holders and dates remain exactly as provided upstream.
- Scope and modifications: external validation only, not shipped with the
  product, effect packages, or UI. Original JPEGs remain byte-for-byte unchanged;
  detected corners, calibrated parameters, annotated comparisons and replacement
  renders are separately identified derived acceptance artifacts. Physical square
  size is unknown, so all solves use checkerboard-square units, not surveyed
  metric coordinates. The images show separate board poses, not a continuous
  photographed camera-motion sequence.
- Tooling: separately installed `opencv-python-headless` 5.0.0.93 and NumPy
  2.4.6, already credited below; no product dependency is added. Calibration
  and rendering validation code is independently authored. The
  [official calibration tutorial](https://docs.opencv.org/4.10.0/dc/dbb/tutorial_py_calibration.html)
  documents the sample photographs and square-unit convention.

## 插件作者工作区：可选本机 WGSL 语言服务

- 作者：wgsl-analyzer contributors。
- 来源：[官方仓库](https://github.com/wgsl-analyzer/wgsl-analyzer)，验收固定版本
  `2026-09-30`，revision `659b96821595a83cbc06abf844461eb736caaab5`。
- Windows x86_64 官方发行资产 `wgsl-analyzer-x86_64-pc-windows-msvc.zip`，
  SHA-256 `f9b9c0621149fcf989907787daeab9212035a42af45d4511b6ec50834a7e6b05`。
- 许可：MIT OR Apache-2.0；使用未经修改的官方可执行文件，原始许可随本地验收目录保存。
- 范围：独立安装的可选子进程，通过公开 LSP 标准连接。上游源码、图标和二进制
  不写入本仓库，不嵌入效果包，也未加入正式发布流水线。客户端、虚拟文档映射、
  界面和故障夹具由本项目独立实现；没有复制 VS Code 扩展或其他编辑器界面资源。
- 标准来源：[Microsoft LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)。

本文件列出当前计划进入仓库或发布包的第三方材料。每个组件保留原始许可证和版权声明。根目录 [LICENSE](LICENSE) 只适用于按该许可证贡献的 Motion Studio 代码。

This file lists third-party material currently intended for the repository or a release. Each component keeps its original license and copyright notice. The root [LICENSE](LICENSE) applies only to Motion Studio code contributed under that license.

| 组件 / Component | 用途 / Use | 版权与来源 / Copyright and source | 许可证与声明 / License and notice |
| --- | --- | --- | --- |
| Motion Studio 源码 / Motion Studio source | Rust、UI、schema、测试和原创资产 / Rust, UI, schemas, tests, and original assets | Motion Studio contributors | MIT，见 [LICENSE](LICENSE) / MIT, see [LICENSE](LICENSE) |
| arboard 3.7.0 | 系统文本剪贴板，用于跨属性复用缓动定义；关闭图像功能 / System text clipboard for portable easing definitions; image features disabled | 1Password and arboard contributors; [v3.7.0](https://github.com/1Password/arboard/tree/5f5c8637911ba219edee619105a19b4f74e66a9f), exact package checksum in `Cargo.lock`; no source modifications | MIT OR Apache-2.0; upstream `LICENSE-MIT.txt` and `LICENSE-APACHE.txt` retained in the published crate |
| IBM Plex Mono Regular | 随应用分发的等宽字体 / Bundled monospace font | IBM Corp.; `google/fonts` revision `51303ca9e8ac9dcea7b12d307ba568fd0e6fcfca`, path `ofl/ibmplexmono/IBMPlexMono-Regular.ttf` | SIL Open Font License 1.1，见 [`crates/motion-media/fonts/OFL.txt`](crates/motion-media/fonts/OFL.txt) 和 [`crates/motion-media/fonts/SOURCE.json`](crates/motion-media/fonts/SOURCE.json) / SIL Open Font License 1.1, see the linked files |
| wgpu / Naga 30.0.1 | 官方 Cargo 依赖，用于 GPU 渲染、shader 转换与校验 / Official Cargo dependencies for GPU rendering and shader translation and validation | gfx-rs developers; [wgpu v30.0.1](https://github.com/gfx-rs/wgpu/tree/v30.0.1), exact package revisions and checksums in `Cargo.lock` | MIT OR Apache-2.0; upstream licenses retained in the published crates. 无本地源码补丁 / No local source modifications |
| Shader Effects 公式与参考 / Shader Effects formulas and references | 效果库使用的适配公式和独立 kernel / Adapted formulas and independent kernels used by the motion library | `shader-effects-inc/shaders`, revision `d49008cc2d6b547f216ca09bc13ae8f44572f040` | MIT；范围见 [`crates/motion-effects/motion-library/SOURCES.json`](crates/motion-effects/motion-library/SOURCES.json) 和 [`crates/motion-effects/motion-library/LICENSE.shaders.txt`](crates/motion-effects/motion-library/LICENSE.shaders.txt) / MIT; see the linked scope and notice files |
| FFmpeg desktop shared runtime | 视频导入与解码 / Video import and decoding | FFmpeg developers; Unix source `n8.1.3` (SHA-256 `09f990289327d3ebfedf4fc15cae3f884a65dab7a5aac7bbb5b01ff83900efd9`); Windows BtbN `autobuild-2026-09-30-13-08`, FFmpeg `n8.1.3-9-g29e619e767` (SHA-256 `3e47bda1607740550141e37c0e49d1e5182b34699f15adfd137ee266d346811a`); exact source/build links in `tools/prepare_ffmpeg.py` and bundled `FFmpeg-SOURCE.json` | Unix LGPL-2.1-or-later, Windows distribution LGPL-3.0-or-later. No source modifications; Unix external dependency autodetection/GPL/nonfree disabled. Shared libraries remain separately replaceable. Original license texts, configure arguments and per-file hashes accompany release archives. |
| libclang 18.1.1 (CI tool only) | Generate FFmpeg Rust bindings; not bundled in releases | LLVM contributors and Tao He; [libclang 18.1.1](https://pypi.org/project/libclang/18.1.1/), upstream LLVM 18.1.1; no modifications | Apache-2.0 WITH LLVM-exception; original notices in the installed wheel |
| cargo-audit 0.22.2 (CI tool only) | Audit the full Cargo.lock against current RustSec advisories; not bundled in releases | RustSec contributors; [release source revision 281452c35cf0870969042374110f099a411bc185](https://github.com/rustsec/rustsec/tree/281452c35cf0870969042374110f099a411bc185/cargo-audit); no source or binary modifications. Official Linux x86_64 archive SHA-256 `ab28a1bdb54db4d5d8ad5981cf1f959410370b3d28250dbd35f6a44248620e39`, Windows x86_64 archive SHA-256 `0a7316540862c13d954f648917ceacca593747baed6eec180fafa590be2710ab` | MIT OR Apache-2.0; upstream LICENSE-MIT and LICENSE-APACHE accompany the source. RustSec advisory data remains in the CI workspace; reports record its exact Git revision. |
| actions/cache (CI tool only) | Cache pinned libclang wheels and verified FFmpeg build inputs/runtime; not bundled in releases | GitHub and actions/cache contributors; [revision 0057852bfaa89a56745cba8c7296529d2fc39830](https://github.com/actions/cache/tree/0057852bfaa89a56745cba8c7296529d2fc39830) (v4); no modifications | MIT; [upstream license](https://github.com/actions/cache/blob/0057852bfaa89a56745cba8c7296529d2fc39830/LICENSE) |
| zlib 1.3.2 (Unix FFmpeg) | 解码压缩头 MOV；静态链接进 Unix libav，不依赖构建机 libz / Compressed MOV headers, statically linked into Unix libav | Jean-loup Gailly and Mark Adler; [official 1.3.2 source](https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz), SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`; no source changes, static PIC build | Zlib license; original `LICENSE` accompanies archives as `licenses/ffmpeg/zlib-LICENSE.txt`; source, revision, checksum and linkage in `FFmpeg-SOURCE.json` |

## Optional desktop depth prototype

The #75 static WGSL-candidate audit reuses this same pinned, external Small
export and the existing optional ONNX installation. It adds independently
authored metadata/inspection controllers, not another dependency, model or
upstream implementation. Shape folding evaluates only a bounded integer
constant program; no learned inference is executed by that audit. Its public
archives exclude the external model, weights, upstream source and image assets.
The follow-up standalone composed research adds independently authored WGSL
indexing, typed plan admission, lifetime allocation and runtime controllers.
Its archived static blueprint derives operation names, dimensions, attributes
and resource hashes from the same external Small export; it contains no learned
tensor payloads. Numerical references use locally authored normalized RGB;
neither those raw tensors nor model/runtime binaries are redistributed.

The optional WGSL host backend promotes independently authored shaders, strict
graph admission and execution code from that numerical qualification. Shared
RGB/letterbox/remap procedures and directly owned subprocess orchestration are
locally authored. The new worker ships no learned model/resource payload; static
plan/resource installation remains separate. Real host acceptance reuses the
already attributed NASA Destiny fixture with bounded crop, clockwise rotation
and mirroring, without adding image assets to product or release resources.
The native panic controls and twelve-frame crop observer are independently
authored Motion Studio test data under this repository's MIT license. Their
source hashes and base revision are recorded in the panic-retention evidence
manifest. The ordered crops reuse that same credited NASA photograph; public
evidence contains generated measurements and attribution, without image, model
or learned tensor payloads.

Depth Anything V2 Small is by Lihe Yang, Bingyi Kang, Zilong Huang, Zhen Zhao,
Xiaogang Xu, Jiashi Feng, Hengshuang Zhao and the upstream contributors. Reference:
[official source](https://github.com/DepthAnything/Depth-Anything-V2/tree/a561b849ebae10a6f5ef49e26c83cbbcd36c71bf),
revision `a561b849ebae10a6f5ef49e26c83cbbcd36c71bf`;
[official Small weights](https://huggingface.co/depth-anything/Depth-Anything-V2-Small),
SHA-256 `715fade13be8f229f8a70cc02066f656f2423a59effd0579197bbf57860e1378`.
Only Small's Apache-2.0 weights are supported by the export tool; the
noncommercial Base/Large/Giant models are not used. No model weights, reference
source or upstream images are bundled. The separately installed reference is
unmodified; the owned export procedure produces a fixed-shape opset-17 ONNX
artifact and retains its original license and provenance next to that artifact.
The owned exporter also offers explicit 266-, 392- and 518-square candidate
exports from those same pinned Small weights. Each export must independently
pass the unchanged CPU reference error bounds before publication. This changes
the export input shape, not the upstream implementation or its license, and
does not qualify a product quality tier or authorize default distribution.
The host's letterboxing, process protocol, scheduling and installation checks
are independently authored under this repository's MIT license.

The optional worker uses separately installed Python (Python Software Foundation,
PSF license), NumPy 2.4.6 (NumPy Developers, BSD-3-Clause,
[source](https://github.com/numpy/numpy)), opencv-python-headless 5.0.0.93
(OpenCV team and wheel contributors, Apache-2.0 and the installed wheel's
third-party notices, [source](https://github.com/opencv/opencv-python)), and ONNX
Runtime 1.31.0 (Microsoft and contributors, MIT,
[source](https://github.com/microsoft/onnxruntime)). These versions record the
validated environment, not bundled dependencies. Local model export additionally
uses unmodified PyTorch 2.14.1 (PyTorch contributors, BSD-3-Clause,
[source](https://github.com/pytorch/pytorch)) and ONNX 1.23.2 (ONNX contributors,
Apache-2.0, [source](https://github.com/onnx/onnx)). Their existing licenses and
notices remain in their external installations; no runtime binaries or packages
are redistributed by this change.

External depth validation also uses NASA's Eileen Collins portrait, distributed
as `skimage/data/astronaut.png` in scikit-image v0.25.2 (Git blob
`834cda0012478c5edc8d43bade96d315dedeaab4`). Author/credit: NASA; source:
[portrait data and public-domain record](https://scikit-image.org/docs/stable/api/skimage.data#skimage.data.astronaut).
The portrait is identified as public domain by the dataset and is used for
factual technical evaluation under NASA's
[media guidelines](https://www.nasa.gov/nasa-brand-center/images-and-media/),
with no endorsement or promotional use. Original downloads remain outside the
product tree; hashes, resizing and crop/translation modifications are recorded
in the external fixture manifest. No NASA image, logo or person is added to the
product UI, examples or release assets.

Natural-domain depth validation additionally uses NASA's Destiny laboratory
photograph `iss007e11800`, recorded 2003-08-03 by the Expedition 7 crew
(individual photographer not specified), and NASA Johnson Space Center's
`Step_Inside_the_Station-4K-Clean` tour, recorded by the ISS crew 2022-10-10
and published in the NASA library 2025-12-17. Sources: [photograph record](https://images.nasa.gov/details/iss007e11800)
and [tour record](https://images.nasa.gov/details/Step_Inside_the_Station-4K-Clean).
NASA is credited as the author/source. Usage follows the informational and
factual technical-evaluation permission in the [NASA media guidelines](https://www.nasa.gov/nasa-brand-center/images-and-media/),
with no endorsement or promotional use; separately identified third-party
content is not granted NASA's permission. Selected library metadata does not
identify a third-party copyright holder. The acquired source variants, hashes,
dates and exact extracted video timestamps are retained in the validation
manifest. Modifications are bounded frame extraction, RGB decoding and image
resizing for numerical/edge comparisons; no source audio is retained. Original
media remains external. Only credited evaluation frames and scalar results may
enter the technical evidence archive, never product UI, examples or releases.

The photographed screen API regression additionally retains a 960-by-654 RGB8
PNG derivative of `ISS007-E-11800` in the test-only fixture directory
`crates/motion-desktop-media/tests/fixtures/professional-screen`. NASA / the
Expedition 7 crew remain credited; the source revision, informational-use
guidelines, original and derivative hashes, explicit sRGB interpretation and
Lanczos resizing are recorded in its `source.json`. The manually authored
screen quad and replacement gradient are independent MIT test data. This is a
factual compositing evaluation, not NASA endorsement, product artwork or a
redistributed original photograph. The fixture is not used by product resources
or release packaging.

## Rust 依赖 / Rust dependencies

The read-only adapter memory probe directly uses two unmodified binding crates
already present in `Cargo.lock`: `ash` 0.38.0+1.3.281 by Maik Klein, Benjamin
Saunders, Marijn Suijten and ash contributors, from
[ash-rs/ash](https://github.com/ash-rs/ash), MIT OR Apache-2.0, package SHA-256
`0bb44936d800fea8f016d7f2311c6a4f97aebd5dc86f09906139ec848cf3a46f`;
and `objc2-metal` 0.3.2 by the objc2 contributors, from
[madsmtm/objc2](https://github.com/madsmtm/objc2), Zlib OR Apache-2.0 OR MIT,
package SHA-256
`a0125f776a10d00af4152d74616409f0d4a2053a6f57fa5b7d6aa2854ac04794`.
Their original licenses remain in the published crates. No binding source,
external assets or documentation text is copied or modified. These target-specific
direct declarations expose the same native adapter that wgpu already owns.

`motion-ui` uses the unmodified `unicode-segmentation` 1.13.3 crate for extended
grapheme boundaries during editing. Authors: kwantam, Manish Goregaokar and
unicode-rs contributors. Source: [unicode-rs/unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation),
crates.io release 1.13.3, package SHA-256
`c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8`.
License: MIT OR Apache-2.0; retain the crate's COPYRIGHT, LICENSE-MIT and
LICENSE-APACHE notices. No upstream source, tables or assets are modified or
copied into the repository. The lockfile already includes this transitive package;
this change declares its direct use by the UI crate without adding another version.

workspace 和传递 Rust 依赖保留各自作者发布的许可证 metadata。发布自动化必须根据 `Cargo.lock` 和包 metadata 生成完整的依赖声明；本表不能替代该清单。

Workspace and transitive Rust dependencies retain the license metadata published by their respective authors. Release automation must generate a complete dependency notice from `Cargo.lock` and package metadata; this table is not a substitute for that inventory.

## 字体与系统资源 / Fonts and system resources

用户选择的系统字体不会由 Motion Studio 重新分发。工程包会保存用户导入时提供的字体字节和许可证文本。新增内置字体前，必须先添加来源记录和完整许可证。

System fonts selected by a user are not redistributed by Motion Studio. A project package stores the selected font bytes and the license text supplied at import time. A new bundled font must add a source record and full license before it is committed.

## Adobe 与其他商标 / Adobe and other trademarks

在需要标识来源或兼容性边界时，Adobe 产品名可以出现在内部参考记录中。Motion Studio 资产不包含 Adobe logo、产品图标、截图、二进制文件、插件、预设或字体。商标免责见 [NOTICE](NOTICE)。

Adobe product names may appear in internal reference notes when identifying a source or compatibility boundary. No Adobe logo, product icon, screenshot, binary, plug-in, preset, or font is included as a Motion Studio asset. See [NOTICE](NOTICE) for the trademark disclaimer.


## Noise mathematical reference

Ken Perlin, *Improving Noise*, SIGGRAPH 2002, [author-hosted paper](https://mrl.cs.nyu.edu/~perlin/paper445.pdf), is a mathematical reference for quintic interpolation and gradient noise. No paper text, reference source code, permutation tables or third-party assets are redistributed. The 2D lattice hash, eight-direction gradients, bounded octave contract, WGSL kernel, CPU oracle and editable example are independently authored Motion Studio code under this repository's MIT license. This entry records a mathematical reference rather than an adapted or bundled third-party implementation.

## Color blending mathematical reference

W3C, *Compositing and Blending Level 1*, Candidate Recommendation Draft dated
2024-03-21, [fixed revision](https://www.w3.org/TR/2024/CRD-compositing-1-20240321/),
sections 6 and 10, defines the source-over and separable transfer mathematics used
as a reference for the node Blend operation. The document is under the W3C
permissive document license; no document text, illustrations, sample assets or
upstream implementation is bundled. Motion Studio's WGSL kernel, typed node,
CPU oracle and example are independently authored under the repository MIT
license. Bounded Add, Subtract and Divide transfers are arithmetic extensions,
not claims of W3C blend-mode conformance. No layer compositing behavior is changed.

## Scene ingestion dependencies and external references

The CPU-only `motion-scene` importer is independently authored from the public
[Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html),
document revision `8e798b02d254cea97659a333cfcb20875b62bdd4`. No specification text,
upstream loader implementation, screenshots or product assets are copied.

Three existing, locked registry components become direct dependencies of this
crate. Their unmodified implementations and notices remain in their packages;
the exact registry checksums are recorded in `Cargo.lock`.

| Component | Author and source revision | License | Modification scope |
| --- | --- | --- | --- |
| base64 | Marshall Pierce and contributors; [rust-base64](https://github.com/marshallpierce/rust-base64), 0.23.1 | MIT OR Apache-2.0 | Unmodified Cargo dependency for data URIs and portable node-image payloads; exact package checksum in Cargo.lock |
| percent-encoding | rust-url developers; [rust-url](https://github.com/servo/rust-url), 2.3.2 | MIT OR Apache-2.0 | Unmodified Cargo dependency for local URI decoding |
| sha2 | RustCrypto developers; [hashes](https://github.com/RustCrypto/hashes), 0.10.9 | MIT OR Apache-2.0 | Unmodified Cargo dependency for source/resource identity |

External validation uses unmodified `Avocado` (Microsoft, public 2017) and
`BoxVertexColors` (Marco Hutter, public 2023) from
[Khronos glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/8f492390ddc7a2ae221a1369ef560589cf8c70fb),
revision `8f492390ddc7a2ae221a1369ef560589cf8c70fb`. Both model-specific `LICENSE.md`
and metadata identify CC0-1.0. Original glTF/GLB files, buffers, textures and
licenses stay in the external validation directory. Source URLs and individual
SHA-256 values are recorded in its manifest. Only local, owned frozen bundles
and numerical reports are generated; no model or texture enters the product,
node examples or release artwork. This records technical evaluation, not
endorsement by the authors or Khronos.

Additional external normative evaluation uses unmodified `SimpleSkin` by Marco
Hutter (2017, CC0-1.0) and `AlphaBlendModeTest` by Ed Mackey / Analytical
Graphics, Inc. (2018, CC-BY-4.0), from the same pinned Sample Assets revision
`8f492390ddc7a2ae221a1369ef560589cf8c70fb`. Model-specific `LICENSE.md` and
`metadata.json` are retained alongside the external sources; the metadata and
license descriptions themselves are CC-BY-4.0. Their files remain outside the
repository's product assets. Evaluation may produce native pixel readbacks,
independently computed numerical references and exact frozen-restore reports;
these are validation evidence only. No upstream screenshot or source asset is
copied into the application, examples, icons or release artwork. The external
manifest records each source URL and SHA-256. A downloaded fixture is not a
claim that its rendering acceptance has passed.

The independently authored OBJ/MTL importer references Library of Congress,
Digital Formats descriptions [fdd000507 (OBJ)](https://www.loc.gov/preservation/digital/formats/fdd/fdd000507.shtml),
last significant revision 2025-02-18, and
[fdd000508 (MTL)](https://www.loc.gov/preservation/digital/formats/fdd/fdd000508.shtml),
last significant revision 2019-10-04, accessed 2026-10-26. These are reference
links only: no source text, loader code, illustration or model is copied or
redistributed, and rights to the references remain with their authors. Wavefront
names identify the documented interchange format, not a product affiliation.
The OBJ/MTL/glTF comparison geometry and PNG pixels used by the added tests and
local probe are authored by Motion Studio contributors and encoded locally;
they are covered by this repository's MIT license. No extra dependency or
third-party model/texture is introduced for OBJ.

The delayed-video import fixture
`crates/motion-media/tests/fixtures/video/video-delayed-with-audio-tail.mp4`
is authored by Motion Studio contributors under the repository MIT license.
It combines only the video and audio streams of the existing independently
generated `video-delayed.mp4` and `sound-24fps.mp4` fixtures. The author, local
FFmpeg revision, reproduction command, stream intervals, SHA-256 and scope are
recorded in that directory's `TIMING.md`. No third-party media or runtime is
added or changed; the fixture is test input only and is not product artwork.


## HarfRust 文本塑形内核（接入中）

作者：HarfBuzz developers、Yevhenii Reizner。来源：https://github.com/harfbuzz/harfrust 。
固定 crate 版本 0.15.0，发布源码 revision 71d965130ea93905f81b8f03a03d7ecc794fbc47，
crate SHA-256 865dcc4105d51c323d8fedbb2d75b5e9c8ecddf5613f74c82f851d0afbc97114。
许可 MIT，原文保留于 NOTICE。上游源码未修改；本项目独立编写字体绑定、
有界运行布局、缓存与测量/绘制集成。系统字体只在当前机器读取，不复制或随产品分发。
此项接入尚未完成，不代表组合标记、双向文本、emoji 或整个 #129 已验收。

传递依赖：Fontations Developers，read-fonts 0.46.0（revision
47a301615fd95acf0da9fb7dbcd0ea7c7df36943）和 font-types 0.12.7（revision
ef666662f5985965bbbb726c4f77d15481831a1b），来源
https://github.com/googlefonts/fontations ，MIT OR Apache-2.0，本次按 MIT 使用，
许可原文保留于 NOTICE；未修改上游源码。确切 crate 校验值保留在 Cargo.lock。


## Unicode 双向文本算法（UI 布局接入中）

作者：The Servo Project Developers；上游 LICENSE-MIT 版权行为 The Rust Project Developers。
来源：https://github.com/servo/unicode-bidi ，固定版本 0.3.18，revision 580b9c6c68fdb689916017e61d5052be3ed349b5，
crate SHA-256 5c1cb5db39152898a79168971543b1cb5020dff7fe43c8dc468b0885f5e29df5。MIT OR Apache-2.0，本次按 MIT 使用，
原许可和 COPYRIGHT 说明保留于 NOTICE。上游源码未修改；仅调用双向级别与运行
排序 API，与项目独立编写的字素/字体/脚本分段和塑形布局组合。不新增或分发字体
及第三方界面资产；接入模块不代表产品绘制或整个 #129 已完成。

## OpenColorIO 数值参考工具（仅开发验收）

后续配置目录导出同样使用该固定外部运行时，显式选择 CG 配置
`cg-config-v2.2.0_aces-v1.3_ocio-v2.4`、
`cg-config-v4.0.0_aces-v2.0_ocio-v2.5` 或
`cg-config-v5.0.0_aces-v2.1_ocio-v2.6`。原生配置与目录仅在显式的本地
导出/验收目录生成，不作为产品资产或工程默认配置提交。后续 GPU 处理器验收
将这三份原生序列化配置及生成的 GLSL/数值表保存在
`crates/motion-effects/tests/color_processor/reference.json`，仅供测试，不接入
应用默认配置。对应固定配置 ID 如上，版本及原始配置字节 SHA-256 保存在
夹具内的 manifest。生成源码和表未手工修改；输入色卡和容差独立编写。
`crates/motion-effects/tests/color_processor/chain_reference.json` 使用同一
固定 OCIO 2.6.0 运行时和上述三份原始配置生成 14 组两阶段 CPU 数值参考。
不修改原始配置、着色器或数值表；每阶段按实际 Float32 及透明度声明
计算后保存结果，并记录原参考 JSON 的 SHA-256。该派生数据仅用于验收，
不进入产品默认资源；生成与信号身份/纹理绑定代码均独立编写。
内容身份、目录验证、
选择解析及故障夹具由本项目独立编写；不复制 OCIO 实现代码。

作者：Contributors to the OpenColorIO Project。来源：
https://github.com/AcademySoftwareFoundation/OpenColorIO ，官方 Python 包
https://pypi.org/project/opencolorio/2.6.0/ ，固定发行版本 v2.6.0。
本次本地参考运行使用官方 `opencolorio-2.6.0-cp311-cp311-win_amd64.whl`，
SHA-256 `2d9662abd4551e714fa7480443b36ee300ef6e1169d65b92188b36983802ea21`。
许可 BSD-3-Clause，原文保留于 NOTICE。未修改上游代码，未把 OCIO 运行时、
二进制或上游 LUT 打包进产品。处理器适配使用现有官方 Naga 30.0.1 的
`glsl-in`/`wgsl-out` 特性，不修改 Naga 或 OCIO 上游源代码。

`tools/color-management/generate_lut_reference.py` 和
`crates/motion-effects/tests/color_lut/*.cube` 由 Motion Studio 独立编写，
使用仓库 MIT 许可。`reference.json` 的线性和四面体预期值由上述官方
CPU 文件处理器测得；最近邻预期值来自独立的网格距离穷举参考，另保留
官方处理器对最近邻提示返回的实际结果，不将其线性结果冒称为最近邻。
Cube 兼容字段按公开 OCIO 文件变换接口及自建数值样例独立实现；未复制
商业产品的格式说明、代码或素材。用途、插值和域范围均由调用方显式指定。

`export_processor.py` 的数组读取使用独立验收环境中的官方 NumPy 2.3.5，
作者 NumPy Developers，来源 https://pypi.org/project/numpy/2.3.5/ ，BSD-3-Clause。
本地官方 cp311 Windows wheel SHA-256 为
`a414504bef8945eae5f2d7cb7be2d4af77c5d1cb5e20b296c2c25b61dff2900c`。
未修改、未复制 NumPy 源码或二进制进产品。

Naga GLSL 前端新增的间接官方 Cargo 依赖：

| 依赖 | 作者与来源 | 固定版本与许可 | 修改范围 |
|---|---|---|---|
| pp-rs | Corentin Wallez / pp-rs Developers，https://github.com/Kangz/glslpp-rs | 0.2.1，BSD-3-Clause；精确 registry 校验和见 Cargo.lock，许可原文见 NOTICE | 无上游改动 |
| unicode-xid | unicode-rs contributors，https://github.com/unicode-rs/unicode-xid | 0.2.6，MIT OR Apache-2.0；精确 registry 校验和见 Cargo.lock，随包保留许可 | 无上游改动 |
