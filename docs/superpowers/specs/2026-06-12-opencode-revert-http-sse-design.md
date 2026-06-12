# OpenCode Plugin 回退到 HTTP/SSE + 按需重连设计

## 背景

当前 `src-tauri/crates/opencode-plugin` 通过 MCP 调用 `opencode mcp-server` 来驱动智能体。用户要求改回原来的 HTTP/SSE 方式（直接启动 `opencode serve`），并修复 SSE 空闲超时导致的错误日志问题。

同时，`crates/opencode-plugin/` 是未跟踪的旧实现，需要在回退后由用户自行清理或作为参考，但本变更只修改 `src-tauri/crates/opencode-plugin`。

## 目标

1. 删除 `src-tauri/crates/opencode-plugin` 中的 MCP 代码
2. 恢复 HTTP/SSE 架构：启动 `opencode serve`、创建 session、发送消息、监听 `/event`
3. SSE 事件通过 Tauri `AppHandle::emit("agent:event", AgentEvent)` 转发给前端
4. SSE 空闲超时后静默退出，不再自动重连
5. 下次 `send_message` 被调用时自动重建 SSE 连接

## 架构

```
OpencodePlugin::create_instance(config, app_handle, logger)
    │
    ▼
OpencodeInstance
    ├── client (HTTP, 120s timeout)     → create_session / send_message
    ├── sse_client (HTTP, no timeout)   → SSE long poll
    ├── serve_process (opencode serve)
    ├── sse_running (AtomicBool)
    ├── sse_handle (JoinHandle)
    └── sessions (conversation_id → opencode_session_id)

send_message()
    ├── ensure_started()      → spawn opencode serve if needed
    ├── ensure_sse_connected()→ spawn SSE listener if needed
    ├── create/get session
    └── send_message_http()

sse_event_loop()
    ├── GET /event
    ├── parse `data:` lines
    ├── forward to AgentEvent
    └── on timeout/reset → silent exit, sse_running=false
```

## 详细设计

### 1. 文件调整

- **重写：** `src-tauri/crates/opencode-plugin/src/instance.rs`
- **删除：** `src-tauri/crates/opencode-plugin/src/mcp_adapter.rs`
- **修改：** `src-tauri/crates/opencode-plugin/src/lib.rs`（移除 `mcp_adapter` 模块）
- **修改：** `src-tauri/crates/opencode-plugin/Cargo.toml`（添加 reqwest/futures-util/log，移除 agent-runtime 如不需要）
- **可选保留：** `src-tauri/crates/opencode-plugin/src/parser.rs`、`mapper.rs`（本次 HTTP/SSE 使用新的 SSE 映射，旧 parser/mapper 暂时保留但不引用，后续可清理）

### 2. `OpencodeInstance` 结构

```rust
pub struct OpencodeInstance {
    config: InstanceConfig,
    status: Arc<Mutex<InstanceStatus>>,
    app_handle: AppHandle,
    logger: Arc<SessionLogger>,
    client: reqwest::Client,
    sse_client: reqwest::Client,
    base_url: Mutex<Option<String>>,
    serve_process: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    sessions: Arc<Mutex<HashMap<String, String>>>,
    started: Arc<std::sync::atomic::AtomicBool>,
    sse_running: Arc<std::sync::atomic::AtomicBool>,
    sse_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}
```

### 3. SSE 事件到 `AgentEvent` 的映射

SSE `data:` 行的 JSON 格式示例：

```json
{"type": "message.part.delta", "properties": {"delta": "hello"}}
{"type": "thinking", "content": "..."}
{"type": "message.part.updated", "properties": {"part": {"type": "step-start", "text": "..."}}}
{"type": "session.idle"}
{"type": "session.error"}
```

映射规则：

| SSE type | AgentEvent |
|----------|------------|
| `message.part.delta` | `TextDelta { content: delta }` |
| `thinking` | `Thinking { content }` |
| `message.part.updated` (step-start) | `Thinking { content: step_text }` |
| `session.idle` | `Done` |
| `session.error` | `Error { message }` |

交互事件（question/permission）在本次设计中先通过 `Error` 或 `TextDelta` 简单透出，保持最小改动；后续如果需要完整交互支持再扩展。

### 4. SSE Loop 行为

- 使用 `sse_client`（无 timeout）连接 `GET /event`
- 解析 `data:` 行，映射并 emit 事件
- 当连接因超时、连接重置、对端关闭而断开时：
  - 不打印 `ERROR`
  - 可打印 `info!` 级别日志
  - 设置 `sse_running = false`
  - **不重连，直接退出 task**

### 5. 按需重建

`send_message` 流程：

```rust
self.ensure_started().await?;
self.ensure_sse_connected().await?;
let session_id = self.get_or_create_session(conversation_id).await?;
self.send_message_http(&session_id, &message).await?;
```

`ensure_sse_connected` 检查 `sse_running`，为 false 时启动新的 SSE loop task。

### 6. 停止与清理

`stop()`：

- abort SSE task handle
- `sse_running = false`
- kill `opencode serve` 进程
- status = `Stopped`
- emit status changed

## 错误处理

| 场景 | 处理 |
|------|------|
| SSE 空闲超时 | 静默，不报错 |
| SSE 连接重置 | 静默，标记未运行 |
| SSE 收到非 JSON 数据 | 跳过，debug 日志 |
| send_message 时 SSE 未连接 | 自动重建 |
| opencode serve 启动失败 | `ProcessError` |
| 创建 session / 发送消息失败 | `SendFailed` |

## 测试要点

1. `cargo check --manifest-path src-tauri/Cargo.toml -p opencode-plugin` 0 warnings
2. `cargo check --bin ai-coding-desktop` 0 warnings
3. 启动后不发送消息，等待 3 分钟，确认无 `[opencode SSE] stream error: error decoding response body`
4. 发送第一条消息，确认收到 `agent:event` 事件流
5. 等待 SSE 超时后再次发送消息，确认 SSE 重建并正常回复

## 影响范围

- 只影响 `src-tauri/crates/opencode-plugin`
- 事件协议保持 `agent:event` + `AgentEvent`，前端无需改动
- `AgentInstance` trait 当前只有 `send_message`（无 `respond`），因此 HTTP `respond` 接口本次不实现
