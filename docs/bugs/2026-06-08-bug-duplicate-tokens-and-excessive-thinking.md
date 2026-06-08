# Bug: 流式 token 重复与过多思考步骤

## 现象

1. **Token 重复**：前端显示重复的 token，如 "CreatedCreated ``quickquick_sort_sort.py.py``"
2. **过多思考步骤**：每个 assistant 消息出现多个 "思考中" 步骤

## 根因

### 1. Token 重复
`session_manager.register` 未做去重。当同一 WebSocket 连接发送多个含 `conversationId` 的请求时（如 `session.logLoad` 和 `agent.sendMessage`），同一 sender 被注册多次。`send_to_session` 遍历所有 handles 发送消息，导致前端收到重复的 `agent.token` 事件。

### 2. 过多思考步骤
- `stream_opencode_output` 开始时会发送一个 `agent.thinking` 事件（content="AI 正在思考..."）
- 随后 `message.part.updated` 的 `step-start` 也会发送 `agent.thinking` 事件（content=""）
- 两次事件的 content 不同，导致创建两个 thinking 步骤

## 解决方案

### 1. Token 重复
在 `session_manager.register` 中添加去重逻辑：注册新 handle 前先移除相同 `conn_id` 的旧 handle。

```rust
// src-tauri/src/gateway/session_manager.rs
pub async fn register(&self, conversation_id: String, handle: ConnectionHandle) {
    let mut conns = self.connections.write().await;
    let handles = conns.entry(conversation_id).or_insert_with(Vec::new);
    handles.retain(|h| h.id != handle.id);
    handles.push(handle);
}
```

### 2. 过多思考步骤
- 移除 `stream_opencode_output` 开始时的初始 `agent.thinking` 事件，让 `step-start` 事件单独负责
- 在 `useWebSocketListeners.ts` 中使用 `partId` 进行更精确的去重
- 在 `MessageStep` 类型中添加 `partId` 字段

## 验证

- Rust 编译：`cargo check --bin dh` ✅
- TypeScript 编译：`npx tsc --noEmit -p tsconfig.check.json` ✅
