# gatewayd AG-UI 协议改造设计

## 目标

将 `dh-gatewayd` 的 Agent 对外协议统一为 [AG-UI](https://docs.ag-ui.com/introduction)（Agent-User Interaction Protocol），并支持两条独立的交互信道：

1. **WebUI 信道**：`WS /sessions/{sessionId}/events`，双向（客户端发送 `RunAgentInput`，服务端推送 AG-UI 事件）。
2. **HTTP POST + SSE 信道**：`POST /sessions/{sessionId}/runs`，请求体为 AG-UI `RunAgentInput`，响应为 SSE 流。

本次范围仅限 `apps/gatewayd`，不改造桌面端 `src-tauri`。现有 `/agents/*` 与 `/agents/events` 接口在本次改造中废弃，由 session 维度的新接口替代。

## 背景与约束

- `opencode-plugin` 与 `claude-plugin` 仍输出内部 `ProcessEvent`，再由 `agent-core::process::EventMapper` 转成 `agent.token` / `agent.thinking` / `agent.done` 等 JSON-RPC 通知。
- 本次改造不触及插件与 `agent-core` 内部事件模型，仅在 `gatewayd` 侧做**二次映射**：把现有的 JSON-RPC 通知映射为 AG-UI 标准事件。
- 需要引入 `session` 概念：一个 session 可以挂载一个 agent 实例（opencode 或 claude），sessionId 复用为 AG-UI 的 `threadId`。

## 架构设计

```
┌─────────────────────────────────────────────────────────────────────┐
│                          客户端（CLI / Web / 桌面端）                  │
│  ┌─────────────────────────────┐  ┌────────────────────────────────┐│
│  │ WS /sessions/{id}/events    │  │ POST /sessions/{id}/runs (SSE) ││
│  │  发送 RunAgentInput          │  │  请求体 RunAgentInput           ││
│  │  接收 AG-UI 事件             │  │  接收 AG-UI SSE 事件            ││
│  └─────────────┬───────────────┘  └──────────────┬─────────────────┘│
└────────────────┼────────────────────────────────┼──────────────────┘
                 │                                │
                 ▼                                ▼
        ┌─────────────────────────────────────────────────────┐
        │              dh-gatewayd (axum)                      │
        │  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐ │
        │  │ Session API  │  │ WebSocket    │  │ SSE Handler │ │
        │  │ /sessions/*  │  │ handler      │  │             │ │
        │  └──────┬───────┘  └──────┬───────┘  └──────┬──────┘ │
        │         │                 │                  │        │
        │         └─────────────────┼──────────────────┘        │
        │                           ▼                           │
        │              ┌───────────────────────┐                │
        │              │   SessionManager      │                │
        │              │  - session registry   │                │
        │              │  - instance→session   │                │
        │              │  - per-session broadcast│               │
        │              └───────────┬───────────┘                │
        │                          │                            │
        │         ┌────────────────┼────────────────┐           │
        │         ▼                ▼                ▼           │
        │  ┌────────────┐  ┌──────────────┐  ┌──────────────┐  │
        │  │ AguiMapper │  │ AguiEventSink│  │ AgentService │  │
        │  │ ProcessEvent│  │ (EventSink)  │  │ (unchanged)  │  │
        │  │ → AG-UI    │  │              │  │              │  │
        │  └────────────┘  └──────────────┘  └──────────────┘  │
        │         ▲                │                ▲           │
        │         │                │                │           │
        │         └────────────────┴────────────────┘           │
        │                           │                           │
        │              ┌────────────┴────────────┐              │
        │              │   opencode-plugin /       │              │
        │              │   claude-plugin           │              │
        │              │   (ProcessEvent + JSON-RPC│              │
        │              │    notifications)         │              │
        │              └───────────────────────────┘              │
        └─────────────────────────────────────────────────────┘
```

## 关键抽象

### SessionManager

每个 `Session` 包含：

- `session_id: String`（同时作为 AG-UI `threadId`）
- `instances: Vec<String>`（挂载的 agent instance id 列表，MVP 限制为 1 个）
- `event_tx: broadcast::Sender<AguiEvent>`（该 session 的事件广播）
- `state: Arc<Mutex<serde_json::Value>>`（AG-UI 共享状态，MVP 可空对象）

`SessionManager` 负责：

- `create_session()` → 生成 UUID sessionId
- `create_agent(session_id, plugin_key, name, workspace)` → 调用 `AgentService::create_instance`，记录 instance→session 映射
- `start_run(session_id, RunAgentInput)` → 生成 runId，调用对应 agent 实例的 `send_message`
- `route_jsonrpc_event(instance_id, method, payload)` → 找到 session，交给 `AguiMapper` 转换后广播

### AguiEventSink

实现 `agent_core::event_sink::EventSink`，接收插件当前发出的 JSON-RPC 通知：

```rust
impl EventSink for AguiEventSink {
    fn emit(&self, event_type: &str, payload: serde_json::Value) {
        // 1. 从 payload 读取 instance_id
        // 2. 通过 SessionManager 找到 session
        // 3. 交给 AguiMapper 生成 Vec<AguiEvent>
        // 4. 广播到 session.event_tx
    }
}
```

### AguiMapper

维护每个 run 的临时状态（当前 message_id、tool_call_id），把离散通知转换为 AG-UI 的 Start/Content/End 序列：

| 来源 JSON-RPC 通知 | AG-UI 输出事件序列 |
|--------------------|--------------------|
| `agent.token` { text } | 首次 token：`TextMessageStart` + `TextMessageContent`；后续：`TextMessageContent`；收到 `agent.done` 或 `agent.error` 时补 `TextMessageEnd` |
| `agent.thinking` { content, type: "thinking" } | `ThinkingTextMessageStart` + `ThinkingTextMessageContent` + `ThinkingTextMessageEnd`（单条 thinking 可合并为一条 Content） |
| `agent.thinking` { type: "tool_use", content/toolName } | `ToolCallStart` + `ToolCallArgs` |
| `agent.thinking` { type: "tool_result", content, toolName, failed } | `ToolCallResult` |
| `agent.permission` / `agent.question` / `agent.todowrite` | `Custom` 事件（name 对应原类型） |
| `agent.done` | `TextMessageEnd`（如未完成）+ `RunFinished` |
| `agent.error` { message } | `TextMessageEnd`（如未完成）+ `RunError` |
| `agent:status_changed` | `Custom` 事件 `status_changed`（或 MVP 中忽略） |
| `session.log` | `Raw` 事件（source: "session-log"） |

## API 接口

### 创建 Session

```http
POST /sessions
Content-Type: application/json

Response 201:
{
  "sessionId": "550e8400-e29b-41d4-a716-446655440000"
}
```

### 在 Session 下创建 Agent

```http
POST /sessions/{sessionId}/agents
Content-Type: application/json

Body:
{
  "plugin_key": "opencode",   // 或 "claude-code"
  "name": "my-opencode",
  "workspace": "/path/to/project"
}

Response 201:
{
  "instance_id": "inst_xxx",
  "plugin_key": "opencode",
  "name": "my-opencode",
  "status": "running"
}
```

MVP 约定：一个 session 同一时间只允许挂载一个 agent 实例。重复创建返回 `409 Conflict`。

### WebUI 信道：双向 WebSocket

```
GET /sessions/{sessionId}/events
Upgrade: websocket
```

**客户端 → 服务端**：发送 AG-UI `RunAgentInput` JSON：

```json
{
  "threadId": "550e8400-e29b-41d4-a716-446655440000",
  "runId": "optional-run-id-or-server-generates",
  "messages": [
    { "role": "user", "content": "帮我重构这段代码" }
  ],
  "state": {},
  "tools": [],
  "context": [],
  "forwardedProps": {}
}
```

**服务端 → 客户端**：AG-UI 事件 JSON，作为 WebSocket text frame：

```json
{ "type": "RUN_STARTED", "threadId": "...", "runId": "..." }
{ "type": "TEXT_MESSAGE_START", "messageId": "msg-1", "role": "assistant" }
{ "type": "TEXT_MESSAGE_CONTENT", "messageId": "msg-1", "delta": "好的" }
{ "type": "TEXT_MESSAGE_END", "messageId": "msg-1" }
{ "type": "RUN_FINISHED", "threadId": "...", "runId": "..." }
```

### HTTP POST + SSE 信道

```http
POST /sessions/{sessionId}/runs
Content-Type: application/json
Accept: text/event-stream

Body: 同 RunAgentInput

Response 200:
Content-Type: text/event-stream
Cache-Control: no-cache
Connection: keep-alive

data: {"type":"RUN_STARTED","threadId":"...","runId":"..."}

data: {"type":"TEXT_MESSAGE_START","messageId":"...","role":"assistant"}

data: {"type":"TEXT_MESSAGE_CONTENT","messageId":"...","delta":"hello"}

data: {"type":"TEXT_MESSAGE_END","messageId":"..."}

data: {"type":"RUN_FINISHED","threadId":"...","runId":"..."}

```

## AG-UI 事件类型（本次实现子集）

基于 AG-UI 官方 Rust SDK 的字段命名，使用 `SCREAMING_SNAKE_CASE` 的 `type` 标签：

- `RUN_STARTED` / `RUN_FINISHED` / `RUN_ERROR`
- `TEXT_MESSAGE_START` / `TEXT_MESSAGE_CONTENT` / `TEXT_MESSAGE_END`
- `THINKING_TEXT_MESSAGE_START` / `THINKING_TEXT_MESSAGE_CONTENT` / `THINKING_TEXT_MESSAGE_END`
- `TOOL_CALL_START` / `TOOL_CALL_ARGS` / `TOOL_CALL_END` / `TOOL_CALL_RESULT`
- `STATE_SNAPSHOT`（MVP 在 run 开始时回显输入 state）
- `CUSTOM`（用于 permission / question / todowrite / status_changed）
- `RAW`（用于 session.log）

## 文件组织

为避免 `main.rs` 继续膨胀（当前已 959 行，超过 AGENTS.md 600 行限制），新增以下模块：

```
apps/gatewayd/src/
├── main.rs                  # 仅保留启动、router 注册、状态组装
├── agents_impl.rs           # 保留，但移除旧 REST/WS handler，保留 AgentService 初始化
├── session.rs               # SessionManager、Session、Instance→Session 路由
├── agui/
│   ├── mod.rs               # 模块导出
│   ├── types.rs             # AG-UI Event / RunAgentInput / Message / Role 等类型
│   └── mapper.rs            # JSON-RPC → AG-UI 映射状态机
├── agui_sink.rs             # AguiEventSink 实现 EventSink
└── handlers/
    ├── mod.rs
    ├── session.rs           # POST /sessions, POST /sessions/:id/agents
    ├── websocket.rs         # WS /sessions/:id/events
    └── sse.rs               # POST /sessions/:id/runs
```

## 输入处理流程

1. 客户端通过 WebSocket 或 POST 发送 `RunAgentInput`。
2. 服务端校验 `threadId == URL sessionId`；若 `runId` 为空则生成 UUID。
3. 立即向该 session 广播 `RUN_STARTED`。
4. 从 `messages` 中取出最后一条 `role == "user"` 的消息内容。
5. 调用 `AgentInstance::send_message(sessionId, content)`，其中 `sessionId` 作为 `conversation_id` 传给插件。
6. 插件产生的 JSON-RPC 通知进入 `AguiEventSink`。
7. `AguiEventSink` 通过 instance_id 找到 session，由 `AguiMapper` 转成 AG-UI 事件并广播。
8. WebSocket 与 SSE 订阅者同时收到事件。
9. 当收到 `agent.done` 或流结束时，广播 `RUN_FINISHED`；出错时广播 `RUN_ERROR`。

## 错误处理

- Session 不存在：返回 `404 Not Found`
- Session 下无 agent 实例：返回 `409 Conflict` 或 `422 Unprocessable Entity`
- 插件启动/发送失败：广播 `RUN_ERROR`，并关闭 SSE / 保持 WebSocket 可用
- 客户端断开：清理该连接订阅，不停止 agent 进程

## 状态与工具

- **State**：MVP 只读回显。`RunAgentInput.state` 在 `RUN_STARTED` 后作为 `STATE_SNAPSHOT` 广播一次。`STATE_DELTA` 暂不实现。
- **Tools**：MVP 透传 `RunAgentInput.tools` 到插件内部工具配置，但 AG-UI 侧不主动管理工具调用生命周期（仍由插件内部完成）。

## 废弃接口

以下 gatewayd 旧接口在本次改造后不再可用，由新接口替代：

| 旧接口 | 替代接口 |
|--------|----------|
| `POST /agents` | `POST /sessions` + `POST /sessions/{id}/agents` |
| `GET /agents` | 暂无（如需列出可后续加 `GET /sessions`） |
| `GET /agents/{id}` | `GET /sessions/{sessionId}/agents/{agentId}`（后续） |
| `POST /agents/{id}/message` | `WS /sessions/{id}/events` 或 `POST /sessions/{id}/runs` |
| `DELETE /agents/{id}` | `DELETE /sessions/{sessionId}/agents/{agentId}`（后续） |
| `GET /agents/events` | `GET /sessions/{sessionId}/events` |

## 测试策略

1. **单元测试**：`AguiMapper` 对常见 JSON-RPC 通知序列的映射结果。
2. **集成测试**：启动 `dh-gatewayd`，通过 `curl` + `websocat` 或简单脚本验证完整对话流。
3. **双信道并发**：同一 session 同时连接 WebSocket 与 SSE，确认事件一致。
4. **错误场景**：session 不存在、未挂载 agent、插件崩溃时的 `RUN_ERROR`。

## 后续扩展

- 桌面端 `src-tauri` 接入同一套 AG-UI 映射层。
- 支持一个 session 挂载多个 agent 实例，并通过 `forwardedProps.agent_id` 选择。
- 实现 `STATE_DELTA` 与前端 tool 回调（`Custom` 事件双向）。
- 引入 `ag-ui-core` crate 作为依赖，替换自研类型。
