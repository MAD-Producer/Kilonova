# 节点学习库 · Learning Library

建议从基础形状开始，再进入三种视觉方向。每个 `.msgraph` 都能独立读取、修改公开参数并导出 `.msfx`；修改内置示例的副本不会覆盖模板。

## 基础练习 · Fundamentals

| ID | 文件 | 重点 |
| --- | --- | --- |
| `drifting_circle` | [漂移圆形](drifting-circle.msgraph) | 时间、参数、抗锯齿距离场 |
| `uv_gradient` | [UV 渐变](uv-gradient.msgraph) | 归一化坐标与颜色混合 |
| `animated_stripes` | [动态条纹](animated-stripes.msgraph) | 重复相位与时间 |
| `sdf_ring` | [圆环描边](sdf-ring.msgraph) | 距离与描边覆盖率 |
| `rounded_button` | [圆角按钮](rounded-button.msgraph) | 圆角矩形、填充与描边 |
| `smooth_union` | [平滑并集](smooth-union.msgraph) | 两路距离场的平滑合并 |
| `image_invert` | [图像反相](image-invert.msgraph) | 图像输入、采样与颜色处理 |
| `parameter_tint` | [参数染色](parameter-tint.msgraph) | 公开参数与图像混合 |

## 柔光空间 · Soft Space

| ID / 文件 | 节点 / 参数 | 学习路线 |
| --- | --- | --- |
| `soft_sphere` · [柔光球体](soft-sphere.msgraph) | 54 / 5 | 像素位置 → 球面法线 → 漫反射近似 → 圆形填充；独立椭圆渐变模拟接触光影 |
| `soft_sphere_motion` · [柔光球体 · 伪3D运动](soft-sphere-motion.msgraph) | 121 / 10 | 周期绕行与透视缩放 → 球面法线与细环旋转 → 漫反射近似；落点、高度和尺度驱动椭圆光影 |
| `translucent_card` · [半透卡片](translucent-card.msgraph) | 31 / 5 | 圆角距离场 → 填充与描边 → 内置渐变背景混合 → 慢速漂浮 |

调整球体光照方向与灰色调，观察相同轮廓如何产生不同的体积感；再改卡片的半尺寸、圆角与覆盖强度。

原柔光球体示例保持不变，运动版作为独立示例提供。运动版公开“绕行幅度”“伪3D纵深”“抬起高度”和“球体旋转速度”。横移、纵深和抬落共享运动相位，透视尺度同时作用于轮廓、解析法线与阴影；抬起时阴影变宽、变淡，落下时收拢。斜轴细环在球面上旋转，使纯色球体的自转可见。漂浮速度和旋转速度可分别设为 0；细环强度设为 0 恢复纯色材质。速度单位为 rad/s，参考画布保持 64×64，大半径或大绕行幅度可能超出画布。

The original sphere is preserved; the motion version is a separate example. It exposes orbit width, pseudo-3D depth, lift height and surface spin speed. Perspective scale applies consistently to the silhouette, analytic normals and shadow. Lift widens and fades the shadow; contact tightens it. A subtle tilted surface band makes spin visible. Orbit and spin can be paused independently with zero speed, and zero band strength restores the plain material.

运动版按画布短边等比缩放，并在任意长宽比画布中居中，保持球体圆形。参数中的 px 使用 64×64 教学坐标。可复现地导出运动版并生成 290 帧、384×384 的 GPU 渲染序列（30 fps）：

```sh
cargo run -p motion-graph --example render_learning_examples -- target/sphere-motion soft_sphere_motion 290 384
```

球体没有场景深度或真实投射阴影。卡片只混合本图生成的背景，不折射、不模糊工程中的其他图层。两者都是二维图像效果。

## 弹性几何 · Graphic Motion

| ID / 文件 | 节点 / 参数 | 学习路线 |
| --- | --- | --- |
| `elastic_capsule` · [阻尼回弹](elastic-capsule.msgraph) | 33 / 3 | 周期相位 → 正弦振荡 × 衰减包络 → 位移、宽高反向联动 |
| `graphic_cutout` · [布尔裁切](graphic-cutout.msgraph) | 22 / 3 | 矩形与移动圆形 → 距离场差集 → 填充 → 撞色 |

尝试改变回弹周期与幅度，观察位移和挤压拉伸的关系；再用裁切半径和速度组织负空间。

回弹是 shader 的解析动画，不是物理模拟或时间轴缓动。宽高反向联动只近似保持形状面积，圆角变化会影响实际覆盖面积。差集仅生成自己的图形，不裁切其他工程图层。

## 发光氛围 · Glow Atmosphere

| ID / 文件 | 节点 / 参数 | 学习路线 |
| --- | --- | --- |
| `pulse_halo` · [脉动光环](pulse-halo.msgraph) | 36 / 4 | UV 径向距离 → 环带衰减 → SDF 亮核 → 慢速脉动 |
| `floating_lights` · [漂浮光斑](floating-lights.msgraph) | 71 / 3 | 三路动态中心 → 距离与柔和覆盖 → 分层混合 |

先调光晕宽度，区分亮核与宽光晕；再改变光斑速度和强度，观察相位差如何避免同步运动。

这里的发光是解析颜色渐变，不是对输入图像执行 Bloom 卷积。光斑没有真实景深、体积雾、粒子模拟或次表面散射。

## 接下来扩展什么

第一批聚焦已有节点能够完整运行的效果。后续真实能力分别依赖统一深度输入、深度辅助阴影、多 pass Bloom 与模糊，以及支持预算和降级的调度器。这些能力完成后可以继续增加独立示例，而不是修改已有图的含义。

相关后端规划：[专业合成路线](https://github.com/Yukk1o/motion-studio/issues/77)、[深度输入与推理](https://github.com/Yukk1o/motion-studio/issues/74)、[执行 DAG](https://github.com/Yukk1o/motion-studio/issues/78)。

All examples are original, editable teaching graphs. The reference canvas is 64×64. Soft shadows, transparency and halos in this first collection are explicit 2D approximations, not physical materials, depth rendering or volumetric light. The bilingual runtime catalogue contains a lesson and limitations for every example.
