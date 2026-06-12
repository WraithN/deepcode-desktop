# OpenCode SSE 按需重连设计

## 背景

`opencode-plugin` 通过 SSE 长连接监听 `opencode serve` 的事件流。当前实现中：

- `reqwest::Client` 设置了 120 秒全局 timeout
- SSE loop 在独立 tokio task 中运行，断开即 sleep 3 秒后重连
- 空闲时 SSE 连接会在 120 秒后因超时断开，打印大量 `[opencode SSE] stream error: error decoding response body` 错误日志

## 问题

1. 空闲 SSE 超时属于正常行为，不应记为 `ERROR`
2. 没有活跃会话时仍保持重连，造成无意义的连接和日志噪音
3. 用户希望 SSE 在空闲超时后静默断开，待下次有消息发送时再重建

## 目标

让 `opencode-plugin` 的 SSE 事件流：

1. 超时/连接重置类断开静默处理，不再打印 `ERROR`
2. 空闲断开后不再由后台循环自动重连
3. 下次 `send_message` / `respond` 调用时自动重建 SSE 连接

## 方案

采用**按需重建 SSE（方案 A）**：为 SSE 配置独立 client、维护连接状态、在消息发送前检查并重建。

## 详细设计

### 1. SSE 专用 HTTP Client

- 新增 `sse_client: reqwest::Client`，不设置 response timeout
- 原 `client` 继续用于普通 HTTP 请求（创建 session、发送消息），保留 120 秒 timeout

```rust
sse_client: reqwest::Client::builder()
    .timeout(None)
    .build()
    .unwrap_or_else(|_| reqwest::Client::new()),
```

### 2. SSE 状态管理

在 `OpencodeInstance` 中新增：

```rust
sse_running: Arc<std::sync::atomic::AtomicBool>,
sse_handle: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
```

- `sse_running`：标记当前 SSE loop 是否正在运行
- `sse_handle`：保存 SSE loop 的 task handle，便于 `stop()` 时取消

### 3. SSE Loop 行为变更

`sse_event_loop` 逻辑调整：

- 使用 `sse_client` 请求 `/event`
- 读取 chunk 出错时，按错误类型分类：
  - 超时、连接被重置、对端关闭连接 → `info!` / `debug!`，不记 `error`
  - 其他 decode 错误 → 记 `warn!`
- 当检测到连接已断开（正常 EOF 或静默错误）时：
  - 设置 `sse_running = false`
  - **不进入 sleep + 重连循环**，直接退出 loop

### 4. 消息发送前检查 SSE

`send_message` / `respond` 在调用 `ensure_started()` 之后、发送 HTTP 请求之前：

```rust
if !self.sse_running.load(Ordering::SeqCst) {
    self.ensure_sse_connected().await?;
}
```

`ensure_sse_connected()`：

- 如果 `sse_running` 已经为 true，直接返回
- 否则启动新的 SSE loop task，保存 handle，设置 `sse_running = true`
- 启动后等待一小段时间（如 200ms）确保连接建立，避免遗漏早期事件

### 5. 优雅关闭

`stop()` 调整：

- kill `opencode serve` 进程
- 如果 `sse_handle` 存在，abort 对应的 task
- 设置 `sse_running = false`
- 更新状态为 `Stopped`

## 数据流

```
用户发送消息
    │
    ▼
send_message / respond
    │
    ├── ensure_started()  ──► 启动 opencode serve（如未启动）
    │
    ├── ensure_sse_connected() ──► 若 sse_running=false，启动 SSE loop
    │
    └── send_message_http() ──► POST /session/{id}/message

SSE loop
    │
    ├── 正常事件 ──► 转发到 event_sink
    │
    └── 超时/断开 ──► 设置 sse_running=false，静默退出，不重连
```

## 错误处理

| 场景 | 处理 |
|------|------|
| SSE 空闲超时 | 静默，不打印 error |
| SSE 连接被重置 | 静默，设置 sse_running=false |
| SSE 收到非 JSON 数据 | 跳过该行，记 `debug!` |
| SSE 其他 decode 错误 | 记 `warn!`，退出 loop |
| 发送消息时 SSE 未连接 | 自动调用 `ensure_sse_connected()` 重建 |
| 重建 SSE 失败 | 返回 `InstanceError`，上层决定重试或报错 |

## 测试要点

1. 启动 opencode 后不发送消息，等待超过 120 秒，确认无 `ERROR` 日志
2. 发送第一条消息后，确认 SSE 连接建立并收到 `agent.token` / `agent.done` 事件
3. 等待 SSE 空闲超时后再次发送消息，确认 SSE 自动重建
4. 调用 `stop()` 后确认 SSE task 被清理，无残留

## 影响范围

- 仅修改 `crates/opencode-plugin/src/instance.rs`
- 不改动 `apps/gatewayd/src/main.rs` 中的 RTK 逻辑
- 不改变前端消息协议（event_sink 发出的事件类型不变）
