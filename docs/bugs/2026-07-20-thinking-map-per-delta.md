# 2026-07-20: Thinking 重复包裹导致事件风暴及 Agent 卡住思考

## 现象

1. 会话 `e5829fe6` 中 Agent 长时间显示"思考中"不产生文本输出。
2. 同一会话（gatewayd session）被绑定了两次 opencode 实例（`ses_07fd90b0bffe...` 被回收后 `ses_07fc729efffe...` 再次启动）。
3. 前端 thinking 指示器频繁闪烁（反复打开/关闭），SSE 流中 thinking 事件过于密集。

## 根因

### 根因1：`map_thinking` 对每个 delta 包裹完整生命周期（主因）

`apps/gatewayd/src/agui/mapper.rs:93-127` 中的 `map_thinking` 函数对每个 `agent.thinking` 增量（一个 token）都输出 5 个 AG-UI 事件：

```
THINKING_START
THINKING_TEXT_MESSAGE_START
THINKING_TEXT_MESSAGE_CONTENT  ← 仅这一行是实际内容
THINKING_TEXT_MESSAGE_END
THINKING_END
```

当 LLM 产出 200 个 thinking token 时，产生 1000 个 SSE 事件，而正确行为只需约 204 个事件（Start ×1 + Content ×200 + End ×2 + 1）。事件风暴导致前端渲染压力极大，thinking 指示器在每个 token 之间快速开启/关闭，用户体验为"卡在思考中"。

### 根因2：`send_message_http` 失败导致 opencode HTTP 端点不可达（次要）

`crates/opencode-plugin/src/instance.rs:167` 中的 `send_message_http` 在发送后续消息（如工具结果回传）到 opencode 本地 HTTP 服务器（端口 3005）时报错 `error sending request`。opencode 在等待工具结果时挂起，表现为"卡住"。

## 解决方案

### 修复 `map_thinking` 状态追踪

1. 在 `AguiMapper` 中新增 `current_thinking_active: bool` 字段，跟踪 thinking 是否已激活。
2. 首个 `agent.thinking` delta 时发送 `ThinkingStart` + `ThinkingTextMessageStart` + `ThinkingTextMessageContent`（3 个事件）。
3. 后续 deltas 仅发送 `ThinkingTextMessageContent`（1 个事件）。
4. 新增 `close_thinking()` 方法，在以下时机发送 `ThinkingTextMessageEnd` + `ThinkingEnd` 并清除状态：
   - 首个 `agent.token` 到达时（从 thinking 过渡到文本输出）
   - `agent.done` 到达时
   - `agent.error` 到达时

### 验证结果

- `test_map_thinking` 测试已更新并通过（包含首次 delta → 3 事件、后续 delta → 1 事件、token 过渡关闭 thinking 的完整断言）。
- `cargo test -p dh-gatewayd agui::mapper` 全部 3 个测试通过。
- `cargo build -p dh-gatewayd` 编译通过，无新增 warning。
