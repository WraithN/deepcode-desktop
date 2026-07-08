# CLI `dh chat` 使用旧版 agent API 导致创建失败

## 现象

运行 `dh chat opencode --interactive` 时报错：

```text
failed to create agent:
```

CLI 无法创建 agent，交互式 REPL 无法启动。

## 根因

`apps/cli/src/commands/chat.rs` 中的代码仍然调用 gatewayd 旧版 API：

```text
POST http://127.0.0.1:2346/agents
```

gatewayd 当前已经迁移到基于 session 的 AG-UI 路由，创建 agent 的正确端点是：

```text
POST /sessions/{session_id}/agents
```

因此旧版请求会 404，CLI 抛出 `failed to create agent` 错误。

此外，旧版 CLI 还使用以下已移除的接口：

- WebSocket 事件流：`/agents/events?instance_id={id}` → 已改为 `/sessions/{session_id}/events`
- 发送消息：`POST /agents/{id}/message` → 已改为通过 WebSocket 发送 AG-UI `RunAgentInput`
- 事件格式：旧版自定义事件（`event_type` + `payload`）→ 已改为 AG-UI 事件（`type` 字段为 `SCREAMING_SNAKE_CASE`）

## 解决方案

修改 `apps/cli/src/commands/chat.rs`，整体迁移到 gatewayd 的新 session-based API：

1. **创建 session**：先调用 `POST /sessions` 获取 `sessionId`。
2. **创建 agent**：再调用 `POST /sessions/{session_id}/agents` 创建 agent 实例。
3. **连接 WebSocket**：使用 `ws://127.0.0.1:{admin_port}/sessions/{session_id}/events`。
4. **发送消息**：通过 WebSocket 发送 AG-UI `RunAgentInput` JSON，而不是旧的 `POST /agents/{id}/message`。
5. **渲染事件**：更新 `print_event` 以解析 AG-UI 事件类型，如 `TEXT_MESSAGE_CONTENT`、`TOOL_CALL_START`、`CUSTOM` 等。

验证结果：

- `cargo check --bin dh` 通过，无 warning。
- `cargo build -p deepharness-cli` 成功生成 `dh` 二进制文件。
