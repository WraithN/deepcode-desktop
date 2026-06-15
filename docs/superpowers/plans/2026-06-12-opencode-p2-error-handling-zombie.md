# OpenCode Plugin P2 错误处理与子进程回收实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 gatewayd 中 send_message 错误被静默丢弃的问题，以及 opencode serve 子进程未被回收产生僵尸进程的问题。

**Architecture:** 小范围精确修改：send_message 返回 JoinHandle 并通过 EventSink 广播错误；stop/启动失败路径中 kill 后调用 wait。

**Tech Stack:** Rust, tokio

---

## 文件变更清单

- **Modify:** `apps/gatewayd/src/agents_impl.rs`
  - `AgentService::send_message` 返回 `JoinHandle`，错误时 emit `agent.error`
  - `send_message_handler` 改为 202 Accepted 响应
- **Modify:** `crates/opencode-plugin/src/instance.rs`
  - `stop()` 中 kill 后调用 `wait()`
  - `ensure_started()` 健康检查失败路径中也调用 `wait()`

---

### Task 1: 修复 opencode serve 子进程回收

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:118-123`
- Modify: `crates/opencode-plugin/src/instance.rs:354-370`

- [ ] **Step 1: 在 `ensure_started()` 健康检查失败路径中回收子进程**

原代码：

```rust
if !ready {
    let _ = child.start_kill();
    self.started.store(false, std::sync::atomic::Ordering::SeqCst);
    return Err(InstanceError::ProcessError(
        format!("opencode serve did not become ready on port {}", port),
    ));
}
```

改为：

```rust
if !ready {
    let _ = child.start_kill();
    let _ = child.wait().await;
    self.started.store(false, std::sync::atomic::Ordering::SeqCst);
    return Err(InstanceError::ProcessError(
        format!("opencode serve did not become ready on port {}", port),
    ));
}
```

- [ ] **Step 2: 在 `stop()` 中回收子进程**

原代码：

```rust
if let Some(mut child) = self.serve_process.lock().await.take() {
    let _ = child.start_kill();
}
```

改为：

```rust
if let Some(mut child) = self.serve_process.lock().await.take() {
    let _ = child.start_kill();
    let _ = child.wait().await;
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 2: 修复 gatewayd send_message 错误处理

**Files:**
- Modify: `apps/gatewayd/src/agents_impl.rs:163-181`
- Modify: `apps/gatewayd/src/agents_impl.rs:305-325`

- [ ] **Step 1: 修改 `AgentService::send_message` 签名和实现**

原代码：

```rust
pub async fn send_message(
    &self,
    instance_id: &str,
    conversation_id: &str,
    message: &str,
) -> Result<(), InstanceError> {
    let instance = self
        .instances
        .lock()
        .await
        .get(instance_id)
        .ok_or(InstanceError::NotFound(instance_id.to_string()))?;
    let message = message.to_string();
    let conversation_id = conversation_id.to_string();
    tokio::spawn(async move {
        let _ = instance.send_message(&conversation_id, &message).await;
    });
    Ok(())
}
```

改为：

```rust
pub fn send_message(
    &self,
    instance_id: &str,
    conversation_id: &str,
    message: &str,
) -> Result<tokio::task::JoinHandle<()>, InstanceError> {
    let instance = self
        .instances
        .blocking_lock()
        .get(instance_id)
        .cloned()
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

注意：因为函数不再 async，锁需要改为 `.blocking_lock()`。

- [ ] **Step 2: 修改 `send_message_handler` 响应**

原代码：

```rust
match service
    .send_message(&id, &req.conversation_id, &req.message)
    .await
{
    Ok(()) => (StatusCode::OK, Json(serde_json::json!({"sessionID": req.conversation_id, "parts": []}))).into_response(),
    Err(e) => (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": e.to_string()})),
    )
        .into_response(),
}
```

改为：

```rust
match service.send_message(&id, &req.conversation_id, &req.message).await {
    Ok(_) => (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"sessionID": req.conversation_id, "parts": []})),
    )
        .into_response(),
    Err(e) => (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": e.to_string()})),
    )
        .into_response(),
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: PASS

---

### Task 3: 编译检查与 warning 清零

**Files:**
- 无需修改文件

- [ ] **Step 1: 运行 workspace 检查**

Run: `cargo check --workspace`
Expected: 0 errors, 0 warnings

- [ ] **Step 2: 运行 Tauri lib 检查**

Run: `cd src-tauri && cargo check --lib -p dh-desktop`
Expected: 0 errors, 0 warnings

---

### Task 4: 端到端验证

**Files:**
- 无需修改文件

- [ ] **Step 1: 重新构建 gatewayd 和 cli**

```bash
cargo build -p dh-gatewayd -p deepharness-cli --release
```

- [ ] **Step 2: 启动 gatewayd**

```bash
RUST_LOG=info ./target/release/dh-gatewayd --attach opencode
```

- [ ] **Step 3: 测试正常发消息**

```bash
curl -s -X POST http://127.0.0.1:2346/agents/<id>/message \
  -H "Content-Type: application/json" \
  -d '{"conversation_id":"p2-test","message":"hello"}'
```

Expected: HTTP 202, body `{"sessionID":"p2-test","parts":[]}`

- [ ] **Step 4: 测试 stop 实例后无僵尸进程**

```bash
curl -s -X POST http://127.0.0.1:2346/agents/<id>/stop
pgrep -a "opencode serve" || echo "no opencode serve"
ps aux | grep defunct | grep opencode || echo "no zombie opencode"
```

Expected: 无 opencode serve 进程，无 opencode 僵尸进程。

- [ ] **Step 5: CLI 验证**

```bash
printf "你好\n/quit\n" | ./target/release/dh chat --interactive opencode
```

Expected: 能正常收发消息。

---

## Self-Review Checklist

- [x] **Spec coverage:** 两项 P2 要求都有对应 task
- [x] **Placeholder scan:** 无 TBD / TODO / "implement later"
- [x] **Type consistency:** `send_message` 返回 `Result<JoinHandle<()>, InstanceError>`，handler 中不再 `.await`
