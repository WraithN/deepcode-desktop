# OpenCode Plugin SSE 按需重连设计

## 背景

`crates/opencode-plugin/src/instance.rs` 当前通过 `opencode serve` 提供 HTTP/SSE 服务。SSE 长连接使用一个带 120 秒全局 timeout 的 `reqwest::Client`。当 SSE 空闲超过 120 秒时，连接超时断开，打印大量 `[opencode SSE] stream error: error decoding response body` 错误日志，并且后台 loop 每 3 秒自动重连，造成无意义的连接和日志噪音。

## 目标

1. SSE 空闲超时后**静默处理**，不再打印 `ERROR`
2. 空闲断开后**不再由后台自动重连**
3. 下次 `send_message` 被调用时**自动重建 SSE 连接**
4. 顺手拆分 `instance.rs`，避免超过 600 行限制
5. 顺手把 `std::sync::Mutex` 替换为 `tokio::sync::Mutex`，避免在 async 上下文中阻塞线程

## 非目标

- 不清理 `parser.rs` / `mapper.rs` / `mcp_adapter.rs` 等死代码
- 不修复前后端事件名不一致
- 不重构 gatewayd/desktop 重复的 `AgentService`
- 不引入真实的 opencode PID 获取（保持 `pid: 0`）

## 架构

```
OpencodeInstance
    ├── client (HTTP, 120s timeout)
    ├── sse_client (HTTP, no timeout)
    ├── serve_process
    ├── status: tokio::sync::Mutex<InstanceStatus>
    ├── base_url: tokio::sync::Mutex<Option<String>>
    ├── sessions: tokio::sync::Mutex<HashMap<...>>
    ├── event_sender: tokio::sync::Mutex<Option<broadcast::Sender<SseEvent>>>
    ├── sse_running: AtomicBool
    └── sse_handle: tokio::sync::Mutex<Option<JoinHandle>>

send_message()
    ├── ensure_started()        → spawn opencode serve
    ├── ensure_sse_connected()  → spawn SSE listener if needed
    ├── get_or_create_session() → POST /session
    └── send_message_http()     → POST /session/{id}/message

sse_event_loop()  (in sse.rs)
    ├── GET /event (sse_client)
    ├── parse data: lines
    ├── forward to EventSink
    └── on timeout/reset → silent exit, sse_running=false
```

## 详细设计

### 1. 文件拆分

- `crates/opencode-plugin/src/instance.rs`：保留 `OpencodeInstance` 结构体、`AgentInstance` impl、`ensure_started`、`send_message`、`stop` 等主流程
- `crates/opencode-plugin/src/sse.rs`：提取 `SseEvent`、`sse_event_loop`、`forward_sse_to_event_sink`、`is_sse_disconnect_error`
- `crates/opencode-plugin/src/lib.rs`：新增 `pub mod sse;`

### 2. SSE 专用 client

新增 `sse_client: reqwest::Client`，设置一个较长的 response timeout（reqwest 0.12 不支持完全禁用 timeout）：

```rust
sse_client: reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(3600))
    .build()
    .unwrap_or_else(|_| reqwest::Client::new()),
```

原 `client` 继续用于普通 HTTP 请求（创建 session、发送消息），保留 120 秒 timeout。

### 3. Mutex 替换

把 `status`、`base_url`、`event_sender`、`sessions` 从 `std::sync::Mutex` 替换为 `tokio::sync::Mutex`。

注意：`stop()` 是 async 的，替换后代码更自然。`status()` 是 sync 方法，需要 `.blocking_lock()` 或在 async 调用方改为 await。

### 4. SSE 状态管理

新增字段：

```rust
sse_running: Arc<std::sync::atomic::AtomicBool>,
sse_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
```

- `sse_running`：标记 SSE loop 是否正在运行
- `sse_handle`：保存 SSE loop task handle，便于 `stop()` 时 abort

### 5. SSE Loop 行为

`sse_event_loop` 逻辑：

- 使用 `sse_client` 请求 `/event`
- 读取 chunk 出错时分类：
  - 超时、连接被重置、对端关闭连接 → `info!`，不记 `error`
  - 其他 decode 错误 → `warn!`
- 当检测到连接已断开时：
  - 设置 `sse_running = false`
  - **不进入 sleep + 重连循环**，直接退出 loop

### 6. 按需重建

`send_message` 流程：

```rust
self.ensure_started().await?;
self.ensure_sse_connected().await?;
let session_id = self.get_or_create_session(conversation_id).await?;
self.send_message_http(&session_id, message).await?;
```

`ensure_sse_connected()`：

- 如果 `sse_running` 已经为 true，直接返回
- 否则启动新的 SSE loop task，保存 handle，设置 `sse_running = true`
- 启动后等待 200ms，确保连接建立，避免遗漏早期事件

### 7. 停止与清理

`stop()`：

- abort SSE task handle
- `sse_running = false`
- kill `opencode serve` 进程
- status = `Stopped`
- emit status changed

## 错误处理

| 场景 | 处理 |
|------|------|
| SSE 空闲超时 | 静默，不打印 error |
| SSE 连接被重置 | 静默，标记未运行 |
| SSE 收到非 JSON 数据 | 跳过该行，debug 日志 |
| 发送消息时 SSE 未连接 | 自动调用 `ensure_sse_connected()` 重建 |
| 重建 SSE 失败 | 返回 `InstanceError`，上层决定重试或报错 |

## 测试要点

1. `cargo check -p opencode-plugin` 0 warnings
2. `cargo check --lib -p dh-desktop` 0 warnings
3. 启动后不发送消息，等待超过 120 秒，确认无 `[opencode SSE] stream error: error decoding response body`
4. 发送第一条消息，确认收到 `agent.token` / `agent.done` 事件
5. 等待 SSE 空闲超时后再次发送消息，确认 SSE 自动重建
6. 调用 `stop()` 后确认 SSE task 被清理

## 影响范围

- 修改 `crates/opencode-plugin/src/instance.rs`
- 新增 `crates/opencode-plugin/src/sse.rs`
- 修改 `crates/opencode-plugin/src/lib.rs`
- 不改变前端消息协议（EventSink 发出的事件类型不变）
