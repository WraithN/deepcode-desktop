# opencode crash 自愈

## 现象

当 opencode serve 进程崩溃或异常退出后：
1. gatewayd 的 SSE 连接进入无限重试循环，但永远不会重新启动 opencode 进程
2. 后续发送到网关的消息全部失败（ECONNREFUSED）
3. 用户只能手动重启整个 gatewayd 才能恢复
4. 之前已建立的 session 映射全部失效，但 cached session_id 仍被使用

## 根因

`OpencodeInstance::ensure_started()` 使用 `AtomicBool` 确保启动逻辑只执行一次。一旦 opencode serve 启动成功并被标记为 `started = true`，后续所有消息都通过 HTTP 直接发送到这个已经死亡的进程。没有任何代码检测 HTTP 请求失败并尝试重启。

SSE 层虽然有自动重连（每 3 秒），但它只重连到同一个已死的 opencode 进程，永远不会触发进程级重启。

## 解决方案

### 1. 添加 reset_and_restart() 方法

在 `crates/opencode-plugin/src/instance.rs` 中添加：

```rust
async fn reset_and_restart(&self) -> Result<(), InstanceError> {
    // 1. 杀死旧 opencode 进程
    if let Some(mut child) = self.serve_process.lock().await.take() {
        let _ = child.start_kill();
    }
    // 2. 关闭旧 SSE 传输（abort 后台任务 → drop sender → 关闭 mpsc channel → 事件处理任务自然退出）
    if let Some(mut handle) = self.transport_handle.lock().await.take() {
        let _ = handle.close().await;
    }
    // 3. 清除所有 session 映射（opencode 重启后旧 session 全部失效）
    self.session_map.clear();
    // 4. 重置状态
    *self.base_url.lock().unwrap() = None;
    *self.status.lock().unwrap() = InstanceStatus::Stopped;
    self.started.store(false, Ordering::SeqCst);
    self.emit_status(InstanceStatus::Stopped);
    // 5. 重新启动 opencode serve 并连接 SSE
    self.ensure_started().await
}
```

### 2. 在 send_message/respond 中自动重试

当 HTTP 调用失败时（网络错误、连接拒绝等），自动执行 reset_and_restart 后再试：

- `send_message()`：失败 → 重置 → 重启 → 创建新 session → 重发消息
- `respond()`：失败 → 重置 → 重启 → 重发消息

### 3. 添加 clear() 到 ConversationSessionMap

在 `crates/agent-core/src/session_map.rs` 中添加 `clear()` 方法，用于清空双向映射。

### 验证

- `cargo test -p opencode-plugin -p agent-core` 全部通过（43 tests）
- 构建无 warning
