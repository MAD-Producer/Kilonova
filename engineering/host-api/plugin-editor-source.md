# 插件作者工程：第一阶段数据契约

本文件记录 `feat/plugin-editor` 的 M1 实现。可保存源工程并校验节点图或 WGSL；
原生面板渲染、拖拽设计器、软件内/独立编辑窗口和布局打包尚待 M2–M5。
无窗口校验命令不表示独立编辑器窗口已经实现。

## 工程与权威来源

`.msplugin` 为 JSON 作者工程，保留且只接受当前 `format_version: 1`。
根字段 `package`、`logic`、`interface` 均必填，未知字段拒绝；没有格式迁移。
`package` 包含必填的 `id`、`version`、`author`、`license`，作者允许为空。

- `logic.kind: graph` 的 `document` 是现有 `GraphDocument`，资源 PNG 随图保存。
  公共参数由图自身的 `public_parameters` 转换，复用现有包编译器的转换函数。
- `logic.kind: wgsl` 的 `document` 包含源码、入口、效果身份、公共参数、工作空间和 alpha 模式。
  只复用现有 SDK 6 图像效果与便携着色器校验，不开放原生代码、自定义宿主绑定或任意计算入口。
- `interface` 为结构化原生布局，包含自己的 `format_version: 1` 与根节点。
  同一效果只保留一种逻辑源，不进行 WGSL 到节点的反向转换。

已有 `.msgraph`、效果包协议、渲染语义和锁定基线不随新作者格式改动。
`compile_logic()` 只编译逻辑。其 manifest 尚未携带新布局，不能作为布局发布完成的证据。

## 草稿保存与预览检查

`validate_draft()` / `to_json()` / `save()` 接受未连完的图、未写完的 WGSL，
以及指向已删除公共参数的布局绑定。它们仍检查工程结构、图片身份、版本、
有限数值与资源预算；不把无效引用替换为默认参数。

`validate_bindings()` 在草稿检查后解析绑定、检查控件类型与参数可用性。
`compile_logic()` 再运行实际图编译器或 WGSL 解析、验证与便携后端。
结构有效的语法错误草稿可以保存，错误会在编译阶段返回。

保存先验证并生成完整字节，再复用现有图工程的同目录暂存/同步/rename。
验证或发布失败返回原始错误，清理暂存文件，不覆盖已有有效文件；不会重试或改写目标目录。

## 参数与绑定

布局使用公共参数的稳定 `id`，不使用数组索引。多个控件可绑定同一个参数，
但布局不保存参数值、动画或第二份参数表。移动控件和改变尺寸不会改变逻辑输出。

源码通过 `param_<id>()` 读取公共参数。字母数字保持原样，其他 UTF-8 字节写成
`_<两位十六进制>_`，包含下划线本身，避免规范化冲突。例如：

| 参数 ID | 生成函数 |
| --- | --- |
| `amount` | `param_amount()` |
| `amount-offset` | `param_amount_2d_offset()` |
| `amount_offset` | `param_amount_5f_offset()` |

`parameter_function()` 在源码未编译时也可查询函数名。参数重排时函数名不变，
编译器更新其底层槽位；重命名参数不会自动重写作者源码或布局。

当前控件：Float→Number，Vec2/Vec3→Vector，Color→Color，Bool→Checkbox，Enum→Dropdown。
`Auto` 按真实类型选择。Curve 对象控件尚未提供，绑定它会明确拒绝。
这份契约不承诺当前图模型尚未具有的参数类型。

## 布局与容量

布局节点支持 Row、Column、Group、Tabs、Split、Parameter、Preview、Timeline、Note。
每个节点及标签页具有独立且全局唯一的稳定 ID。空容器允许作为草稿。
Preview 与 Timeline 各最多一个，最终由宿主适配器提供内容。

宽高用 Auto、Points、Weight 表达；有限正值上限 8192，分栏比例为 0.05–0.95。
间距为 None/Compact/Normal/Relaxed，对齐为 Start/Center/End/Stretch，待运行时从
现有主题取值。布局不携带颜色、可执行命令或原生代码，不建立第二套主题。

| 边界 | 当前限制 |
| --- | --- |
| 作者工程输入 | 25 MiB，读取前及读取后均检查 |
| WGSL 原文 | 240 KiB；附加参数函数后总源码最多 256 KiB |
| WGSL 公共参数 | 32 项；文本元数据合计 64 KiB，每项枚举选项最多 256 个 |
| 布局组件 | 256 个，含标签页；根深度为 1，最多 16 层 |
| 单个 Tabs | 32 页 |
| 分组/标签标题、说明文字 | 分别最多 256 / 2048 UTF-8 字节 |
| 图像 | 沿用现有图工程的内容哈希、图片数量、压缩及解码预算 |

## 无窗口校验

```powershell
motion-studio.exe --plugin-check crates/motion-graph/examples/parameter-tint.msplugin
```

示例由本项目独立编写，使用现有源码依赖，不增加外部素材或依赖。
成功时 stdout 仅有 JSON 报告：逻辑类型、参数数、布局组件数与编译字节数。
失败以非零状态退出；不会打开合成、创建用户工程、初始化 GPU 或修改源文件。
该模式与其他命令/MCP 模式互斥。

读取和编译均在进入处理前发布 in-flight，写入并刷新 Info start，携带请求号、源路径和阶段。
完成耗时为 Debug；失败为 Error，进入默认文件日志和 stderr。异常退出通过现有 panic hook
记录原始 panic/强制回溯，操作作用域清除 in-flight，不捕获为成功或重启。
显示与 Debug 终止摘要有长度/控制字符限制，不输出 WGSL 整段诊断片段；
`Error::source()` 保留可按原始类型读取的本机原因，供后续诊断界面使用。

验证证据及边界见 [M1 验收记录](../validation/plugin-editor-m1.md)。
