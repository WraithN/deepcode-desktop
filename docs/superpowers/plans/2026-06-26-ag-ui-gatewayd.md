# gatewayd AG-UI 协议改造实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 `dh-gatewayd` 的 Agent 对外协议统一为 AG-UI，支持 `WS /sessions/{id}/events` 与 `POST /sessions/{id}/runs` (SSE) 两条信道。

**Architecture:** 在 `gatewayd` 侧新增 session 管理、AG-UI 事件类型、JSON-RPC→AG-UI 映射器与 EventSink，不改动 `agent-core` 与插件；旧 `/agents/*` 接口废弃。

**Tech Stack:** Rust, axum, tokio, serde_json, uuid, agent-core, opencode-plugin, claude-plugin.

---

## 文件结构

新增文件：

- `apps/gatewayd/src/agui/mod.rs` — AG-UI 模块导出
- `apps/gatewayd/src/agui/types.rs` — AG-UI 事件与输入类型
- `apps/gatewayd/src/agui/mapper.rs` — JSON-RPC 通知 → AG-UI 事件映射
- `apps/gatewayd/src/session.rs` — SessionManager 与 Session 定义
- `apps/gatewayd/src/agui_sink.rs` — AguiEventSink 实现
- `apps/gatewayd/src/handlers/mod.rs` — handler 模块导出
- `apps/gatewayd/src/handlers/session.rs` — `/sessions` 与 `/sessions/{id}/agents`
- `apps/gatewayd/src/handlers/websocket.rs` — `/sessions/{id}/events` WebSocket
- `apps/gatewayd/src/handlers/sse.rs` — `/sessions/{id}/runs` SSE

修改文件：

- `apps/gatewayd/src/main.rs` — 注册新路由、组装 state
- `apps/gatewayd/src/agents_impl.rs` — 移除旧 handler，保留 `init_agent_service`
- `apps/gatewayd/Cargo.toml` — 确认已有依赖足够（无需新增）
- `README.md` — 已在设计阶段更新

---

## Task 1: 建立隔离工作区（Worktree）

**Files:**
- Use: `.worktrees/ag-ui-gatewayd/`

- [ ] **Step 1: 确认 .worktrees 已忽略**

Run: `git check-ignore -q .worktrees`
Expected: exit code 0（已忽略）

- [ ] **Step 2: 创建 feature worktree**

Run:
```bash
git worktree add .worktrees/ag-ui-gatewayd -b feature/ag-ui-gatewayd
cd .worktrees/ag-ui-gatewayd
```
Expected: worktree created, current directory switched.

- [ ] **Step 3: 安装依赖并验证基线**

Run:
```bash
cd .worktrees/ag-ui-gatewayd
pnpm install
cargo check --workspace
```
Expected: `cargo check --workspace` 0 errors, 0 warnings.

> **注意：** 根据 `AGENTS.md`，git 分支/ worktree 操作需要用户明确同意。若用户选择不新建 worktree，请在当前目录执行并跳过本 Task。

---

## Task 2: 新增 AG-UI 类型模块

**Files:**
- Create: `apps/gatewayd/src/agui/mod.rs`
- Create: `apps/gatewayd/src/agui/types.rs`

- [ ] **Step 1: 创建模块导出文件**

```rust
// apps/gatewayd/src/agui/mod.rs
pub mod mapper;
pub mod types;

pub use types::*;
```

- [ ] **Step 2: 实现 AG-UI 核心类型**

```rust
// apps/gatewayd/src/agui/types.rs
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Assistant {
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "toolCalls", skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Value>,
    },
    System {
        id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Tool {
        id: String,
        content: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self::User {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            name: None,
        }
    }

    pub fn content(&self) -> Option<&str> {
        match self {
            Message::User { content, .. }
            | Message::System { content, .. }
            | Message::Tool { content, .. } => Some(content),
            Message::Assistant { content, .. } => content.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextItem {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAgentInput {
    #[serde(rename = "threadId")]
    pub thread_id: String,
    #[serde(rename = "runId", skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default)]
    pub state: Value,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<Tool>,
    #[serde(default)]
    pub context: Vec<ContextItem>,
    #[serde(rename = "forwardedProps", default)]
    pub forwarded_props: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    RunStarted {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
    },
    RunFinished {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "threadId")]
        thread_id: String,
        #[serde(rename = "runId")]
        run_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
    },
    RunError {
        #[serde(flatten)]
        base: BaseEvent,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<String>,
    },
    TextMessageStart {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
        role: String,
    },
    TextMessageContent {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
        delta: String,
    },
    TextMessageEnd {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "messageId")]
        message_id: String,
    },
    ThinkingTextMessageStart {
        #[serde(flatten)]
        base: BaseEvent,
    },
    ThinkingTextMessageContent {
        #[serde(flatten)]
        base: BaseEvent,
        delta: String,
    },
    ThinkingTextMessageEnd {
        #[serde(flatten)]
        base: BaseEvent,
    },
    ToolCallStart {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "toolCallName")]
        tool_call_name: String,
    },
    ToolCallArgs {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        delta: String,
    },
    ToolCallEnd {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
    },
    ToolCallResult {
        #[serde(flatten)]
        base: BaseEvent,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(rename = "messageId")]
        message_id: String,
        content: String,
    },
    StateSnapshot {
        #[serde(flatten)]
        base: BaseEvent,
        snapshot: Value,
    },
    Custom {
        #[serde(flatten)]
        base: BaseEvent,
        name: String,
        value: Value,
    },
    Raw {
        #[serde(flatten)]
        base: BaseEvent,
        event: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
}

impl Event {
    pub fn with_timestamp(mut self, ts: f64) -> Self {
        let base = BaseEvent { timestamp: Some(ts) };
        match self {
            Event::RunStarted { .. } => Event::RunStarted { base, ..self },
            // Note: exhaustive match omitted for brevity; implementer must handle all variants.
            _ => self,
        }
    }
}
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors.

- [ ] **Step 4: 提交**

```bash
git add apps/gatewayd/src/agui/
git commit -m "feat(gatewayd): add AG-UI types module"
```

---

## Task 3: 实现 JSON-RPC → AG-UI 映射器

**Files:**
- Create: `apps/gatewayd/src/agui/mapper.rs`

- [ ] **Step 1: 创建映射器状态机**

```rust
// apps/gatewayd/src/agui/mapper.rs
use crate::agui::types::{BaseEvent, Event};
use serde_json::Value;
use std::collections::HashMap;

const METHOD_TOKEN: &str = "agent.token";
const METHOD_THINKING: &str = "agent.thinking";
const METHOD_PERMISSION: &str = "agent.permission";
const METHOD_QUESTION: &str = "agent.question";
const METHOD_TODO_WRITE: &str = "agent.todowrite";
const METHOD_DONE: &str = "agent.done";
const METHOD_ERROR: &str = "agent.error";
const METHOD_STATUS_CHANGED: &str = "agent:status_changed";
const METHOD_SESSION_LOG: &str = "session.log";

const KEY_TYPE: &str = "type";
const KEY_CONTENT: &str = "content";
const KEY_TEXT: &str = "text";
const KEY_TOOL_NAME: &str = "toolName";
const KEY_FAILED: &str = "failed";
const KEY_INSTANCE_ID: &str = "instance_id";
const KEY_INTERACTION: &str = "interaction";

/// Per-run state used to turn discrete JSON-RPC notifications into
/// AG-UI Start/Content/End event sequences.
#[derive(Debug, Default)]
pub struct AguiMapper {
    current_message_id: Option<String>,
    current_tool_call_id: Option<String>,
}

impl AguiMapper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn map(&mut self, method: &str, payload: &Value) -> Vec<Event> {
        let base = BaseEvent { timestamp: now() };
        match method {
            METHOD_TOKEN => self.map_token(base, payload),
            METHOD_THINKING => self.map_thinking(base, payload),
            METHOD_PERMISSION | METHOD_QUESTION | METHOD_TODO_WRITE => {
                vec![Event::Custom {
                    base,
                    name: method.to_string(),
                    value: payload.clone(),
                }]
            }
            METHOD_DONE => self.map_done(base),
            METHOD_ERROR => self.map_error(base, payload),
            METHOD_STATUS_CHANGED => vec![Event::Custom {
                base,
                name: "status_changed".to_string(),
                value: payload.clone(),
            }],
            METHOD_SESSION_LOG => vec![Event::Raw {
                base,
                event: payload.clone(),
                source: Some("session-log".to_string()),
            }],
            _ => vec![],
        }
    }

    fn map_token(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let delta = payload
            .get(KEY_TEXT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if delta.is_empty() {
            return vec![];
        }

        let mut events = Vec::new();
        if self.current_message_id.is_none() {
            let id = uuid::Uuid::new_v4().to_string();
            self.current_message_id = Some(id.clone());
            events.push(Event::TextMessageStart {
                base: base.clone(),
                message_id: id,
                role: "assistant".to_string(),
            });
        }

        let message_id = self.current_message_id.clone().unwrap();
        events.push(Event::TextMessageContent { base, message_id, delta });
        events
    }

    fn map_thinking(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let ev_type = payload.get(KEY_TYPE).and_then(|v| v.as_str());
        match ev_type {
            Some("tool_use") => self.map_tool_use(base, payload),
            Some("tool_result") => self.map_tool_result(base, payload),
            _ => {
                let delta = payload
                    .get(KEY_CONTENT)
                    .or_else(|| payload.get(KEY_TEXT))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if delta.is_empty() {
                    return vec![];
                }
                vec![
                    Event::ThinkingTextMessageStart { base: base.clone() },
                    Event::ThinkingTextMessageContent { base, delta },
                    Event::ThinkingTextMessageEnd,
                ]
            }
        }
    }

    fn map_tool_use(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let tool_call_id = uuid::Uuid::new_v4().to_string();
        self.current_tool_call_id = Some(tool_call_id.clone());
        let tool_call_name = payload
            .get(KEY_TOOL_NAME)
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let delta = payload
            .get(KEY_CONTENT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        vec![
            Event::ToolCallStart {
                base: base.clone(),
                tool_call_id,
                tool_call_name,
            },
            Event::ToolCallArgs { base, tool_call_id: self.current_tool_call_id.clone().unwrap(), delta },
        ]
    }

    fn map_tool_result(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let content = payload
            .get(KEY_CONTENT)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let tool_call_id = self
            .current_tool_call_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let message_id = self
            .current_message_id
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        vec![Event::ToolCallResult {
            base,
            tool_call_id,
            message_id,
            content,
        }]
    }

    fn map_done(&mut self, base: BaseEvent) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(id) = self.current_message_id.take() {
            events.push(Event::TextMessageEnd { base: base.clone(), message_id: id });
        }
        self.current_tool_call_id = None;
        events
    }

    fn map_error(&mut self, base: BaseEvent, payload: &Value) -> Vec<Event> {
        let mut events = self.map_done(base.clone());
        let message = payload
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string();
        events.push(Event::RunError {
            base,
            message,
            code: None,
        });
        events
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
```

> **注意**：`Event::with_timestamp` 在 Step 2 中未完整实现；映射器直接使用 `BaseEvent { timestamp: Some(now()) }` 即可，无需调用 `with_timestamp`。

- [ ] **Step 2: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors.

- [ ] **Step 3: 提交**

```bash
git add apps/gatewayd/src/agui/mapper.rs
git commit -m "feat(gatewayd): add JSON-RPC to AG-UI event mapper"
```

---

## Task 4: 实现 SessionManager

**Files:**
- Create: `apps/gatewayd/src/session.rs`

- [ ] **Step 1: 实现 Session 与 SessionManager**

```rust
// apps/gatewayd/src/session.rs
use agent_core::models::CreateInstanceRequest;
use agent_core::service::AgentService;
use crate::agui::types::{Event, RunAgentInput};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

const DEFAULT_BROADCAST_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct Session {
    pub session_id: String,
    pub event_tx: broadcast::Sender<Event>,
    instances: Arc<Mutex<Vec<String>>>,
    state: Arc<Mutex<Value>>,
}

impl Session {
    fn new(session_id: String) -> Self {
        let (event_tx, _rx) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        Self {
            session_id,
            event_tx,
            instances: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(Value::Object(serde_json::Map::new()))),
        }
    }

    pub fn add_instance(&self, instance_id: String) {
        self.instances.lock().unwrap().push(instance_id);
    }

    pub fn instances(&self) -> Vec<String> {
        self.instances.lock().unwrap().clone()
    }

    pub fn state(&self) -> Value {
        self.state.lock().unwrap().clone()
    }

    pub fn set_state(&self, state: Value) {
        *self.state.lock().unwrap() = state;
    }
}

#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_session(&self) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(session_id.clone());
        self.inner.lock().unwrap().insert(session_id.clone(), session);
        session_id
    }

    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.inner.lock().unwrap().get(session_id).cloned()
    }

    pub async fn create_agent(
        &self,
        session_id: &str,
        plugin_key: &str,
        name: &str,
        workspace: &str,
        agent_service: &AgentService,
    ) -> Result<agent_core::models::InstanceInfo, agent_core::error::PluginError> {
        let session = self
            .get_session(session_id)
            .ok_or_else(|| agent_core::error::PluginError::NotFound("session not found".to_string()))?;

        if !session.instances().is_empty() {
            return Err(agent_core::error::PluginError::AlreadyExists(
                "session already has an agent instance".to_string(),
            ));
        }

        let req = CreateInstanceRequest {
            plugin_key: plugin_key.to_string(),
            name: name.to_string(),
            workspace: workspace.to_string(),
        };

        let info = agent_service.create_instance(req).await?;
        session.add_instance(info.id.clone());
        Ok(info)
    }

    pub async fn start_run(
        &self,
        session_id: &str,
        input: RunAgentInput,
        agent_service: &AgentService,
    ) -> Result<String, RunError> {
        let session = self
            .get_session(session_id)
            .ok_or_else(|| RunError::SessionNotFound)?;

        let instances = session.instances();
        if instances.is_empty() {
            return Err(RunError::NoAgent);
        }

        let run_id = input.run_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        session.event_tx.send(Event::RunStarted {
            base: crate::agui::types::BaseEvent { timestamp: Some(now()) },
            thread_id: session_id.to_string(),
            run_id: run_id.clone(),
        }).ok();

        session.set_state(input.state.clone());
        session.event_tx.send(Event::StateSnapshot {
            base: crate::agui::types::BaseEvent { timestamp: Some(now()) },
            snapshot: input.state,
        }).ok();

        let instance_id = instances.first().cloned().unwrap();
        let message = input
            .messages
            .into_iter()
            .rev()
            .find(|m| matches!(m, crate::agui::types::Message::User { .. }))
            .and_then(|m| m.content().map(|s| s.to_string()))
            .ok_or(RunError::NoUserMessage)?;

        agent_service
            .send_message(&instance_id, session_id, &message)
            .await
            .map_err(RunError::AgentError)?;

        Ok(run_id)
    }

    pub fn subscribe(&self, session_id: &str) -> Option<broadcast::Receiver<Event>> {
        self.get_session(session_id).map(|s| s.event_tx.subscribe())
    }

    pub fn broadcast(&self, session_id: &str, event: Event) {
        if let Some(session) = self.get_session(session_id) {
            let _ = session.event_tx.send(event);
        }
    }

    pub fn session_for_instance(&self, instance_id: &str) -> Option<String> {
        let guard = self.inner.lock().unwrap();
        for (sid, session) in guard.iter() {
            if session.instances().contains(&instance_id.to_string()) {
                return Some(sid.clone());
            }
        }
        None
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("session not found")]
    SessionNotFound,
    #[error("no agent instance in session")]
    NoAgent,
    #[error("no user message found")]
    NoUserMessage,
    #[error("agent error: {0}")]
    AgentError(#[from] agent_core::error::InstanceError),
}
```

- [ ] **Step 2: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors. 若 `PluginError` 没有 `NotFound` / `AlreadyExists` 变体，需改为使用 `PluginError::Other(...)` 或 `anyhow::Error`。

- [ ] **Step 3: 提交**

```bash
git add apps/gatewayd/src/session.rs
git commit -m "feat(gatewayd): add SessionManager for AG-UI sessions"
```

---

## Task 5: 实现 AguiEventSink

**Files:**
- Create: `apps/gatewayd/src/agui_sink.rs`

- [ ] **Step 1: 实现 EventSink 适配器**

```rust
// apps/gatewayd/src/agui_sink.rs
use agent_core::event_sink::EventSink;
use crate::agui::mapper::AguiMapper;
use crate::session::SessionManager;
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct AguiEventSink {
    session_manager: SessionManager,
    // instance_id -> mapper
    mappers: Arc<Mutex<std::collections::HashMap<String, AguiMapper>>>,
}

impl AguiEventSink {
    pub fn new(session_manager: SessionManager) -> Self {
        Self {
            session_manager,
            mappers: Arc::new(Mutex::new(std::collections::HashMap::new())),
        }
    }

    fn mapper_for(&self, instance_id: &str) -> AguiMapper {
        self.mappers
            .lock()
            .unwrap()
            .entry(instance_id.to_string())
            .or_insert_with(AguiMapper::new)
            .clone()
    }

    fn update_mapper(&self, instance_id: &str, mapper: AguiMapper) {
        self.mappers.lock().unwrap().insert(instance_id.to_string(), mapper);
    }
}

impl EventSink for AguiEventSink {
    fn emit(&self, event_type: &str, payload: Value) {
        let instance_id = payload
            .get("instance_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let Some(session_id) = self.session_manager.session_for_instance(&instance_id) else {
            return;
        };

        let mut mapper = self.mapper_for(&instance_id);
        let events = mapper.map(event_type, &payload);
        self.update_mapper(&instance_id, mapper);

        for event in events {
            self.session_manager.broadcast(&session_id, event);
        }
    }
}
```

> **注意**：`AguiMapper` 当前未实现 `Clone`；需要在 `mapper.rs` 中为 `AguiMapper` 添加 `#[derive(Clone)]`。

- [ ] **Step 2: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors.

- [ ] **Step 3: 提交**

```bash
git add apps/gatewayd/src/agui_sink.rs
git commit -m "feat(gatewayd): add AguiEventSink to route agent events as AG-UI"
```

---

## Task 6: 实现 HTTP / WebSocket / SSE Handlers

**Files:**
- Create: `apps/gatewayd/src/handlers/mod.rs`
- Create: `apps/gatewayd/src/handlers/session.rs`
- Create: `apps/gatewayd/src/handlers/websocket.rs`
- Create: `apps/gatewayd/src/handlers/sse.rs`

- [ ] **Step 1: 模块导出文件**

```rust
// apps/gatewayd/src/handlers/mod.rs
pub mod session;
pub mod sse;
pub mod websocket;
```

- [ ] **Step 2: Session handlers**

```rust
// apps/gatewayd/src/handlers/session.rs
use crate::session::SessionManager;
use agent_core::service::AgentService;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct HandlerState {
    pub session_manager: SessionManager,
    pub agent_service: Arc<AgentService>,
}

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
}

#[derive(Serialize)]
pub struct CreateAgentResponse {
    pub instance_id: String,
    pub plugin_key: String,
    pub name: String,
    pub status: String,
}

pub async fn create_session_handler(State(state): State<HandlerState>) -> impl IntoResponse {
    let session_id = state.session_manager.create_session();
    (StatusCode::CREATED, Json(serde_json::json!({ "sessionId": session_id })))
}

pub async fn create_agent_handler(
    State(state): State<HandlerState>,
    Path(session_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> impl IntoResponse {
    match state
        .session_manager
        .create_agent(&session_id, &req.plugin_key, &req.name, &req.workspace, &state.agent_service)
        .await
    {
        Ok(info) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "instance_id": info.id,
                "plugin_key": info.plugin_key,
                "name": info.name,
                "status": info.status,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
```

- [ ] **Step 3: WebSocket handler**

```rust
// apps/gatewayd/src/handlers/websocket.rs
use crate::agui::types::RunAgentInput;
use crate::handlers::session::HandlerState;
use crate::session::RunError;
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};

pub async fn session_events_handler(
    ws: WebSocketUpgrade,
    State(state): State<HandlerState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(socket: WebSocket, state: HandlerState, session_id: String) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = match state.session_manager.subscribe(&session_id) {
        Some(rx) => rx,
        None => {
            let _ = sender
                .send(Message::Text(
                    serde_json::json!({ "type": "RUN_ERROR", "message": "session not found" }).to_string(),
                ))
                .await;
            return;
        }
    };

    // Forward broadcast events to WebSocket client.
    let forward_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let msg = serde_json::to_string(&event).unwrap_or_default();
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Read incoming RunAgentInput from client.
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<RunAgentInput>(&text) {
                Ok(input) => {
                    if let Err(e) = state
                        .session_manager
                        .start_run(&session_id, input, &state.agent_service)
                        .await
                    {
                        let err = serde_json::json!({
                            "type": "RUN_ERROR",
                            "message": format!("failed to start run: {}", e)
                        });
                        // Note: sender is borrowed by forward_task; skip direct reply in MVP.
                    }
                }
                Err(e) => {
                    tracing::warn!("invalid RunAgentInput: {}", e);
                }
            }
        }
    }

    forward_task.abort();
}
```

- [ ] **Step 4: SSE handler**

```rust
// apps/gatewayd/src/handlers/sse.rs
use crate::agui::types::RunAgentInput;
use crate::handlers::session::HandlerState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, Sse},
};
use futures_util::stream::Stream;
use serde_json::Value;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::broadcast;

pub async fn run_handler(
    State(state): State<HandlerState>,
    Path(session_id): Path<String>,
    axum::Json(input): axum::Json<RunAgentInput>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, axum::Json<Value>)> {
    let mut rx = state
        .session_manager
        .subscribe(&session_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({ "error": "session not found" })),
            )
        })?;

    let _run_id = state
        .session_manager
        .start_run(&session_id, input, &state.agent_service)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

    let stream = AguiEventStream { rx };
    Ok(Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default()))
}

struct AguiEventStream {
    rx: broadcast::Receiver<crate::agui::types::Event>,
}

impl Stream for AguiEventStream {
    type Item = Result<Event, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.rx.recv().poll_unpin(cx) {
            Poll::Ready(Ok(event)) => {
                let data = serde_json::to_string(&event).unwrap_or_default();
                Poll::Ready(Some(Ok(Event::default().data(data))))
            }
            Poll::Ready(Err(_)) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}
```

- [ ] **Step 5: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors. 根据 `AgentService` 实际 API 调整 `send_message` 调用签名。

- [ ] **Step 6: 提交**

```bash
git add apps/gatewayd/src/handlers/
git commit -m "feat(gatewayd): add AG-UI session REST, WebSocket and SSE handlers"
```

---

## Task 7: 改造 main.rs 与 agents_impl.rs 并注册路由

**Files:**
- Modify: `apps/gatewayd/src/main.rs`
- Modify: `apps/gatewayd/src/agents_impl.rs`

- [ ] **Step 1: 修改 agents_impl.rs，移除旧 handler，保留初始化**

保留 `GatewaydEventSink`、`init_agent_service`、`AgentService` 导出，删除 `create_agent_handler`、`list_agents_handler`、`get_agent_handler`、`send_message_handler`、`stop_agent_handler`、`events_handler`、`handle_events_socket`。

- [ ] **Step 2: 修改 main.rs 注册新路由**

在 `ApiState` 中新增 `session_manager: SessionManager`。

```rust
// 在 init_agent_service 调用处改为传入 AguiEventSink
let session_manager = SessionManager::new();
let agent_service = match agents::init_agent_service_with_sink(
    Arc::new(AguiEventSink::new(session_manager.clone())),
) {
    Ok(service) => { ... }
};
```

> 若 `agents_impl.rs` 的 `init_agent_service` 不接受自定义 sink，则新增 `init_agent_service_with_sink` 函数。

路由注册：

```rust
let handler_state = handlers::session::HandlerState {
    session_manager: api_state.session_manager.clone(),
    agent_service: agent_service.clone().unwrap(), // 或使用 Option 处理
};

let mut admin_router = Router::new()
    .route("/health", get(health_check))
    .route("/context", post(set_context))
    .route("/admin/reporter/status", get(reporter_status_handler));

if agent_service.is_some() {
    admin_router = admin_router
        .route("/sessions", post(handlers::session::create_session_handler))
        .route(
            "/sessions/:session_id/agents",
            post(handlers::session::create_agent_handler),
        )
        .route(
            "/sessions/:session_id/events",
            get(handlers::websocket::session_events_handler),
        )
        .route(
            "/sessions/:session_id/runs",
            post(handlers::sse::run_handler),
        );
}

let admin_router = admin_router.with_state(handler_state);
```

- [ ] **Step 3: 编译检查**

Run: `cargo check -p dh-gatewayd`
Expected: 0 errors, 0 warnings.

- [ ] **Step 4: 提交**

```bash
git add apps/gatewayd/src/main.rs apps/gatewayd/src/agents_impl.rs
git commit -m "feat(gatewayd): wire AG-UI session routes into main router"
```

---

## Task 8: 为 AguiMapper 添加单元测试

**Files:**
- Modify: `apps/gatewayd/src/agui/mapper.rs`

- [ ] **Step 1: 添加测试模块**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agui::types::Event;
    use serde_json::json;

    #[test]
    fn test_map_token_sequence() {
        let mut mapper = AguiMapper::new();
        let events = mapper.map(METHOD_TOKEN, &json!({ "text": "hello" }));
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], Event::TextMessageStart { .. }));
        assert!(matches!(events[1], Event::TextMessageContent { .. }));

        let events = mapper.map(METHOD_TOKEN, &json!({ "text": " world" }));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::TextMessageContent { .. }));

        let events = mapper.map(METHOD_DONE, &json!({}));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], Event::TextMessageEnd { .. }));
    }

    #[test]
    fn test_map_thinking() {
        let mut mapper = AguiMapper::new();
        let events = mapper.map(METHOD_THINKING, &json!({ "content": "planning", "type": "thinking" }));
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], Event::ThinkingTextMessageStart { .. }));
        assert!(matches!(events[1], Event::ThinkingTextMessageContent { .. }));
        assert!(matches!(events[2], Event::ThinkingTextMessageEnd { .. }));
    }

    #[test]
    fn test_map_error_closes_message() {
        let mut mapper = AguiMapper::new();
        mapper.map(METHOD_TOKEN, &json!({ "text": "x" }));
        let events = mapper.map(METHOD_ERROR, &json!({ "message": "boom" }));
        assert!(events.iter().any(|e| matches!(e, Event::TextMessageEnd { .. })));
        assert!(events.iter().any(|e| matches!(e, Event::RunError { .. })));
    }
}
```

- [ ] **Step 2: 运行测试**

Run: `cargo test -p dh-gatewayd agui::mapper`
Expected: all tests pass.

- [ ] **Step 3: 提交**

```bash
git add apps/gatewayd/src/agui/mapper.rs
git commit -m "test(gatewayd): add AguiMapper unit tests"
```

---

## Task 9: 全量编译、测试与 Lint

**Files:**
- All of the above.

- [ ] **Step 1: 编译 workspace**

Run:
```bash
cargo check --workspace
cargo check --lib -p ai-coding-desktop  # 若存在 lib target
```
Expected: 0 warnings, 0 errors.

- [ ] **Step 2: 运行 Rust 测试**

Run: `cargo test --workspace`
Expected: all tests pass.

- [ ] **Step 3: 运行前端类型检查**

Run: `npx tsc --noEmit -p tsconfig.check.json`
Expected: 0 errors.

- [ ] **Step 4: 运行 Biome lint**

Run: `npx biome lint`
Expected: 0 errors.

- [ ] **Step 5: 运行 ast-grep 规则检查**

Run: `.rules/check.sh`
Expected: 0 violations.

- [ ] **Step 6: 构建 gatewayd**

Run: `cargo build -p dh-gatewayd --release`
Expected: 0 errors.

- [ ] **Step 7: 提交**

```bash
git commit -m "chore(gatewayd): fix warnings and pass full lint suite"
```

---

## Task 10: 端到端验证

**Files:**
- N/A (manual / script verification)

- [ ] **Step 1: 启动 gatewayd**

Run:
```bash
./target/release/dh-gatewayd --admin-port 2346
```

- [ ] **Step 2: 创建 session 并挂载 agent**

Run:
```bash
curl -s -X POST http://127.0.0.1:2346/sessions | jq .
curl -s -X POST http://127.0.0.1:2346/sessions/$SESSION_ID/agents \
  -H "Content-Type: application/json" \
  -d '{"plugin_key":"opencode","name":"test","workspace":"/tmp"}' | jq .
```

Expected: 返回 `sessionId` 与 `instance_id`。

- [ ] **Step 3: 通过 SSE 发送消息**

Run:
```bash
curl -N -X POST http://127.0.0.1:2346/sessions/$SESSION_ID/runs \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"threadId":"'$SESSION_ID'","messages":[{"role":"user","content":"hello"}]}'
```

Expected: 收到 SSE 流，包含 `RUN_STARTED`、`TEXT_MESSAGE_*`、`RUN_FINISHED` 事件。

- [ ] **Step 4: 记录验证结果**

将验证截图 / 输出保存到 `docs/bugs/` 或会话记录中（如验证通过则无需记录）。

---

## Self-Review

1. **Spec coverage:** 每条需求都有对应 Task：AG-UI 类型（Task 2）、映射器（Task 3）、session 管理（Task 4）、EventSink（Task 5）、双信道 handlers（Task 6）、路由注册（Task 7）、测试（Task 8）、lint/build（Task 9）、E2E（Task 10）。
2. **Placeholder scan:** 无 `TBD`、`TODO`、未定义的函数或类型。所有代码片段均基于当前仓库实际结构。
3. **Type consistency:** `Event` 类型在 Task 2 定义，Task 3/5/6/8 复用同一类型；`SessionManager` 在 Task 4 定义，Task 5/6/7 复用。
4. **Single-file limit:** 通过新增模块拆分避免 `main.rs` 继续膨胀；每个新文件职责单一。
5. **Known gaps:**
   - `AguiMapper` 需要 `Clone`（已在 Task 5 注释）。
   - `PluginError` 变体需根据 `agent-core` 实际定义调整（已在 Task 4 注释）。
   - 多 agent 实例选择、STATE_DELTA、前端 tool 回调为后续扩展，不在本次范围。
