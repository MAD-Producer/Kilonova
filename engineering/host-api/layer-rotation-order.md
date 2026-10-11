# 图层组合旋转顺序

工程格式 16 增加 `Layer.rotation_order`。安卓新建视觉图层、`add_shape` 和新合成引用使用 `fixed_xyz`。旧 JSON 缺少字段时继续使用 `local_xyz`；`add` 保留传入图层的顺序，复制、预合成、保存和工程打包也保留该字段。`Layer::solid` 底层构造器保留历史默认值，调用方可显式指定新顺序。

`fixed_xyz` 按父级坐标轴先 X、再 Y、最后 Z 作用于点，矩阵为 `Rz * Ry * Rx`；无父级时使用合成坐标轴。旧 `local_xyz` 使用 `Rx * Ry * Rz`，对应向量先 Z、再 Y、最后 X。坐标仍为 X 向右、Y 向下、Z 向远处；世界坐标转换仍翻转 Y/Z。

```json
{"op":"set_rotation_order","composition":"comp-main","object":12,"order":"fixed_xyz"}
```

切换顺序会改变多轴非零的空间姿态，不改角度、关键帧或表达式，也不烘焙动画；一次撤销恢复原顺序。锁定、缺失、音频、调整层及摄影机请求返回错误。顺序不可动画；二维图层仅使用 Z 旋转，两种顺序结果一致。安卓三维图层的「图层操作」菜单提供「固定轴顺序 X→Y→Z」开关。

`capabilities.layer_3d.rotation_order` 声明 `supported`、`project_format`、`command`、`default`（新界面创建）、`missing_field`（旧数据解码）、`orders`、`changes_pose`、`animated` 和 `camera`。图层几何、锚点补偿、父子级、点击检测、预览与导出使用相同矩阵；粒子出生历史保留顺序，修改后使缓存失效。

依据：[Adobe 官方工作人员对固定轴旋转顺序的说明](https://community.adobe.com/questions-529/from-which-axes-are-the-rotation-values-in-the-timeline-panel-measured-for-a-3d-layer-60543)。这里验证的是独立数学基准、工程兼容性及两端渲染一致性；不声明未经对应 AE 版本参考画面验收的完整 AE 三维兼容。Orientation、摄影机自动朝向与摄影机生命周期需独立实现。
