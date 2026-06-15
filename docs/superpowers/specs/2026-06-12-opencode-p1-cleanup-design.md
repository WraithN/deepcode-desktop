# OpenCode Plugin P1 清理与修复设计

## 目标

完成以下 4 项 P1 级别的清理与修复：

1. 清理 `opencode-plugin` 中的死代码文件
2. 修复前后端状态事件名不一致
3. 修复 `sendMessage` 返回值格式
4. 修复 `opencode serve` 健康检查逻辑

---

## 1. 清理死代码

### 背景

`crates/opencode-plugin/src/` 下存在三个未被引用的文件：

- `parser.rs` — 解析 OpenCode JSON line 事件
- `mapper.rs` — 将 `OpencodeRawEvent` 映射为 `AgentEvent`
- `mcp_adapter.rs` — 将 MCP notification 解析为 `AgentEvent`

当前 `instance.rs` 已改用 HTTP/SSE 直接处理事件，这三个文件不再使用。

### 改动

- 删除 `crates/opencode-plugin/src/parser.rs`
- 删除 `crates/opencode-plugin/src/mapper.rs`
- 删除 `crates/opencode-plugin/src/mcp_adapter.rs`
- 修改 `crates/opencode-plugin/src/lib.rs`，移除以下模块导出：
  ```rust
  pub mod mapper;
  pub mod mcp_adapter;
  pub mod parser;
  ```
- 检查 `Cargo.toml`，移除仅被这些文件使用的依赖（如有）

---

## 2. 修复状态事件名不一致

### 背景

后端 `OpencodeInstance::emit_status` 发出的事件名为 `agent:status_changed`，而前端 `useWebSocketListeners.ts` 订阅的是 `agent.status`。其他事件名如 `agent.thinking`、`agent.token`、`agent.done` 均使用点号分隔，因此 `agent.status` 更符合现有约定。

### 改动

修改 `crates/opencode-plugin/src/instance.rs`：

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

---

## 3. 修复 sendMessage 返回值

### 背景

gatewayd 的 `send_message_handler` 当前返回：

```json
{"status": "sent"}
```

而前端 `chatStore.ts` 期望返回包含 `sessionID` 和 `parts` 的结构：

```typescript
result.sessionID
result.parts
```

### 方案

保持 `AgentInstance::send_message` trait 签名不变，仅在 gatewayd handler 中改进返回值。

修改 `apps/gatewayd/src/agents_impl.rs` 的 `send_message_handler`：

1. 在调用 `service.send_message` 之前，通过 `service.get_instance(id)` 获取实例
2. 如果实例存在，读取其 `session_id`（优先使用请求中的 `conversation_id` 作为 session 标识；如需真实 opencode session id，则需要额外获取）
3. 返回：

```json
{
  "sessionID": "<conversation_id or session_id>",
  "parts": []
}
```

本次采用最小改动：直接返回请求中的 `conversation_id` 作为 `sessionID`，`parts` 为空数组。后续如果需要真实 opencode session id，再扩展 `AgentInstance` trait。

---

## 4. 修复 opencode serve 健康检查

### 背景

`OpencodeInstance::ensure_started` 通过 `GET /health` 检查 `opencode serve` 是否就绪。但在 opencode 1.16.0 中，`/health` 返回 HTML 页面而非健康状态，虽然 HTTP 200 仍让检查通过，但语义不正确，且未来可能变化。

### 改动

将健康检查从 `GET /health` 改为 `GET /`：

```rust
let health_url = format!("{}/", base_url);
```

只要 opencode serve 返回 HTTP 200（即使是 HTML），就认为服务已就绪。

---

## 影响范围

- `crates/opencode-plugin/src/instance.rs`
- `crates/opencode-plugin/src/lib.rs`
- `crates/opencode-plugin/src/parser.rs`（删除）
- `crates/opencode-plugin/src/mapper.rs`（删除）
- `crates/opencode-plugin/src/mcp_adapter.rs`（删除）
- `crates/opencode-plugin/Cargo.toml`（可能需要）
- `apps/gatewayd/src/agents_impl.rs`

## 测试要点

1. `cargo check --workspace` 0 warnings
2. `cargo check --lib -p dh-desktop` 0 warnings
3. gatewayd 启动后 attach opencode，发送消息，前端/CLI 能收到 `agent.status` 事件
4. `POST /agents/{id}/message` 返回 `{ sessionID, parts: [] }`
5. opencode serve 能正常启动并通过健康检查
