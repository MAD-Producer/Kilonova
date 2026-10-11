# 扩展图层片段范围

工程格式13允许图层在合成零点之前开始，或在合成结束之后保留出点。工程总时长不变，播放和导出只采样合成范围；保存、复制和撤销不裁掉范围之外的数据。

`timeline.in_frame/out_frame` 使用有符号整数合成帧，范围为 -36000..36000，要求入点小于出点。`offset_frame` 继续表示源时间原点：局部帧 = 合成帧 - offset_frame。移动片段同步平移三个字段；裁剪只改变入/出点，关键帧保持局部时间。

```json
{"timeline":{"in_frame":-6,"out_frame":120,"offset_frame":-6}}
```

编辑请求沿用现有名称，端点允许负整数：

```json
{"op":"move_layer_clip","object":2,"in_frame":-16}
{"op":"trim_layer_clip","object":2,"in_frame":-6,"out_frame":120}
```

分割仍使用合成内的整数帧，并且必须在片段内部。小数端点本版尚不支持；调用方不能静默取整。媒体片段还需满足原有可恢复源时间范围校验，超范围时返回错误并保留工程和撤销历史。

`capabilities.layer_timeline.extended_spans` 声明 `supported`、`project_format`、`in_frame_min`、`out_frame_max`、`coordinates` 和 `outside_composition`。安卓的精确移动/裁剪、拖动和多选移动从此字段读取范围，旧宿主缺少该字段时沿用合成范围。

格式1–12的既有合法工程原样兼容；首次保存扩展片段时升级到13。旧版本应用需拒绝13，不能把负入点改为零或缩短出点。完整原始轨道保存在工程中，派生 timeline 数据不写入工程文件。

重命名、修改尺寸、保持时长或延长时长不裁掉原出点。真正缩短时长继续使用已有影响预览和 `shorten=reject/trim` 策略；`trim` 是显式删除/裁剪操作。工程格式13不代表支持全部 AE 工程能力，也没有改变小数关键帧或摄影机规则。
