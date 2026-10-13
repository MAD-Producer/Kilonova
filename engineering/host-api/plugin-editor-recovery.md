# 插件编辑事务恢复

插件编辑页仍使用 `editor_open`、`editor_message` 和 `editor_close`。正常完成使用 `editor_close(commit=true)`，取消或退出使用 `commit=false`。取消恢复打开手势前的工程，不撤销此前已提交的编辑。

## 页面存活与断开

有效 token 的成功消息、资源读取、预览请求和 `editor_heartbeat` 会续租。手势连续 120 秒无有效活动后，下次进入任何共享 host 操作时先取消遗留事务并使 token 失效。没有后台计时线程；完全无 host 活动期间不主动轮询。

```json
{"op":"editor_heartbeat","token":"editor-42"}
```

页面空闲但仍需保留未提交编辑时，前端每 30 秒发送一次 heartbeat；返回 `leaseMillis` 与 `gesture`。错误或过期 token 不续租。

运输层断开、页面销毁或导航离开时，前端发送：

```json
{"op":"editor_disconnected","token":"editor-42"}
```

该操作总是取消，不能提交。窗口 detach 也会取消并释放编辑器渲染目标。全局 `HISTORY_CANCEL` / `HISTORY_COMMIT` 会同步关闭插件编辑会话。

## 主编辑器恢复入口

`preview.pluginEditorRecovery` 提供 `open`、`gesture`、`leaseMillis`、`cancelOperation` 和 `last`。`last` 包含恢复原因、是否取消了页面手势与当前 revision。主编辑器可在遗留手势阻塞编辑时显示提示及“取消插件编辑”按钮，调用可信 host 操作：

```json
{"op":"cancel_plugin_gesture"}
```

此操作不需要页面 token；没有插件会话时无副作用，不取消其他 host 手势。它仅用于主编辑器的可信操作通道，不能作为插件页面绕过 token 的入口。

插件请求中发生可展开的 Rust panic 时，核心恢复未提交编辑，清除颜色和参数子编辑状态并返回错误；host 关闭页面并使 token 失效。内存耗尽或进程 abort 不属于可恢复 panic。

实际提示按钮、页面 heartbeat 和断开生命周期接线由前端实现，本次只提供共享后端能力。已有 batch prepare panic 回滚保持不变。
