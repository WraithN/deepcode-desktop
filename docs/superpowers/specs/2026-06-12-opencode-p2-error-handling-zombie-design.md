# OpenCode Plugin P2 错误处理与子进程回收设计

## 目标

修复以下两项 P2 架构债务：

1. gatewayd 中 `send_message` 的错误被静默吞掉
2. `opencode serve` 子进程停止后未被回收，产生僵尸进程

---

## 1. gatewayd send_message 错误处理

### 背景

当前 `apps/gatewayd/src/agents_impl.rs` 中的 `AgentService::send_message`：

```rust
pub async fn send_message(&self, ...) -> Result<(), InstanceError> {
    let instance = self.instances.lock().await.get(instance_id).cloned()
        .ok_or(InstanceError::NotFound(...))?;
    let message = message.to_string();
    let conversation_id = conversation_id.to_string();
    tokio::spawn(async move {
        let _ = instance.send_message(&conversation_id, &message).await;
    });
    Ok(())
}
```

`tokio::spawn` 中的错误被 `let _ = ...` 丢弃，HTTP handler 立即返回 `{"status":"sent"}`，调用方无法知道消息是否真正发出。

### 方案

保持 HTTP handler 不长时间阻塞（因为 AI 响应可能耗时数十秒），但把异步任务中的错误通过 `EventSink` 广播给所有订阅者。

修改 `AgentService::send_message` 返回 `tokio::task::JoinHandle<()>`：

```rust
pub async fn send_message(
    &self,
    instance_id: &str,
    conversation_id: &str,
    message: &str,
) -> Result<tokio::task::JoinHandle<()>, InstanceError> {
    let instance = self.instances.lock().await.get(instance_id).clone()
        .ok_or(InstanceError::NotFound(instance_id.to_string()))?;
    let message = message.to_string();
    let conversation_id = conversation_id.to_string();
    let event_sink = self.event_sink.clone();
    let instance_id = instance_id.to_string();

    Ok(tokio::spawn(async move {
        if let Err(e) = instance.send_message(&conversation_id, &message).await {
            event_sink.emit(
                "agent.error",
                serde_json::json!({
                    "instance_id": instance_id,
                    "conversation_id": conversation_id,
                    "message": e.to_string(),
                }),
            );
        }
    }))
}
```

HTTP handler 中保留立即返回 202 的语义。`send_message` 本身是 async，但内部只执行获取实例和 spawn，不会长时间阻塞：

```rust
match service.send_message(&id, &req.conversation_id, &req.message).await {
    Ok(_) => (StatusCode::ACCEPTED, Json(json!({"sessionID": req.conversation_id, "parts": []}))).into_response(),
    Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": e.to_string()}))).into_response(),
}
```

状态码从 `200 OK` 改为 `202 Accepted`，更准确地表达"已接受，异步处理中"。

---

## 2. opencode serve 子进程回收

### 背景

`crates/opencode-plugin/src/instance.rs` 的 `stop()`：

```rust
if let Some(mut child) = self.serve_process.lock().await.take() {
    let _ = child.start_kill();
}
```

`start_kill()` 发送 SIGKILL，但没有调用 `wait()` 回收子进程，导致进程表中出现 `<defunct>` 僵尸进程。

### 方案

kill 后调用 `child.wait().await`：

```rust
if let Some(mut child) = self.serve_process.lock().await.take() {
    let _ = child.start_kill();
    let _ = child.wait().await;
}
```

在 `ensure_started()` 中，如果健康检查失败也需要回收：

```rust
if !ready {
    let _ = child.start_kill();
    let _ = child.wait().await;
    self.started.store(false, ...);
    return Err(InstanceError::ProcessError(...));
}
```

---

## 影响范围

- `apps/gatewayd/src/agents_impl.rs`
- `crates/opencode-plugin/src/instance.rs`

---

## 测试要点

1. `cargo check --workspace` 0 warnings
2. `cargo check --lib -p dh-desktop` 0 warnings
3. gatewayd 发送消息后，如果 opencode serve 启动失败，能看到 `agent.error` 事件
4. 反复 stop/start 实例，确认无 `<defunct>` 僵尸进程
