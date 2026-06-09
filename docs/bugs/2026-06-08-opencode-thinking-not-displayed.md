# Bug: opencode 思考信息流前端不展示

## 现象

使用 opencode 智能体时，前端不展示思考信息流（thinking steps）。ChatPanel 中只显示最终文本输出，看不到"思考中"等步骤状态。

## 根因分析

opencode serve 的 SSE `/event` 端点不输出独立的 `"thinking"` 类型事件，而是通过 `message.part.updated` 中的 `step-start` part 来传递 thinking 信息。

问题出在三个层面：

1. **Rust 后端 `streaming.rs` — `message.part.updated` 的 `step-start` 处理**：
   直接发送 `part.clone()` 给前端，但 part 的结构是 `{"id":"...","type":"step-start","tool":"thinking","text":"..."}`，**没有 `content` 字段**。

2. **Rust 后端 `streaming.rs` — `"thinking"` 事件处理**：
   只读取 `event.payload.get("content")`，如果 opencode 使用 `"text"` 字段则获取不到内容。

3. **前端 `useWebSocketListeners.ts`**：
   只读取 `params.content`，不兼容 `params.text` 字段。

## 解决方案

### Rust 后端 `streaming.rs`

- `message.part.updated` 的 `step-start` 处理：从 part 中提取 `"text"` 和 `"id"`，构建包含 `"content"` 的标准 payload
- `"thinking"` 事件处理：同时支持 `"content"` 和 `"text"` 字段
- `stream_fallback` 的 `step-start` 处理：同样构建标准 payload

### 前端 `useWebSocketListeners.ts`

- `agent.thinking` 通知处理：同时读取 `content` 和 `text` 字段
  ```typescript
  const content = (p.content as string) || (p.text as string) || '';
  ```

## 验证结果

- `cargo check --bin dh` ✅
- `npx tsc --noEmit -p tsconfig.check.json` ✅
- 核心测试全部通过 ✅
