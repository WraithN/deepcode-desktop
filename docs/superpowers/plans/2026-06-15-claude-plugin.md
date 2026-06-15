# Claude Code Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Claude Code agent plugin to DeepHarness by abstracting a reusable process-driver layer in `agent-core`, refactoring `opencode-plugin` to use it, and creating a new `claude-plugin`.

**Architecture:** Introduce `agent-core::process` module containing transport abstractions (`Transport`/`TransportHandle`), two implementations (`StdioTransport` for Claude CLI, `HttpTransport` for OpenCode), a unified `ProcessEvent` model, and an `EventMapper`. `opencode-plugin` migrates its HTTP/SSE logic to `HttpTransport`; `claude-plugin` spawns `claude -p --input-format=stream-json --output-format=stream-json` via `StdioTransport`.

**Tech Stack:** Rust, `tokio`, `async-trait`, `serde_json`, `reqwest`, `tokio-tungstenite` (already in tree), Tauri v2.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `crates/agent-core/src/instance.rs` | Extend `InstanceConfig` with `model`/`permission_mode`. |
| `crates/agent-core/src/process/mod.rs` | Re-export process module. |
| `crates/agent-core/src/process/transport.rs` | `Transport`/`TransportHandle` traits + `TransportError`. |
| `crates/agent-core/src/process/event.rs` | `ProcessEvent` enum and helpers. |
| `crates/agent-core/src/process/mapper.rs` | Map `ProcessEvent` to `EventSink` payloads. |
| `crates/agent-core/src/process/stdio.rs` | `StdioTransport` for stdin/stdout NDJSON. |
| `crates/agent-core/src/process/http.rs` | `HttpTransport` for HTTP + SSE. |
| `crates/opencode-plugin/src/instance.rs` | Refactor to use `HttpTransport`. |
| `crates/claude-plugin/Cargo.toml` | New crate manifest. |
| `crates/claude-plugin/src/lib.rs` | Re-export plugin. |
| `crates/claude-plugin/src/plugin.rs` | `ClaudePlugin` implementation. |
| `crates/claude-plugin/src/instance.rs` | `ClaudeInstance` implementation. |
| `crates/claude-plugin/src/parser.rs` | Parse Claude stream-json lines to `ProcessEvent`. |
| `src-tauri/Cargo.toml` | Add `claude-plugin` dependency. |
| `src-tauri/src/main.rs` | Register `ClaudePlugin` in `AgentService`. |

---

### Task 1: Extend `InstanceConfig`

**Files:**
- Modify: `crates/agent-core/src/instance.rs`
- Test: `crates/agent-core/src/instance.rs` (existing test)

- [ ] **Step 1: Add fields to `InstanceConfig`**

```rust
#[derive(Clone, Debug)]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
}
```

- [ ] **Step 2: Update existing test to include new fields**

```rust
#[test]
fn test_instance_config() {
    let cfg = InstanceConfig {
        id: "i-1".into(),
        name: "test".into(),
        workspace: "/tmp".into(),
        session_id: Some("s-1".into()),
        model: Some("sonnet".into()),
        permission_mode: Some("bypassPermissions".into()),
    };
    assert_eq!(cfg.id, "i-1");
    assert_eq!(cfg.model.as_deref(), Some("sonnet"));
}
```

- [ ] **Step 3: Run test**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p agent-core instance_config
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/agent-core/src/instance.rs
git commit -m "feat(agent-core): add model and permission_mode to InstanceConfig"
```

---

### Task 2: Define Process Transport Traits

**Files:**
- Create: `crates/agent-core/src/process/mod.rs`
- Create: `crates/agent-core/src/process/transport.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Create `crates/agent-core/src/process/transport.rs`**

```rust
use async_trait::async_trait;
use serde_json::Value;
use std::fmt;

#[derive(Debug)]
pub enum TransportError {
    ProcessStart(String),
    SendFailed(String),
    ReceiveFailed(String),
    Closed,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::ProcessStart(msg) => write!(f, "process start failed: {msg}"),
            TransportError::SendFailed(msg) => write!(f, "send failed: {msg}"),
            TransportError::ReceiveFailed(msg) => write!(f, "receive failed: {msg}"),
            TransportError::Closed => write!(f, "transport closed"),
        }
    }
}

impl std::error::Error for TransportError {}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError>;
    fn endpoint(&self) -> Option<String>;
}

#[async_trait]
pub trait TransportHandle: Send + Sync {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError>;
    async fn receive(&mut self) -> Result<Value, TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

- [ ] **Step 2: Create `crates/agent-core/src/process/mod.rs`**

```rust
pub mod event;
pub mod http;
pub mod mapper;
pub mod stdio;
pub mod transport;

pub use transport::{Transport, TransportError, TransportHandle};
```

- [ ] **Step 3: Export process module from `agent-core`**

Modify `crates/agent-core/src/lib.rs`:

```rust
pub mod error;
pub mod event;
pub mod event_sink;
pub mod instance;
pub mod logger;
pub mod mcp;
pub mod plugin;
pub mod process;
```

- [ ] **Step 4: Build check**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check -p agent-core
```

Expected: success, 0 warnings

- [ ] **Step 5: Commit**

```bash
git add crates/agent-core/src/lib.rs crates/agent-core/src/process/
git commit -m "feat(agent-core): add process transport traits"
```

---

### Task 3: Define `ProcessEvent` Model

**Files:**
- Create: `crates/agent-core/src/process/event.rs`

- [ ] **Step 1: Create event model**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessEvent {
    Init { session_id: String },
    UserMessage { content: String },
    AssistantMessage { content: String },
    TextDelta { text: String },
    Thinking { content: String },
    ToolUse { name: String, input: Value },
    ToolResult { name: String, result: String, failed: bool },
    Permission { tool_name: String, action: String },
    Question { questions: Vec<QuestionItem> },
    TodoWrite { todos: Vec<TodoItem> },
    Done,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub completed: bool,
}
```

- [ ] **Step 2: Add unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serde_text_delta() {
        let ev = ProcessEvent::TextDelta { text: "hello".into() };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("text_delta"));
        let decoded: ProcessEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, decoded);
    }

    #[test]
    fn test_serde_question() {
        let ev = ProcessEvent::Question {
            questions: vec![QuestionItem { id: "q1".into(), text: "ok?".into() }],
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains("question"));
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p agent-core process::event
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/agent-core/src/process/event.rs
git commit -m "feat(agent-core): add ProcessEvent model"
```


---

### Task 4: Implement EventMapper

**Files:**
- Create: `crates/agent-core/src/process/mapper.rs`

- [ ] **Step 1: Implement mapper**

```rust
use crate::event_sink::DynEventSink;
use crate::process::event::ProcessEvent;
use serde_json::json;

pub struct EventMapper {
    instance_id: String,
    conversation_id: String,
}

impl EventMapper {
    pub fn new(instance_id: String, conversation_id: String) -> Self {
        Self {
            instance_id,
            conversation_id,
        }
    }

    pub fn map(&self, event: ProcessEvent, sink: &DynEventSink) {
        match event {
            ProcessEvent::TextDelta { text } => {
                if !text.is_empty() {
                    sink.emit(
                        "agent.token",
                        json!({
                            "text": text,
                            "instance_id": self.instance_id,
                            "conversation_id": self.conversation_id,
                        }),
                    );
                }
            }
            ProcessEvent::Thinking { content } => {
                sink.emit(
                    "agent.thinking",
                    json!({
                        "content": content,
                        "id": format!("thinking-{}", self.instance_id),
                        "type": "step-start",
                        "instance_id": self.instance_id,
                        "conversation_id": self.conversation_id,
                    }),
                );
            }
            ProcessEvent::Permission { tool_name, action } => {
                sink.emit(
                    "agent.permission",
                    json!({
                        "sessionID": self.conversation_id,
                        "interaction": {
                            "type": "permission",
                            "toolName": tool_name,
                            "action": action,
                        },
                        "conversation_id": self.conversation_id,
                        "instance_id": self.instance_id,
                    }),
                );
            }
            ProcessEvent::Question { questions } => {
                sink.emit(
                    "agent.question",
                    json!({
                        "sessionID": self.conversation_id,
                        "interaction": {
                            "type": "question",
                            "questions": questions,
                        },
                        "conversation_id": self.conversation_id,
                        "instance_id": self.instance_id,
                    }),
                );
            }
            ProcessEvent::TodoWrite { todos } => {
                sink.emit(
                    "agent.todowrite",
                    json!({
                        "sessionID": self.conversation_id,
                        "interaction": {
                            "type": "todowrite",
                            "todos": todos,
                        },
                        "conversation_id": self.conversation_id,
                        "instance_id": self.instance_id,
                    }),
                );
            }
            ProcessEvent::Done => {
                sink.emit(
                    "agent.done",
                    json!({
                        "instance_id": self.instance_id,
                        "conversation_id": self.conversation_id,
                    }),
                );
            }
            ProcessEvent::Error { message } => {
                sink.emit(
                    "agent.error",
                    json!({
                        "message": message,
                        "instance_id": self.instance_id,
                        "conversation_id": self.conversation_id,
                    }),
                );
            }
            _ => {}
        }
    }
}
```

- [ ] **Step 2: Add test with mock EventSink**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_sink::{DynEventSink, EventSink};
    use crate::process::event::{ProcessEvent, QuestionItem};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct MockSink {
        events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl EventSink for MockSink {
        fn emit(&self, method: &str, payload: serde_json::Value) {
            self.events.lock().unwrap().push((method.to_string(), payload));
        }
    }

    #[test]
    fn test_map_text_delta() {
        let sink = MockSink::default();
        let mapper = EventMapper::new("i-1".into(), "c-1".into());
        mapper.map(ProcessEvent::TextDelta { text: "hi".into() }, &DynEventSink::new(sink.clone()));
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "agent.token");
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p agent-core process::mapper
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/agent-core/src/process/mapper.rs
git commit -m "feat(agent-core): add ProcessEvent to EventSink mapper"
```

---

### Task 5: Implement `StdioTransport`

**Files:**
- Create: `crates/agent-core/src/process/stdio.rs`

- [ ] **Step 1: Implement StdioTransport**

```rust
use crate::process::transport::{Transport, TransportError, TransportHandle};
use async_trait::async_trait;
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

pub struct StdioTransport {
    program: String,
    args: Vec<String>,
    cwd: String,
}

impl StdioTransport {
    pub fn new(program: impl Into<String>, args: Vec<String>, cwd: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args,
            cwd: cwd.into(),
        }
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args)
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| TransportError::ProcessStart(format!("{}: {}", self.program, e)))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| TransportError::ProcessStart("stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TransportError::ProcessStart("stdout unavailable".into()))?;

        Ok(Box::new(StdioHandle {
            child,
            writer: BufWriter::new(stdin),
            reader: BufReader::new(stdout).lines(),
        }))
    }

    fn endpoint(&self) -> Option<String> {
        None
    }
}

struct StdioHandle {
    child: Child,
    writer: BufWriter<ChildStdin>,
    reader: tokio::io::Lines<BufReader<ChildStdout>>,
}

#[async_trait]
impl TransportHandle for StdioHandle {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError> {
        let line = serde_json::to_string(&payload)
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        self.writer
            .write_all(b"\n")
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        self.writer
            .flush()
            .await
            .map_err(|e| TransportError::SendFailed(e.to_string()))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value, TransportError> {
        match self.reader.next_line().await {
            Ok(Some(line)) => serde_json::from_str(&line)
                .map_err(|e| TransportError::ReceiveFailed(format!("{e}: {line}"))),
            Ok(None) => Err(TransportError::Closed),
            Err(e) => Err(TransportError::ReceiveFailed(e.to_string())),
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let _ = self.child.start_kill();
        Ok(())
    }
}
```

- [ ] **Step 2: Add unit test with `cat`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stdio_echo() {
        let transport = StdioTransport::new("cat".to_string(), vec![], ".".to_string());
        let mut handle = transport.start().await.unwrap();
        handle.send(serde_json::json!({"hello":"world"})).await.unwrap();
        let value = handle.receive().await.unwrap();
        assert_eq!(value["hello"], "world");
        handle.close().await.unwrap();
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p agent-core process::stdio -- --nocapture
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/agent-core/src/process/stdio.rs
git commit -m "feat(agent-core): implement StdioTransport"
```

---

### Task 6: Implement `HttpTransport`

**Files:**
- Create: `crates/agent-core/src/process/http.rs`

- [ ] **Step 1: Implement HttpTransport**

```rust
use crate::process::transport::{Transport, TransportError, TransportHandle};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::Value;

pub struct HttpTransport {
    base_url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn start(&self) -> Result<Box<dyn TransportHandle>, TransportError> {
        Ok(Box::new(HttpHandle {
            base_url: self.base_url.clone(),
            client: self.client.clone(),
            receiver: None,
        }))
    }

    fn endpoint(&self) -> Option<String> {
        Some(self.base_url.clone())
    }
}

struct HttpHandle {
    base_url: String,
    client: reqwest::Client,
    receiver: Option<tokio::sync::mpsc::Receiver<Value>>,
}

#[async_trait]
impl TransportHandle for HttpHandle {
    async fn send(&mut self, payload: Value) -> Result<(), TransportError> {
        // HttpTransport send is not used for generic send; sessions are managed by consumers.
        // Keep the trait simple: this is a no-op placeholder.
        let _ = payload;
        Ok(())
    }

    async fn receive(&mut self) -> Result<Value, TransportError> {
        if let Some(ref mut rx) = self.receiver {
            rx.recv()
                .await
                .ok_or(TransportError::Closed)
        } else {
            Err(TransportError::ReceiveFailed("SSE not connected".into()))
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.receiver = None;
        Ok(())
    }
}

impl HttpHandle {
    pub fn connect_sse(&mut self, instance_id: String, sender: tokio::sync::mpsc::Sender<Value>) {
        let url = format!("{}/event", self.base_url);
        let client = self.client.clone();
        let (tx, rx) = tokio::sync::mpsc::channel::<Value>(1000);
        self.receiver = Some(rx);

        tokio::spawn(async move {
            loop {
                match client
                    .get(&url)
                    .header("Accept", "text/event-stream")
                    .send()
                    .await
                {
                    Ok(resp) => {
                        let mut stream = resp.bytes_stream();
                        while let Some(chunk) = stream.next().await {
                            if let Ok(bytes) = chunk {
                                let text = String::from_utf8_lossy(&bytes);
                                for line in text.lines() {
                                    if let Some(data) = line.strip_prefix("data: ") {
                                        if let Ok(value) = serde_json::from_str::<Value>(data) {
                                            let _ = tx.send(value).await;
                                            let _ = sender.send(value.clone()).await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("[HttpTransport] SSE connect error: {}, retrying...", e);
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
            }
        });
    }
}
```

- [ ] **Step 2: Build check**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check -p agent-core
```

Expected: success, 0 warnings

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/process/http.rs
git commit -m "feat(agent-core): implement HttpTransport skeleton"
```

---


### Task 7: Refactor `opencode-plugin` to use `HttpTransport`

**Files:**
- Modify: `crates/opencode-plugin/src/instance.rs`

- [ ] **Step 1: Replace direct HTTP/SSE with `HttpTransport`**

Rewrite `OpencodeInstance` to hold an `AgentProcess` (a small wrapper around `HttpTransport`) and use `ProcessEvent` mapping. Keep the public `AgentInstance` trait methods unchanged.

Key changes:
- Remove `reqwest::Client`, `base_url: Mutex<Option<String>>`, `event_sender`.
- Add `transport: Box<dyn HttpTransport>` or a wrapper `ProcessAgent`.
- Convert SSE payloads to `ProcessEvent` in a dedicated function.

Because this is a large refactor, split the file if it exceeds 600 effective lines. Create:
- `crates/opencode-plugin/src/transport.rs` — OpenCode-specific HTTP helpers
- `crates/opencode-plugin/src/mapper.rs` — OpenCode SSE payload -> `ProcessEvent`
- Keep `instance.rs` focused on `AgentInstance` implementation.

- [ ] **Step 2: Implement OpenCode -> ProcessEvent mapping**

```rust
fn map_opencode_sse(payload: &Value) -> Option<ProcessEvent> {
    let event_type = payload
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match event_type {
        "message.part.delta" => {
            let delta = payload
                .get("properties")
                .and_then(|p| p.get("delta"))
                .and_then(|d| d.as_str())?;
            Some(ProcessEvent::TextDelta { text: delta.into() })
        }
        "thinking" => {
            let content = payload
                .get("content")
                .or_else(|| payload.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Some(ProcessEvent::Thinking { content: content.into() })
        }
        "message.part.updated" => {
            let part = payload.get("properties")?.get("part")?;
            if part.get("type").and_then(|v| v.as_str()) == Some("step-start") {
                let text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                Some(ProcessEvent::Thinking { content: text.into() })
            } else {
                None
            }
        }
        "session.idle" => Some(ProcessEvent::Done),
        "session.error" => Some(ProcessEvent::Error {
            message: payload.to_string(),
        }),
        _ => None,
    }
}
```

- [ ] **Step 3: Update `OpencodeInstance::send_message`**

Use the existing HTTP endpoints but forward resulting SSE events through `ProcessEvent` and `EventMapper`.

```rust
// Inside send_message async block
self.ensure_started().await?;
let session_id = match self.find_session_for_conversation(&conversation_id) { ... };
let result = self.send_message_http(&session_id, &message).await?;

// Detect interactions from response parts (reuse existing detect_interaction_from_parts)
if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
    if let Some(interaction) = detect_interaction_from_parts(&parts.clone()) {
        // Map to ProcessEvent
        let ev = map_interaction(interaction);
        self.mapper.map(ev, &self.event_sink);
    }
}
```

- [ ] **Step 4: Build and test**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check -p opencode-plugin && cargo test -p opencode-plugin
```

Expected: success, tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/opencode-plugin/
git commit -m "refactor(opencode-plugin): use agent-core::process abstractions"
```

---

### Task 8: Create `claude-plugin` Crate

**Files:**
- Create: `crates/claude-plugin/Cargo.toml`
- Create: `crates/claude-plugin/src/lib.rs`
- Modify: `Cargo.toml` (root workspace members)

- [ ] **Step 1: Create `crates/claude-plugin/Cargo.toml`**

```toml
[package]
name = "claude-plugin"
version = "0.1.0"
edition = "2021"

[dependencies]
agent-core = { path = "../agent-core" }
tokio = { version = "1", features = ["rt", "sync", "process", "io-util", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
log = "0.4"
```

- [ ] **Step 2: Create `crates/claude-plugin/src/lib.rs`**

```rust
pub mod instance;
pub mod parser;
pub mod plugin;

pub use plugin::ClaudePlugin;
```

- [ ] **Step 3: Add to root workspace**

Modify `Cargo.toml`:

```toml
members = [
    "crates/dh-core",
    "crates/dh-platform",
    "crates/dh-db",
    "crates/agent-core",
    "crates/opencode-plugin",
    "crates/claude-plugin",  # add
    "apps/gatewayd",
    "apps/cli",
]
```

- [ ] **Step 4: Build check**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check -p claude-plugin
```

Expected: success

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/claude-plugin/
git commit -m "chore: scaffold claude-plugin crate"
```

---

### Task 9: Implement Claude Stream-JSON Parser

**Files:**
- Create: `crates/claude-plugin/src/parser.rs`

- [ ] **Step 1: Define raw event types**

```rust
use agent_core::process::event::{ProcessEvent, QuestionItem, TodoItem};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeRawEvent {
    System { subtype: String, #[serde(flatten)] extra: Value },
    User { content: Vec<ClaudeContent> },
    Assistant { content: Vec<ClaudeContent> },
    StreamEvent { event: ClaudeStreamEvent },
    ToolUse { name: String, input: Value },
    ToolResult { name: String, result: String, failed: Option<bool> },
    Result { result: String, session_id: String },
    Error { message: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeContent {
    Text { text: String },
    Thinking { thinking: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClaudeStreamEvent {
    TextDelta { delta: ClaudeTextDelta },
    ThinkingDelta { delta: ClaudeThinkingDelta },
    MessageStop {},
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeTextDelta {
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClaudeThinkingDelta {
    pub thinking: String,
}
```

- [ ] **Step 2: Implement parser**

```rust
pub fn parse_claude_line(line: &str) -> Option<ClaudeRawEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

pub fn to_process_event(raw: &ClaudeRawEvent) -> Option<ProcessEvent> {
    match raw {
        ClaudeRawEvent::StreamEvent { event } => match event {
            ClaudeStreamEvent::TextDelta { delta } => {
                Some(ProcessEvent::TextDelta { text: delta.text.clone() })
            }
            ClaudeStreamEvent::ThinkingDelta { delta } => Some(ProcessEvent::Thinking {
                content: delta.thinking.clone(),
            }),
            ClaudeStreamEvent::MessageStop {} => Some(ProcessEvent::Done),
        },
        ClaudeRawEvent::Assistant { content } => {
            let text: String = content
                .iter()
                .filter_map(|c| match c {
                    ClaudeContent::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() {
                None
            } else {
                Some(ProcessEvent::AssistantMessage { content: text })
            }
        }
        ClaudeRawEvent::ToolUse { name, input } => Some(ProcessEvent::ToolUse {
            name: name.clone(),
            input: input.clone(),
        }),
        ClaudeRawEvent::ToolResult { name, result, failed } => Some(ProcessEvent::ToolResult {
            name: name.clone(),
            result: result.clone(),
            failed: failed.unwrap_or(false),
        }),
        ClaudeRawEvent::Result { .. } => Some(ProcessEvent::Done),
        ClaudeRawEvent::Error { message } => Some(ProcessEvent::Error {
            message: message.clone(),
        }),
        _ => None,
    }
}
```

- [ ] **Step 3: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_delta() {
        let line = r#"{"type":"stream_event","event":{"type":"text_delta","delta":{"text":"hello"}}}"#;
        let raw = parse_claude_line(line).unwrap();
        let ev = to_process_event(&raw).unwrap();
        assert!(matches!(ev, ProcessEvent::TextDelta { text } if text == "hello"));
    }

    #[test]
    fn test_parse_done() {
        let line = r#"{"type":"result","result":"ok","session_id":"s-1"}"#;
        let raw = parse_claude_line(line).unwrap();
        let ev = to_process_event(&raw).unwrap();
        assert!(matches!(ev, ProcessEvent::Done));
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p claude-plugin parser
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/claude-plugin/src/parser.rs
git commit -m "feat(claude-plugin): add stream-json parser"
```

---


### Task 10: Implement `ClaudePlugin` and `ClaudeInstance`

**Files:**
- Create: `crates/claude-plugin/src/plugin.rs`
- Create: `crates/claude-plugin/src/instance.rs`

- [ ] **Step 1: Implement `ClaudePlugin`**

```rust
use agent_core::error::PluginError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::plugin::AgentPlugin;
use std::sync::Arc;

pub struct ClaudePlugin {
    logger: Arc<SessionLogger>,
}

impl ClaudePlugin {
    pub fn new(logger: Arc<SessionLogger>) -> Self {
        Self { logger }
    }
}

impl AgentPlugin for ClaudePlugin {
    fn key(&self) -> &'static str {
        "claude-code"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn is_installed(&self) -> bool {
        std::process::Command::new("claude")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn create_instance(
        &self,
        config: InstanceConfig,
        event_sink: DynEventSink,
    ) -> Result<Box<dyn AgentInstance>, PluginError> {
        if !self.is_installed() {
            return Err(PluginError::NotInstalled("claude".to_string()));
        }
        Ok(Box::new(crate::instance::ClaudeInstance::new(
            config,
            event_sink,
            self.logger.clone(),
        )))
    }
}
```

- [ ] **Step 2: Implement `ClaudeInstance`**

Create `crates/claude-plugin/src/instance.rs`:

```rust
use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::event::ProcessEvent;
use agent_core::process::mapper::EventMapper;
use agent_core::process::stdio::StdioTransport;
use agent_core::process::transport::{Transport, TransportHandle};
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

pub struct ClaudeInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    transport: Arc<Mutex<Option<Box<dyn TransportHandle>>>>,
    status: Arc<Mutex<InstanceStatus>>,
    sessions: Arc<Mutex<HashMap<String, String>>>,
}

impl ClaudeInstance {
    pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            event_sink,
            logger,
            transport: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn emit_status(&self, status: InstanceStatus) {
        self.event_sink.emit(
            "agent:status_changed",
            json!({
                "instance_id": self.config.id,
                "status": status,
            }),
        );
    }

    fn build_transport(&self) -> StdioTransport {
        let mut args = vec![
            "-p".to_string(),
            "--input-format=stream-json".to_string(),
            "--output-format=stream-json".to_string(),
            "--verbose".to_string(),
        ];

        let permission_mode = self
            .config
            .permission_mode
            .as_deref()
            .unwrap_or("bypassPermissions");
        args.push(format!("--permission-mode={}", permission_mode));

        if let Some(model) = &self.config.model {
            args.push(format!("--model={}", model));
        }

        args.push(format!("--worktree={}", self.config.workspace));

        if let Some(session_id) = &self.config.session_id {
            args.push(format!("--resume={}", session_id));
        }

        StdioTransport::new("claude".to_string(), args, self.config.workspace.clone())
    }

    async fn ensure_started(&self) -> Result<(), InstanceError> {
        {
            let guard = self.transport.lock().unwrap();
            if guard.is_some() {
                return Ok(());
            }
        }

        let transport = self.build_transport();
        let mut handle = transport
            .start()
            .await
            .map_err(|e| InstanceError::ProcessError(e.to_string()))?;

        // Drain init event to get session_id if present.
        match tokio::time::timeout(std::time::Duration::from_secs(5), handle.receive()).await {
            Ok(Ok(value)) => {
                if let Some(session_id) = value.get("session_id").and_then(|v| v.as_str()) {
                    let mut sessions = self.sessions.lock().unwrap();
                    sessions.insert(self.config.id.clone(), session_id.to_string());
                }
            }
            _ => {}
        }

        {
            let mut guard = self.transport.lock().unwrap();
            *guard = Some(handle);
        }
        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Running { pid: 0 };
        }
        self.emit_status(InstanceStatus::Running { pid: 0 });

        self.logger.log(
            &self.config.id,
            LogLevel::Info,
            "claude-plugin",
            &format!("claude process started for {}", self.config.id),
            None,
            Some(self.config.id.clone()),
        );

        // Spawn background reader.
        let transport_arc = Arc::clone(&self.transport);
        let event_sink = self.event_sink.clone();
        let instance_id = self.config.id.clone();
        let conversation_id = self.config.id.clone();
        tokio::spawn(async move {
            loop {
                let next = {
                    let mut guard = transport_arc.lock().unwrap();
                    if let Some(ref mut handle) = *guard {
                        handle.receive().await.ok()
                    } else {
                        break;
                    }
                };

                if let Some(value) = next {
                    if let Some(raw) = crate::parser::parse_claude_value(&value) {
                        if let Some(ev) = crate::parser::to_process_event(&raw) {
                            let mapper = EventMapper::new(instance_id.clone(), conversation_id.clone());
                            mapper.map(ev, &event_sink);
                        }
                    }
                } else {
                    break;
                }
            }
        });

        Ok(())
    }

    async fn send_user_message(&self, text: &str) -> Result<(), InstanceError> {
        self.ensure_started().await?;
        let payload = json!({
            "type": "message",
            "role": "user",
            "content": [{"type": "text", "text": text}]
        });
        let mut guard = self.transport.lock().unwrap();
        if let Some(ref mut handle) = *guard {
            handle
                .send(payload)
                .await
                .map_err(|e| InstanceError::SendFailed(e.to_string()))?;
            Ok(())
        } else {
            Err(InstanceError::NotRunning("transport not available".into()))
        }
    }
}

impl AgentInstance for ClaudeInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn plugin_key(&self) -> &'static str {
        "claude-code"
    }

    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let conversation_id = conversation_id.to_string();
        let message = message.to_string();
        Box::pin(async move { self.send_user_message(&message).await })
    }

    fn respond(
        &self,
        _session_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let message = message.to_string();
        Box::pin(async move { self.send_user_message(&message).await })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            let mut guard = self.transport.lock().unwrap();
            if let Some(mut handle) = guard.take() {
                let _ = handle.close().await;
            }
            {
                let mut guard = self.status.lock().unwrap();
                *guard = InstanceStatus::Stopped;
            }
            self.emit_status(InstanceStatus::Stopped);
            Ok(())
        })
    }
}
```

- [ ] **Step 3: Update parser to accept `Value`**

Add to `crates/claude-plugin/src/parser.rs`:

```rust
pub fn parse_claude_value(value: &serde_json::Value) -> Option<ClaudeRawEvent> {
    serde_json::from_value(value.clone()).ok()
}
```

- [ ] **Step 4: Build check**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check -p claude-plugin
```

Expected: success, 0 warnings

- [ ] **Step 5: Commit**

```bash
git add crates/claude-plugin/src/plugin.rs crates/claude-plugin/src/instance.rs crates/claude-plugin/src/parser.rs
git commit -m "feat(claude-plugin): implement ClaudePlugin and ClaudeInstance"
```

---

### Task 11: Register Claude Plugin in Desktop App

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add dependency**

Modify `src-tauri/Cargo.toml`:

```toml
[dependencies]
...
agent-core = { path = "../crates/agent-core" }
opencode-plugin = { path = "../crates/opencode-plugin" }
claude-plugin = { path = "../crates/claude-plugin" }  # add
```

- [ ] **Step 2: Register plugin in main.rs**

Modify `src-tauri/src/main.rs` around AgentService initialization:

```rust
let mut agent_service = Arc::new(dh_desktop::service::agent_service::AgentService::new(
    logger.clone(),
    ws_event_sink.clone(),
));
Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
    logger.clone(),
)));
Arc::get_mut(&mut agent_service).unwrap().register_plugin(Box::new(claude_plugin::plugin::ClaudePlugin::new(
    logger.clone(),
)));
```

- [ ] **Step 3: Build check**

```bash
cd /home/nan/deepharness-ent-desktop/src-tauri && cargo check
```

Expected: success, 0 warnings

- [ ] **Step 4: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/main.rs
git commit -m "feat(desktop): register claude-plugin in AgentService"
```

---

### Task 12: Verify Full Build and End-to-End

**Files:**
- All of the above

- [ ] **Step 1: Run full cargo check**

```bash
cd /home/nan/deepharness-ent-desktop && cargo check && cd src-tauri && cargo check
```

Expected: both succeed with 0 warnings

- [ ] **Step 2: Run tests**

```bash
cd /home/nan/deepharness-ent-desktop && cargo test -p agent-core && cargo test -p opencode-plugin && cargo test -p claude-plugin
```

Expected: all pass

- [ ] **Step 3: Run lint**

```bash
cd /home/nan/deepharness-ent-desktop && pnpm lint
```

Expected: pass

- [ ] **Step 4: Build desktop app**

```bash
cd /home/nan/deepharness-ent-desktop && pnpm tauri build
```

Expected: success, producing deb + rpm bundles

- [ ] **Step 5: Launch app**

```bash
cd /home/nan/deepharness-ent-desktop && bash run-desktop.sh
```

Expected: process starts and stays running.

- [ ] **Step 6: Manual test Claude agent**

In the running desktop app:
1. Login.
2. Select **Claude Code** agent.
3. Send a simple message like "hello".
4. Verify stream tokens appear and `agent.done` arrives.

- [ ] **Step 7: Commit final verification notes (optional)**

```bash
git commit --allow-empty -m "test(claude-plugin): verify build and e2e smoke"
```

---

## Self-Review Checklist

- [ ] Spec coverage: every section of `2026-06-15-claude-plugin-design.md` maps to at least one task.
- [ ] No placeholders: no "TBD", "implement later", or vague steps.
- [ ] Type consistency: `InstanceConfig`, `ProcessEvent`, `TransportHandle`, `EventMapper` signatures match across tasks.
- [ ] File size: `instance.rs` files split if they approach 600 effective lines.
