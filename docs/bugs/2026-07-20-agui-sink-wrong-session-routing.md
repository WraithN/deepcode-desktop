# 2026-07-20 — AguiEventSink 将 SSE 事件路由到错误 Session

## 现象
通过 gatewayd 发送 chat 请求后，SSE 客户端仅收到 `RUN_STARTED` 和 `STATE_SNAPSHOT`，后续所有 Agent 事件（TEXT_MESSAGE_START、TEXT_MESSAGE_CONTENT、TEXT_MESSAGE_END 等）均未到达。

## 根因 1（已修复）：session_for_instance 错误路由
`AguiEventSink::emit` 通过 `session_for_instance(&instance_id)` 查找目标 session，但该方法线性扫描所有 session 并返回**第一个匹配**的结果。

当多个 session 共享同一个 Agent 实例（例如 `opencode`）时，`session_for_instance("opencode")` 始终返回最早创建的 session，而非当前实际接收消息的 session，导致所有 session 的 Agent 事件都被广播到错误的会话。

Payload 中已有正确的 `conversation_id`（即 gatewayd session_id），但 `AguiEventSink::emit` 未使用该字段进行路由。

## 根因 2（补充发现）：opencode 冷启动延迟
opencode 实例未被预启动，首次 `send_message` 调用时才触发 `ensure_started()`，opencode 自动 bootstrap 需约 2 分钟。在此期间所有 Agent 事件都无法产生，SSE 流在 `STATE_SNAPSHOT` 之后出现长时间无数据，加重了"事件丢失"的观感。冷启动完成后，事件通过 relay loop → agui_sink → broadcast channel 正常送达 SSE 客户端。

## 解决方案
修改 `apps/gatewayd/src/agui_sink.rs` 中的 `emit` 方法：
- **优先使用** payload 中的 `conversation_id` 作为 `session_id` 进行路由
- **回退**：当 `conversation_id` 为空时，仍使用 `session_for_instance` 作为兜底

### 修改文件
- `apps/gatewayd/src/agui_sink.rs:42-74`

### 验证
1. `cargo build -p dh-gatewayd` 编译通过，无新增 lint warnings。
2. Chat SSE 请求测试：向已有 warm 实例的 session 发送消息，curl 成功收到完整事件流（RUN_STARTED → STATE_SNAPSHOT → TEXT_MESSAGE_START → TEXT_MESSAGE_CONTENT ×N → TEXT_MESSAGE_END），验证 event routing 正确。
