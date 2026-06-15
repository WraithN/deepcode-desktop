# OpenCode Plugin P1 清理与修复实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成 P1 级别的 4 项清理与修复：删除死代码、统一状态事件名、修复 sendMessage 返回值、修复 opencode serve 健康检查。

**Architecture:** 小范围精确修改，不改动 `AgentInstance` trait，仅调整后端实现和 gatewayd handler。

**Tech Stack:** Rust, reqwest, serde_json

---

## 文件变更清单

- **Delete:** `crates/opencode-plugin/src/parser.rs`
- **Delete:** `crates/opencode-plugin/src/mapper.rs`
- **Delete:** `crates/opencode-plugin/src/mcp_adapter.rs`
- **Modify:** `crates/opencode-plugin/src/lib.rs`（移除死代码模块导出）
- **Modify:** `crates/opencode-plugin/src/instance.rs`（状态事件名、健康检查 URL）
- **Modify:** `crates/opencode-plugin/Cargo.toml`（如需要，移除未使用依赖）
- **Modify:** `apps/gatewayd/src/agents_impl.rs`（send_message_handler 返回值）

---

### Task 1: 删除死代码文件并更新 lib.rs

**Files:**
- Delete: `crates/opencode-plugin/src/parser.rs`
- Delete: `crates/opencode-plugin/src/mapper.rs`
- Delete: `crates/opencode-plugin/src/mcp_adapter.rs`
- Modify: `crates/opencode-plugin/src/lib.rs`

- [ ] **Step 1: 删除三个死代码文件**

```bash
rm crates/opencode-plugin/src/parser.rs
rm crates/opencode-plugin/src/mapper.rs
rm crates/opencode-plugin/src/mcp_adapter.rs
```

- [ ] **Step 2: 更新 lib.rs**

```rust
pub mod instance;
pub mod plugin;
pub mod sse;
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 2: 统一状态事件名

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:53-61`

- [ ] **Step 1: 修改 emit_status 中的事件名**

```rust
fn emit_status(&self, status: InstanceStatus) {
    self.event_sink.emit(
        "agent.status",
        json!({
            "instance_id": self.config.id,
            "status": status,
        }),
    );
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 3: 修复 opencode serve 健康检查

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs:108-109`

- [ ] **Step 1: 把健康检查 URL 从 `/health` 改为 `/`**

```rust
let health_url = format!("{}/", base_url);
```

- [ ] **Step 2: 编译检查**

Run: `cargo check -p opencode-plugin`
Expected: PASS

---

### Task 4: 修复 sendMessage 返回值

**Files:**
- Modify: `apps/gatewayd/src/agents_impl.rs:305-325`

- [ ] **Step 1: 修改 send_message_handler**

原代码：

```rust
pub async fn send_message_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(ref s) => s.clone(),
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    match service
        .send_message(&id, &req.conversation_id, &req.message)
        .await
    {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "sent"}))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
```

改为：

```rust
pub async fn send_message_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(req): Json<SendMessageRequest>,
) -> impl IntoResponse {
    let service = match state.agent_service {
        Some(ref s) => s.clone(),
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Agent runtime not available"}))).into_response()
        }
    };
    match service
        .send_message(&id, &req.conversation_id, &req.message)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "sessionID": req.conversation_id,
                "parts": [],
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e.to_string()})),
        )
            .into_response(),
    }
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: PASS

---

### Task 5: 编译检查与 warning 清零

**Files:**
- 无需修改文件（修复编译 warning）

- [ ] **Step 1: 运行 crate 检查**

Run: `cargo check -p opencode-plugin`
Expected: 0 errors, 0 warnings

- [ ] **Step 2: 运行 workspace 检查**

Run: `cargo check --workspace`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: 运行 Tauri lib 检查**

Run: `cd src-tauri && cargo check --lib -p dh-desktop`
Expected: 0 errors, 0 warnings

---

### Task 6: 端到端验证

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

- [ ] **Step 3: 检查 send_message 返回值**

```bash
curl -s -X POST http://127.0.0.1:2346/agents/<id>/message \
  -H "Content-Type: application/json" \
  -d '{"conversation_id":"test-1","message":"hello"}'
```

Expected: `{"sessionID":"test-1","parts":[]}`

- [ ] **Step 4: 检查 agent.status 事件**

从日志中确认收到 `[agent-event] agent.status: ...` 而不是 `agent:status_changed`。

- [ ] **Step 5: CLI 发消息验证**

```bash
printf "你好\n/quit\n" | ./target/release/dh chat --interactive opencode
```

Expected: 能看到 `[status]>>>> running`（因为事件名已统一为 `agent.status`）。

---

## Self-Review Checklist

- [x] **Spec coverage:** 所有 P1 项都有对应 task
- [x] **Placeholder scan:** 无 TBD / TODO / "implement later"
- [x] **Type consistency:** `sessionID` 返回字符串，`parts` 返回空数组
