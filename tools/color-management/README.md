# 色彩管理准备与数值验收

这是 #83 的底层 LUT 验收工具，不是应用的运行时依赖，也不是 OCIO 配置加载器。
应用与 CI 测试只消费提交的独立 Cube 数值样例及参考 JSON，不要求安装 Python
或 OpenColorIO。原生 Rust/WGSL 实现不链接 OCIO。

在独立 Python 环境安装官方 `opencolorio==2.6.0` 后运行：

```text
python tools/color-management/generate_lut_reference.py --output crates/motion-effects/tests/color_lut
```

生成器严格检查参考版本，输出中记录平台、Python 版本、原生模块 SHA-256、
Cube SHA-256 和明确的参考路径。六组线性/四面体结果来自真实 OCIO CPU
FileTransform；四组最近邻结果采用独立的网格距离穷举，等距取较大索引。
OCIO 2.6.0 CPU 对本批 1D/3D 文件的 NEAREST 提示实际输出线性结果，因此
JSON 同时保留 `original_file_expected`，不能把这四组结果宣称为 OCIO 最近邻。
输入含不对称域、端点、域外值、分数网格、负值、高亮及零/小透明度。

## 当前底层 API

`motion_effects::color_lut::ColorLut::from_cube` 要求显式选择插值和用途；
支持单个 1D 或 3D Cube，不猜测混合 shaper、颜色空间或配置角色。
`to_cube` 保真导出域和样本；用途和插值不属于 Cube 数据，重载时仍须提供。
语义身份包含尺寸、种类、插值、用途、域和样本的浮点字节。

- 输入不超过 16 MiB，每行最多 512 字节；1D 为 2–65536 项，3D 边长 2–65。
- 域必须有限、递增且宽度在 Float32 正规数范围；样本留有 Float32 运算余量。
- 域外输入夹到域端点；输出保留负值与高亮，不做显示编码或色调映射。
- CPU `apply_rgba` 先验证全部有限像素和 [0,1] 透明度，再原地变换；失败不写入。
- `motion_render::LutOutput` 使用已有资源准入和编码器，在 Float16/Float32
  色彩图像间执行真实 GPU 映射。相同大小、不同源/目标，声明必须匹配。
- 输入与输出分别声明直通或预乘透明度；预乘输入先反关联，输出按声明重新关联。
  零透明度预乘源以 RGB=0 作为查询值；直通目标仍接收该查询的 LUT 结果。
  浮点 GPU 像素由生产者保证有限值和有效透明度；这里不做逐帧同步读回验证。
- 不自动转换输入/输出空间标签。深度、法线、覆盖率及其他数值图像明确拒绝，
  由调用方保留其原路径；Float16 目标拒绝超出有限半精度范围的 LUT 样本。
- LUT 缓冲区与管线只在创建时建立；复用编码不重新分配。尚未提交及在途命令
  继续保留准入额度，直到真实工作完成；原生 GPU 创建原因通过 Err 返回。
  创建可能等待原生编译，宿主接线必须放进已有非 Present 准备操作，在进入前
  发布 start/in-flight；不能在绘制或健康 Present 热路径中同步创建。

当前没有插件节点、面板导入菜单或工程色彩设置接线。#83 的 OCIO 配置内容身份、
版本化 ACES、角色解析、输入→工作→显示/输出、缺失配置诊断和移动端实机资格
仍须后续实现与独立验收，不由本批 Cube 对照替代。

## 显式配置目录准备

使用同一固定外部 OCIO 2.6.0 环境，选择一个明确版本，输出到尚不存在的目录：

```text
python tools/color-management/export_config.py --builtin cg-config-v5.0.0_aces-v2.1_ocio-v2.6 --output <new-directory>
```

工具另支持 ACES 1.3 的 `cg-config-v2.2.0_aces-v1.3_ocio-v2.4` 与 ACES 2.0 的
`cg-config-v4.0.0_aces-v2.0_ocio-v2.5`，没有“最新”别名或自动升级。输出
`config.ocio` 和 `manifest.json`，配置/运行时版本、编码声明、scene/display
reference、数据空间、角色及显示/视图目录都来自真实 OCIO；不猜测未声明的
编码、原色、白点或输入范围。配置正文是这些完整语义的内容依据。

导出在独立单线程进程中，临时排除 OCIO 的全局显示/视图/隐藏空间覆盖，重新
解析显式序列化正文后恢复原环境。原生库会在创建配置时缓存这些覆盖，仅在
之后删除变量不足以隔离。这个操作不能嵌入编辑器/宿主线程。显式配置上下文
变量、FileTransform（包括隐藏空间和未使用命名变换）与处理器外部资源目前
均明确拒绝；一般用户配置需要后续资源闭包导入，不会漏掉依赖后宣称可移植。

`color_config::ColorConfigManifest::from_json` 及 `VerifiedColorConfig::new`
验证严格目录结构和完整字节集合，保留不可变正文/资源。内容身份覆盖正文、
全部外部资源、固定运行时、ACES 来源、上下文和目录，不使用 OCIO cache ID
代替 SHA-256，不依赖磁盘位置。清单上限 1 MiB、配置 8 MiB、单资源 16 MiB、
资源 128 个、总内容 64 MiB；传入内容也先限额再哈希。

`ColorSelection` 必须明确配置身份、ACES 版本、输入/工作角色或名称、display/
view 和输出目标；`resolve` 不选择默认项。缺资源、字节被改动、身份/版本
不符、缺角色/视图、误将数据空间当颜色都会返回 typed Err。正常资源查询
未命中仍为 Option，不作为故障。所有目录映射拒绝重复 JSON 名称。

这层验证字节身份及目录引用，信任外部准备器输出的 OCIO 目录；它不是 OCIO
语法验证器、签名认证或像素处理器。尚未接入工程字段、宿主信道与默认渲染，
因此不能据此声称已实现完整 OCIO/ACES 工作流或关闭整个 #83。

## 明确阶段的 GPU 处理器准备

同一独立 Python 环境另需固定 `numpy==2.3.5`。先取得目录导出，再提供与其
内容身份绑定的完整 `ColorSelection` JSON；不从环境或“最新”别名选配置：

```text
python tools/color-management/export_processor.py --config-directory <bundle> --selection <selection.json> --phase working_to_display --input-alpha straight --output-alpha straight --output <new-processor.json>
```

阶段必须明确为 `input_to_working`、`working_to_display` 或 `working_to_output`。
输入/工作角色、显示/视图、输出空间及两端透明度都来自明确选择。工具验证配置
正文、运行时及策略身份；外部资源、环境上下文、3D 纹理及动态 uniforms 目前
明确拒绝。写入使用独占创建，不覆盖既有处理器；原生及 IO 失败保留 traceback。

`ColorProcessorSource::from_json(...).compile(&config, &selection)` 返回不可变
WGSL、Float32 RGBA 数值表及完整处理器身份。输入 JSON 最大 16 MiB，原始源码
256 KiB、输出源码 512 KiB；最多四张表，每张最多 262144 texel。编译必须在
非 Present 的准备操作执行，不能挪进逐帧绘制。配置/选择/版本不符及编译故障
返回 typed Err，保留 Naga/JSON 原始原因。

官方 OCIO 生成的解析计算保留，不把完整变换烘焙为夹断 HDR 的普通 LUT。独立
适配器将明确的 2D 纹理表改为数值 storage buffer，执行最近邻或双线性插值，
不要求硬件 Float32 纹理过滤。Naga 使用现有官方 30.0.1，无上游源码补丁。
通过其公开 IR 添加输入/输出范围守卫，再校验并输出 WGSL。调用方绑定：

- group 0：按 `tables()` 顺序，只读 `array<vec4<f32>>` 表，每项 16 字节。
- group 1 binding 0：只读连续 Float32 RGBA 输入；binding 1：读写输出。
- compute `main`：每个 X workgroup 一个像素，Y/Z 为 1。两端 buffer 都不得为空；
  超过较短端的 dispatch 不写入。透明度原值保留，按明确选项反关联和重新关联。

编译器信任外部 OCIO 准备器提供的变换语义，验证内容身份与固定 GPU 接口；
它不是任意着色器的语义认证器。测试夹具覆盖三个明确 ACES 版本、三个阶段和
透明度组合，并另验非中心表坐标。实际应用的纹理准入、准备信道、工程设置、
解码及 OS 显示链尚未接线，本批不能被表述为完整 #83 已实现或 HDR 显示已支持。


## 直接 Float32 图片处理

`ColorProcessorSource::compile_image(config, selection)` 使用同一原生 OCIO
程序、数值表和透明度策略，生成独立验证的 fragment/vertex WGSL。已有 compute
接口与长度守卫保留。片元按实际二维像素位置 `textureLoad`，不依赖 Float32
过滤，不为每帧建立整图中间 buffer，也不在 encode 内读回 CPU。

`motion_render::ColorProcessorOutput::new(renderer, source, processor, output)`
建立不可变的源纹理绑定、管线和预算准入后的数值表。源和输出必须为颜色、
Float32 且 alpha 与已准备程序完全一致；数据、深度、法线和覆盖率应走原有
数值路径。本接口明确拒绝其他精度，不通过隐式裁剪掩盖 half-float 溢出。
`ImageContract` 中历史 Linear/sRGB 标签不能替代完整 OCIO 空间。后续接线
使用 `ColorImageContract` 和 `ColorImageTarget`，将完整配置 hash 与明确空间/
display/view 的身份带入纹理；`VerifiedColorConfig::space_identity` 解析角色为
规范空间名，同一角色和名称得到相同身份。编译后的处理器公开两端身份，
创建绑定时检查源与输出声明，编码时再次检查目标。错误返回 typed
`ColorSpaceMismatch`，没有隐式改标签或替代变换。原始存储管线已收为私有。

`Renderer::color_image_target` 分配有明确身份的浮点纹理；
`ColorImageTarget::from_image` 是生产者显式声明边界，拒绝数据图像、整数
存储和旧 sRGB 语义，不能替代像素转换。内部 Linear 标签仅为原有 ABI 选择
不做传递函数的浮点存储。`read_color_image_target` 同时返回空间身份与原始
字节；读取仍只用于明确导出/验收。普通 scene/effect DAG 尚未使用此身份。

两阶段 CPU 参考保存在 `chain_reference.json`：三份原始配置、14 组输入→工作
空间→显示/输出，含透明度组合；每阶段按 Float32 存储量化，记录原参考文件
SHA-256。原始配置、shader 和数值表保持不变。对应 GPU 测试在同一编码器内
连续建立两次绘制，中间不读回 CPU，另验证空间/配置不匹配时不分配表、不写目标。

每帧复用该绑定，经原有 `AllocationEncoder` 调用 `encode`，目标必须等大、
与源分离且声明匹配。提交仍走 `Renderer::submit_allocated`，合并绑定与目标
的 texture ledger。没有输出编码、tone map、缩放或自动显示传递。数值表预算
在创建前准入；未提交和在途命令保留相应容量，放弃或完成后才释放。

GPU 创建失败返回 `RenderError::ColorProcessorPipeline`，原始 wgpu cause、
相关错误、poll 和可能的首要预算错误分别保留。外层文字仅包含阶段、短身份
和错误种类，不展示整段 shader。宿主命令、开始/在途状态和默认日志/错误信道
尚未接线，不能把这些库级返回值描述为应用错误提示已完成。
