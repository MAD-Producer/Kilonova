# Cube LUT 数值参考

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
