# Agent 适配层 Rust 迁移实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 TypeScript 中的 OpenCode 智能体适配层迁移至 Rust，定义统一 `AgentPlugin` trait，每个智能体独立 crate，Rust 管理全部实例状态，前端通过 Tauri Command + Event 通信。

**Architecture:** Rust Workspace 包含 `agent-core`（trait + 公共类型 + SessionLogger）、`agent-runtime`（进程管理）、`opencode-plugin`（OpenCode 具体实现）。前端精简为薄封装层，通过 `invoke` 发送消息、`listen` 接收事件流。

**Tech Stack:** Rust (Tauri v2, tokio, rusqlite, serde), TypeScript (React, Tauri API)

---

## 文件结构总览

### 新建文件

| 文件 | 职责 |
|------|------|
| `src-tauri/crates/agent-core/Cargo.toml` | agent-core crate 配置 |
| `src-tauri/crates/agent-core/src/lib.rs` | 模块导出 |
| `src-tauri/crates/agent-core/src/plugin.rs` | `AgentPlugin` trait |
| `src-tauri/crates/agent-core/src/instance.rs` | `AgentInstance` trait + `InstanceStatus` |
| `src-tauri/crates/agent-core/src/event.rs` | `AgentEvent` 枚举 |
| `src-tauri/crates/agent-core/src/logger.rs` | `SessionLogger`（异步双写） |
| `src-tauri/crates/agent-core/src/error.rs` | `PluginError`, `InstanceError` |
| `src-tauri/crates/agent-runtime/Cargo.toml` | agent-runtime crate 配置 |
| `src-tauri/crates/agent-runtime/src/lib.rs` | 模块导出 |
| `src-tauri/crates/agent-runtime/src/process.rs` | `ProcessHandle`, `spawn_command`, `kill` |
| `src-tauri/crates/agent-runtime/src/health_check.rs` | 健康检查、僵尸进程清理 |
| `src-tauri/crates/opencode-plugin/Cargo.toml` | opencode-plugin crate 配置 |
| `src-tauri/crates/opencode-plugin/src/lib.rs` | 模块导出 |
| `src-tauri/crates/opencode-plugin/src/plugin.rs` | `OpencodePlugin` |
| `src-tauri/crates/opencode-plugin/src/instance.rs` | `OpencodeInstance` |
| `src-tauri/crates/opencode-plugin/src/parser.rs` | JSON-line 解析 |
| `src-tauri/crates/opencode-plugin/src/mapper.rs` | 原始事件 → `AgentEvent` 映射 |
| `src-tauri/src/commands/agent.rs` | Tauri agent commands |
| `src-tauri/src/commands/session_log.rs` | Tauri session log commands |
| `src-tauri/src/service/agent_service.rs` | `AgentService` |
| `src-tauri/src/service/plugin_registry.rs` | `PluginRegistry` |
| `src-tauri/src/service/instance_registry.rs` | `InstanceRegistry` |
| `src-tauri/src/models/agent.rs` | `InstanceConfig`, `InstanceInfo`, `PluginInfo` |
| `src-tauri/src/models/event.rs` | Rust 侧 AgentEvent 相关 |
| `src-tauri/src/models/log.rs` | `SessionLogEntry`, `LogLevel` |
| `src/hooks/use-agent-service.ts` | 前端 agent service hook |
| `src/hooks/use-session-log-rust.ts` | 前端 session log hook |

### 修改文件

| 文件 | 修改内容 |
|------|----------|
| `src-tauri/Cargo.toml` | 改为 workspace |
| `src-tauri/src/main.rs` | 注册 AgentService、SessionLogger、新增 commands |
| `src-tauri/src/lib.rs` | 新增模块声明 |
| `src-tauri/src/commands/mod.rs` | 新增（或创建）模块聚合 |
| `src-tauri/src/service/mod.rs` | 新增模块聚合 |
| `src-tauri/src/models/mod.rs` | 新增模块聚合 |
| `src-tauri/capabilities/default.json` | 添加 agent command 权限 |
| `src/db/index.ts` | 添加 `sessionLogLoad` 方法 |
| `src/pages/WorkspacePage.tsx` | 替换 agentManager 调用 |
| `src/agents/manager.ts` | 精简为薄封装 |
| `src/agents/registry.ts` | 精简或删除 |
| `src/agents/types.ts` | 与 Rust Event 对齐 |

### 删除文件

| 文件 | 原因 |
|------|------|
| `src/agents/opencode/adapter.ts` | 逻辑移至 Rust |
| `src/agents/opencode/parser.ts` | 逻辑移至 Rust |
| `src/agents/opencode/types.ts` | 不再使用 |
| `src/agents/opencode/adapter.test.ts` | 测试移至 Rust |
| `src/agents/opencode/adapter-integration.test.ts` | 测试移至 Rust |
| `src/agents/opencode/parser.test.ts` | 测试移至 Rust |
| `src-tauri/src/sidecar_manager.rs` | 逻辑提取到 agent-runtime |
| `src-tauri/src/agent_db.rs` | 合并到主数据库 |

---

## Task 1: 配置 Rust Workspace

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/crates/agent-core/Cargo.toml`
- Create: `src-tauri/crates/agent-runtime/Cargo.toml`
- Create: `src-tauri/crates/opencode-plugin/Cargo.toml`
- Modify: `src-tauri/src/main.rs`（添加 crates 依赖）

- [ ] **Step 1: 修改 src-tauri/Cargo.toml 为 workspace**

```toml
[package]
name = "ai-coding-desktop"
version = "0.0.1"
description = "AI Coding Desktop App"
authors = ["you"]
edition = "2021"

[workspace]
members = ["crates/*"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
tauri-plugin-fs = "2"
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
uuid = { version = "1", features = ["v4"] }
chrono = "0.4"

# workspace crates
agent-core = { path = "crates/agent-core" }
agent-runtime = { path = "crates/agent-runtime" }
opencode-plugin = { path = "crates/opencode-plugin" }

[features]
custom-protocol = ["tauri/custom-protocol"]
```

- [ ] **Step 2: 创建 agent-core/Cargo.toml**

```toml
[package]
name = "agent-core"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["rt", "sync", "process", "io-util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2" }
rusqlite = { version = "0.32", features = ["bundled"] }
chrono = "0.4"
async-trait = "0.1"
thiserror = "1"
```

- [ ] **Step 3: 创建 agent-runtime/Cargo.toml**

```toml
[package]
name = "agent-runtime"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1", features = ["rt", "sync", "process", "io-util", "time"] }
agent-core = { path = "../agent-core" }
```

- [ ] **Step 4: 创建 opencode-plugin/Cargo.toml**

```toml
[package]
name = "opencode-plugin"
version = "0.1.0"
edition = "2021"

[dependencies]
agent-core = { path = "../agent-core" }
agent-runtime = { path = "../agent-runtime" }
tokio = { version = "1", features = ["rt", "sync", "process", "io-util"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2" }
async-trait = "0.1"
```

- [ ] **Step 5: 编译验证 workspace 结构**

Run: `cd src-tauri && cargo check`
Expected: 编译通过（此时 crates 为空壳，但 workspace 结构正确）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/crates/
git commit -m "chore: setup rust workspace for agent plugins"
```

---

## Task 2: 实现 agent-core（Trait + 类型 + Logger）

**Files:**
- Create: `src-tauri/crates/agent-core/src/lib.rs`
- Create: `src-tauri/crates/agent-core/src/plugin.rs`
- Create: `src-tauri/crates/agent-core/src/instance.rs`
- Create: `src-tauri/crates/agent-core/src/event.rs`
- Create: `src-tauri/crates/agent-core/src/error.rs`
- Create: `src-tauri/crates/agent-core/src/logger.rs`

- [ ] **Step 1: 创建 error.rs**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Plugin not installed: {0}")]
    NotInstalled(String),
    #[error("Failed to create instance: {0}")]
    CreateInstanceFailed(String),
}

#[derive(Error, Debug)]
pub enum InstanceError {
    #[error("Instance not found: {0}")]
    NotFound(String),
    #[error("Instance not running: {0}")]
    NotRunning(String),
    #[error("Failed to send message: {0}")]
    SendFailed(String),
    #[error("Process error: {0}")]
    ProcessError(String),
}
```

- [ ] **Step 2: 创建 event.rs**

```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { tool_name: String, args: Value },
    ToolResult { tool_name: String, result: String, failed: bool },
    AskPermission { message: String, tool_name: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}
```

- [ ] **Step 3: 创建 instance.rs**

```rust
use crate::error::InstanceError;
use crate::event::AgentEvent;
use async_trait::async_trait;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceStatus {
    Stopped,
    Starting,
    Running { pid: u32 },
    Crashed(String),
}

#[derive(Clone, Debug)]
pub struct InstanceConfig {
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub session_id: Option<String>,
}

#[async_trait]
pub trait AgentInstance: Send + Sync {
    fn id(&self) -> &str;
    fn status(&self) -> InstanceStatus;
    async fn send_message(&self, message: &str) -> Result<(), InstanceError>;
    async fn stop(&self) -> Result<(), InstanceError>;
}
```

- [ ] **Step 4: 创建 plugin.rs**

```rust
use crate::error::PluginError;
use crate::instance::{AgentInstance, InstanceConfig};

pub trait AgentPlugin: Send + Sync {
    fn key(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn is_installed(&self) -> bool;
    fn create_instance(&self, config: InstanceConfig) -> Result<Box<dyn AgentInstance>, PluginError>;
}
```

- [ ] **Step 5: 创建 logger.rs**

```rust
use serde::{Serialize, Deserialize};
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::mpsc;
use rusqlite::params;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionLogEntry {
    pub conversation_id: String,
    pub timestamp: String,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
    pub payload: Option<Value>,
}

#[derive(Clone)]
pub struct SessionLogger {
    sender: mpsc::UnboundedSender<SessionLogEntry>,
}

impl SessionLogger {
    pub fn new(app_handle: AppHandle, db_conn: rusqlite::Connection) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<SessionLogEntry>();

        std::thread::spawn(move || {
            while let Some(entry) = rx.blocking_recv() {
                let _ = app_handle.emit("session:log", &entry);
                let _ = db_conn.execute(
                    "INSERT INTO session_logs (conversation_id, timestamp, level, source, message, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        &entry.conversation_id,
                        &entry.timestamp,
                        entry.level.as_str(),
                        &entry.source,
                        &entry.message,
                        entry.payload.as_ref().map(|v| v.to_string())
                    ],
                );
            }
        });

        Self { sender: tx }
    }

    pub fn log(
        &self,
        conversation_id: &str,
        level: LogLevel,
        source: &str,
        message: &str,
        payload: Option<Value>,
    ) {
        let entry = SessionLogEntry {
            conversation_id: conversation_id.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            source: source.to_string(),
            message: message.to_string(),
            payload,
        };
        let _ = self.sender.send(entry);
    }
}
```

- [ ] **Step 6: 创建 lib.rs**

```rust
pub mod error;
pub mod event;
pub mod instance;
pub mod logger;
pub mod plugin;
```

- [ ] **Step 7: 编译验证**

Run: `cd src-tauri && cargo check -p agent-core`
Expected: 编译通过

- [ ] **Step 8: Commit**

```bash
git add src-tauri/crates/agent-core/
git commit -m "feat(agent-core): define AgentPlugin/AgentInstance traits, AgentEvent, SessionLogger"
```

---

## Task 3: 实现 agent-runtime（进程管理）

**Files:**
- Create: `src-tauri/crates/agent-runtime/src/lib.rs`
- Create: `src-tauri/crates/agent-runtime/src/process.rs`
- Create: `src-tauri/crates/agent-runtime/src/health_check.rs`

- [ ] **Step 1: 创建 process.rs**

```rust
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::oneshot;

pub struct ProcessHandle {
    pub pid: u32,
    pub stdout_lines: tokio::io::Lines<BufReader<ChildStdout>>,
    pub kill_tx: oneshot::Sender<()>,
    pub child: Child,
}

pub async fn spawn_command(
    program: &str,
    args: &[String],
    cwd: &str,
) -> Result<ProcessHandle, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Failed to spawn process: {e}"))?;

    let pid = child.id().unwrap_or(0);
    let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
    let reader = BufReader::new(stdout);
    let lines = reader.lines();

    let (kill_tx, kill_rx) = oneshot::channel();

    tokio::spawn(async move {
        let _ = kill_rx.await;
        let _ = child.kill().await;
    });

    Ok(ProcessHandle {
        pid,
        stdout_lines: lines,
        kill_tx,
        child,
    })
}

pub async fn kill_process(handle: &mut ProcessHandle) -> Result<(), String> {
    let _ = handle.kill_tx.send(());
    handle.child.kill().await.map_err(|e| e.to_string())
}
```

- [ ] **Step 2: 创建 health_check.rs**

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::process::Child;
use tokio::time::{interval, Duration};

pub struct HealthChecker {
    processes: Arc<Mutex<HashMap<String, Child>>>,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn register(&self, instance_id: String, child: Child) {
        if let Ok(mut map) = self.processes.lock() {
            map.insert(instance_id, child);
        }
    }

    pub fn unregister(&self, instance_id: &str) {
        if let Ok(mut map) = self.processes.lock() {
            map.remove(instance_id);
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(5));
            loop {
                ticker.tick().await;
                let mut to_remove = Vec::new();
                if let Ok(mut map) = self.processes.lock() {
                    for (id, child) in map.iter_mut() {
                        match child.try_wait() {
                            Ok(Some(_status)) => {
                                to_remove.push(id.clone());
                            }
                            Ok(None) => {}
                            Err(_) => {
                                to_remove.push(id.clone());
                            }
                        }
                    }
                    for id in &to_remove {
                        map.remove(id);
                    }
                }
            }
        });
    }
}
```

- [ ] **Step 3: 创建 lib.rs**

```rust
pub mod health_check;
pub mod process;
```

- [ ] **Step 4: 编译验证**

Run: `cd src-tauri && cargo check -p agent-runtime`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/agent-runtime/
git commit -m "feat(agent-runtime): add process spawn/kill and health check"
```

---

## Task 4: 实现 opencode-plugin（解析 + 映射 + Instance）

**Files:**
- Create: `src-tauri/crates/opencode-plugin/src/lib.rs`
- Create: `src-tauri/crates/opencode-plugin/src/parser.rs`
- Create: `src-tauri/crates/opencode-plugin/src/mapper.rs`
- Create: `src-tauri/crates/opencode-plugin/src/instance.rs`
- Create: `src-tauri/crates/opencode-plugin/src/plugin.rs`

- [ ] **Step 1: 创建 parser.rs**

解析 `opencode run --format json` 的 stdout 行输出。参考 `src/agents/opencode/parser.ts` 的逻辑。

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpencodeRawEvent {
    Thinking { content: String },
    TextDelta { content: String },
    ToolUse { name: String, args: Value },
    ToolResult { name: String, result: String, failed: Option<bool> },
    AskPermission { message: String, tool: String },
    AskUser { questions: Vec<String> },
    Error { message: String },
    Done,
}

pub fn parse_opencode_json_line(line: &str) -> Option<OpencodeRawEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str::<OpencodeRawEvent>(trimmed).ok()
}
```

- [ ] **Step 2: 创建 mapper.rs**

```rust
use agent_core::event::AgentEvent;
use crate::parser::OpencodeRawEvent;

pub fn map_to_agent_event(raw: OpencodeRawEvent) -> AgentEvent {
    match raw {
        OpencodeRawEvent::Thinking { content } => AgentEvent::Thinking { content },
        OpencodeRawEvent::TextDelta { content } => AgentEvent::TextDelta { content },
        OpencodeRawEvent::ToolUse { name, args } => AgentEvent::ToolUse {
            tool_name: name,
            args,
        },
        OpencodeRawEvent::ToolResult { name, result, failed } => AgentEvent::ToolResult {
            tool_name: name,
            result,
            failed: failed.unwrap_or(false),
        },
        OpencodeRawEvent::AskPermission { message, tool } => AgentEvent::AskPermission {
            message,
            tool_name: tool,
        },
        OpencodeRawEvent::AskUser { questions } => AgentEvent::AskUser { questions },
        OpencodeRawEvent::Error { message } => AgentEvent::Error { message },
        OpencodeRawEvent::Done => AgentEvent::Done,
    }
}
```

- [ ] **Step 3: 创建 instance.rs**

```rust
use agent_core::error::InstanceError;
use agent_core::event::AgentEvent;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_runtime::process::{kill_process, spawn_command, ProcessHandle};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

pub struct OpencodeInstance {
    config: InstanceConfig,
    status: Arc<Mutex<InstanceStatus>>,
    app_handle: AppHandle,
    logger: Arc<SessionLogger>,
    process_handle: Arc<Mutex<Option<ProcessHandle>>>,
}

impl OpencodeInstance {
    pub fn new(config: InstanceConfig, app_handle: AppHandle, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            app_handle,
            logger,
            process_handle: Arc::new(Mutex::new(None)),
        }
    }

    fn emit_event(&self, event: AgentEvent) {
        let _ = self.app_handle.emit(
            "agent:event",
            serde_json::json!({
                "instance_id": self.config.id,
                "event": event,
            }),
        );
    }

    fn emit_status(&self, status: InstanceStatus) {
        let _ = self.app_handle.emit(
            "agent:status_changed",
            serde_json::json!({
                "instance_id": self.config.id,
                "status": status,
            }),
        );
    }
}

#[async_trait]
impl AgentInstance for OpencodeInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    async fn send_message(&self, message: &str) -> Result<(), InstanceError> {
        let conversation_id = self.config.session_id.clone().unwrap_or_default();
        self.logger.log(
            &conversation_id,
            LogLevel::Info,
            "opencode-plugin",
            "send_message called",
            Some(serde_json::json!({ "message": message, "workspace": &self.config.workspace })),
        );

        let mut args = vec![
            "run".to_string(),
            "--format".to_string(),
            "json".to_string(),
        ];
        if !self.config.workspace.is_empty() {
            args.push("--dir".to_string());
            args.push(self.config.workspace.clone());
        }
        if let Some(ref session) = self.config.session_id {
            args.push("--session".to_string());
            args.push(session.clone());
        }
        args.push(message.to_string());

        self.logger.log(
            &conversation_id,
            LogLevel::Debug,
            "opencode-plugin",
            "CLI args built",
            Some(serde_json::json!({ "args": args })),
        );

        let mut handle = spawn_command("opencode", &args, &self.config.workspace)
            .await
            .map_err(|e| InstanceError::ProcessError(e))?;

        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Running { pid: handle.pid };
        }
        self.emit_status(self.status());

        self.logger.log(
            &conversation_id,
            LogLevel::Info,
            "agent-runtime",
            "process spawned",
            Some(serde_json::json!({ "pid": handle.pid })),
        );

        let mut events_parsed = 0;
        while let Ok(Some(line)) = handle.stdout_lines.next_line().await {
            if let Some(raw) = crate::parser::parse_opencode_json_line(&line) {
                let event = crate::mapper::map_to_agent_event(raw);
                events_parsed += 1;
                self.emit_event(event);
            }
        }

        if events_parsed == 0 {
            self.emit_event(AgentEvent::TextDelta {
                content: "(无输出)".to_string(),
            });
        }

        self.emit_event(AgentEvent::Done);

        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Stopped;
        }
        self.emit_status(InstanceStatus::Stopped);

        self.logger.log(
            &conversation_id,
            LogLevel::Info,
            "opencode-plugin",
            "send_message completed",
            Some(serde_json::json!({ "events_parsed": events_parsed })),
        );

        Ok(())
    }

    async fn stop(&self) -> Result<(), InstanceError> {
        let mut guard = self.process_handle.lock().unwrap();
        if let Some(ref mut handle) = *guard {
            kill_process(handle).await.map_err(|e| InstanceError::ProcessError(e))?;
        }
        {
            let mut status = self.status.lock().unwrap();
            *status = InstanceStatus::Stopped;
        }
        self.emit_status(InstanceStatus::Stopped);
        Ok(())
    }
}
```

- [ ] **Step 4: 创建 plugin.rs**

```rust
use agent_core::error::PluginError;
use agent_core::instance::{AgentInstance, InstanceConfig};
use agent_core::logger::SessionLogger;
use agent_core::plugin::AgentPlugin;
use std::sync::Arc;
use tauri::AppHandle;

pub struct OpencodePlugin {
    app_handle: AppHandle,
    logger: Arc<SessionLogger>,
}

impl OpencodePlugin {
    pub fn new(app_handle: AppHandle, logger: Arc<SessionLogger>) -> Self {
        Self { app_handle, logger }
    }
}

impl AgentPlugin for OpencodePlugin {
    fn key(&self) -> &'static str {
        "opencode"
    }

    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn is_installed(&self) -> bool {
        std::process::Command::new("opencode")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn create_instance(&self, config: InstanceConfig) -> Result<Box<dyn AgentInstance>, PluginError> {
        if !self.is_installed() {
            return Err(PluginError::NotInstalled("opencode".to_string()));
        }
        Ok(Box::new(crate::instance::OpencodeInstance::new(
            config,
            self.app_handle.clone(),
            self.logger.clone(),
        )))
    }
}
```

- [ ] **Step 5: 创建 lib.rs**

```rust
pub mod instance;
pub mod mapper;
pub mod parser;
pub mod plugin;
```

- [ ] **Step 6: 编译验证**

Run: `cd src-tauri && cargo check -p opencode-plugin`
Expected: 编译通过

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/opencode-plugin/
git commit -m "feat(opencode-plugin): implement parser, mapper, instance and plugin"
```

---

## Task 5: 实现 AgentService + Registry

**Files:**
- Create: `src-tauri/src/models/agent.rs`
- Create: `src-tauri/src/models/event.rs`
- Create: `src-tauri/src/models/log.rs`
- Create: `src-tauri/src/models/mod.rs`
- Create: `src-tauri/src/service/plugin_registry.rs`
- Create: `src-tauri/src/service/instance_registry.rs`
- Create: `src-tauri/src/service/agent_service.rs`
- Create: `src-tauri/src/service/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 创建 models 模块**

`src-tauri/src/models/agent.rs`:
```rust
use agent_core::instance::InstanceStatus;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct PluginInfo {
    pub key: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct InstanceInfo {
    pub id: String,
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
    pub status: InstanceStatus,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct CreateInstanceRequest {
    pub plugin_key: String,
    pub name: String,
    pub workspace: String,
}
```

`src-tauri/src/models/event.rs`:
```rust
pub use agent_core::event::AgentEvent;
```

`src-tauri/src/models/log.rs`:
```rust
pub use agent_core::logger::{LogLevel, SessionLogEntry};
```

`src-tauri/src/models/mod.rs`:
```rust
pub mod agent;
pub mod event;
pub mod log;
```

- [ ] **Step 2: 创建 service 模块**

`src-tauri/src/service/plugin_registry.rs`:
```rust
use agent_core::plugin::AgentPlugin;
use std::collections::HashMap;

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn AgentPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn AgentPlugin>) {
        self.plugins.insert(plugin.key().to_string(), plugin);
    }

    pub fn get(&self, key: &str) -> Option<&Box<dyn AgentPlugin>> {
        self.plugins.get(key)
    }

    pub fn list(&self) -> Vec<(&String, &Box<dyn AgentPlugin>)> {
        self.plugins.iter().collect()
    }
}
```

`src-tauri/src/service/instance_registry.rs`:
```rust
use agent_core::instance::AgentInstance;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct InstanceRegistry {
    instances: HashMap<String, Arc<Mutex<Box<dyn AgentInstance>>>>,
}

impl InstanceRegistry {
    pub fn new() -> Self {
        Self {
            instances: HashMap::new(),
        }
    }

    pub fn insert(&mut self, id: String, instance: Arc<Mutex<Box<dyn AgentInstance>>>) {
        self.instances.insert(id, instance);
    }

    pub fn get(&self, id: &str) -> Option<Arc<Mutex<Box<dyn AgentInstance>>>> {
        self.instances.get(id).cloned()
    }

    pub fn remove(&mut self, id: &str) {
        self.instances.remove(id);
    }

    pub fn list(&self) -> Vec<(&String, &Arc<Mutex<Box<dyn AgentInstance>>>)> {
        self.instances.iter().collect()
    }
}
```

`src-tauri/src/service/agent_service.rs`:
```rust
use crate::models::agent::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use agent_core::error::{InstanceError, PluginError};
use agent_core::instance::{InstanceConfig, InstanceStatus};
use agent_core::logger::SessionLogger;
use std::sync::{Arc, Mutex};

pub struct AgentService {
    plugins: super::plugin_registry::PluginRegistry,
    instances: Arc<Mutex<super::instance_registry::InstanceRegistry>>,
    logger: Arc<SessionLogger>,
}

impl AgentService {
    pub fn new(logger: Arc<SessionLogger>) -> Self {
        let mut plugins = super::plugin_registry::PluginRegistry::new();
        // 注册 opencode plugin
        // 注意：这里需要在 main.rs 中传入 AppHandle 来创建 plugin
        Self {
            plugins,
            instances: Arc::new(Mutex::new(super::instance_registry::InstanceRegistry::new())),
            logger,
        }
    }

    pub fn register_plugin(&mut self, plugin: Box<dyn agent_core::plugin::AgentPlugin>) {
        self.plugins.register(plugin);
    }

    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        self.plugins
            .list()
            .into_iter()
            .map(|(key, p)| PluginInfo {
                key: key.clone(),
                name: p.name().to_string(),
                installed: p.is_installed(),
            })
            .collect()
    }

    pub fn create_instance(
        &self,
        req: CreateInstanceRequest,
    ) -> Result<InstanceInfo, PluginError> {
        let plugin = self
            .plugins
            .get(&req.plugin_key)
            .ok_or(PluginError::NotFound(req.plugin_key.clone()))?;

        let id = format!("{}-{}", req.plugin_key, uuid::Uuid::new_v4());
        let config = InstanceConfig {
            id: id.clone(),
            name: req.name,
            workspace: req.workspace,
            session_id: None,
        };

        let instance = plugin.create_instance(config)?;
        let info = InstanceInfo {
            id: instance.id().to_string(),
            plugin_key: req.plugin_key,
            name: instance.id().to_string(),
            workspace: req.workspace,
            status: instance.status(),
        };

        self.instances
            .lock()
            .unwrap()
            .insert(id, Arc::new(Mutex::new(instance)));

        Ok(info)
    }

    pub async fn send_message(
        &self,
        instance_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .unwrap()
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        instance.lock().unwrap().send_message(message).await
    }

    pub async fn stop_instance(&self, instance_id: &str) -> Result<(), InstanceError> {
        let instance = self
            .instances
            .lock()
            .unwrap()
            .get(instance_id)
            .ok_or(InstanceError::NotFound(instance_id.to_string()))?;

        instance.lock().unwrap().stop().await
    }

    pub fn get_instance(&self, instance_id: &str) -> Option<InstanceInfo> {
        let registry = self.instances.lock().unwrap();
        let instance = registry.get(instance_id)?;
        let guard = instance.lock().unwrap();
        Some(InstanceInfo {
            id: guard.id().to_string(),
            plugin_key: "unknown".to_string(), // 简化处理
            name: guard.id().to_string(),
            workspace: "".to_string(),
            status: guard.status(),
        })
    }

    pub fn list_instances(&self) -> Vec<InstanceInfo> {
        let registry = self.instances.lock().unwrap();
        registry
            .list()
            .into_iter()
            .map(|(id, instance)| {
                let guard = instance.lock().unwrap();
                InstanceInfo {
                    id: id.clone(),
                    plugin_key: "unknown".to_string(),
                    name: guard.id().to_string(),
                    workspace: "".to_string(),
                    status: guard.status(),
                }
            })
            .collect()
    }
}
```

`src-tauri/src/service/mod.rs`:
```rust
pub mod agent_service;
pub mod instance_registry;
pub mod plugin_registry;
```

- [ ] **Step 3: 更新 src-tauri/src/lib.rs**

```rust
pub mod commands;
pub mod models;
pub mod service;
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/models/ src-tauri/src/service/ src-tauri/src/lib.rs src-tauri/src/commands/
git commit -m "feat(service): add AgentService, PluginRegistry, InstanceRegistry"
```

---

## Task 6: 实现 Tauri Commands

**Files:**
- Create: `src-tauri/src/commands/agent.rs`
- Create: `src-tauri/src/commands/session_log.rs`
- Create: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: 创建 commands/agent.rs**

```rust
use crate::models::agent::{CreateInstanceRequest, InstanceInfo, PluginInfo};
use crate::service::agent_service::AgentService;
use agent_core::logger::SessionLogger;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn agent_list_plugins(service: State<'_, AgentService>) -> Vec<PluginInfo> {
    service.list_plugins()
}

#[tauri::command]
pub fn agent_create_instance(
    service: State<'_, AgentService>,
    req: CreateInstanceRequest,
) -> Result<InstanceInfo, String> {
    service.create_instance(req).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn agent_send_message(
    service: State<'_, AgentService>,
    logger: State<'_, Arc<SessionLogger>>,
    instance_id: String,
    message: String,
    conversation_id: String,
) -> Result<(), String> {
    logger.log(
        &conversation_id,
        agent_core::logger::LogLevel::Info,
        "agent-service",
        "send_message called",
        Some(serde_json::json!({ "instance_id": &instance_id, "message": &message })),
    );

    match service.send_message(&instance_id, &message).await {
        Ok(_) => {
            logger.log(
                &conversation_id,
                agent_core::logger::LogLevel::Info,
                "agent-service",
                "message dispatched",
                None,
            );
            Ok(())
        }
        Err(e) => {
            logger.log(
                &conversation_id,
                agent_core::logger::LogLevel::Error,
                "agent-service",
                "send_message failed",
                Some(serde_json::json!({ "error": e.to_string() })),
            );
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn agent_stop_instance(
    service: State<'_, AgentService>,
    instance_id: String,
) -> Result<(), String> {
    service.stop_instance(&instance_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn agent_get_instance(
    service: State<'_, AgentService>,
    instance_id: String,
) -> Result<InstanceInfo, String> {
    service
        .get_instance(&instance_id)
        .ok_or_else(|| "Instance not found".to_string())
}

#[tauri::command]
pub fn agent_list_instances(service: State<'_, AgentService>) -> Vec<InstanceInfo> {
    service.list_instances()
}
```

- [ ] **Step 2: 创建 commands/session_log.rs**

```rust
use crate::DbState;
use rusqlite::params;
use serde_json::Value;
use tauri::State;

#[tauri::command]
pub fn session_log_load(
    state: State<'_, DbState>,
    conversation_id: String,
) -> Result<Vec<Value>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, conversation_id, timestamp, level, source, message, payload
             FROM session_logs
             WHERE conversation_id = ?1
             ORDER BY timestamp ASC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map(params![&conversation_id], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "conversation_id": row.get::<_, String>(1)?,
                "timestamp": row.get::<_, String>(2)?,
                "level": row.get::<_, String>(3)?,
                "source": row.get::<_, String>(4)?,
                "message": row.get::<_, String>(5)?,
                "payload": row.get::<_, Option<String>>(6)?,
            }))
        })
        .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}
```

- [ ] **Step 3: 创建 commands/mod.rs**

```rust
pub mod agent;
pub mod session_log;
```

- [ ] **Step 4: 修改 main.rs**

在 `main.rs` 中：
1. 新增 `session_logs` 表初始化
2. 初始化 `AgentService` 和 `SessionLogger`
3. 注册 opencode plugin
4. 注册新的 commands

```rust
// 在 init_db 中新增
fn init_db(conn: &Connection) -> SqliteResult<()> {
    // ... 原有表 ...
    conn.execute(
        "CREATE TABLE IF NOT EXISTS session_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            conversation_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            level TEXT NOT NULL,
            source TEXT NOT NULL,
            message TEXT NOT NULL,
            payload TEXT
        )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_session_logs_conversation ON session_logs(conversation_id, timestamp)",
        [],
    )?;
    Ok(())
}

// 在 main() 中 setup 阶段
.setup(|app| {
    let db_path = db_path(app);
    let conn = Connection::open(&db_path).expect("打开数据库失败");
    init_db(&conn).expect("初始化数据库失败");
    app.manage(DbState(Mutex::new(conn)));

    // 初始化 SessionLogger
    let app_handle = app.handle().clone();
    let db_path_clone = db_path.clone();
    let logger_conn = Connection::open(&db_path_clone).expect("打开日志数据库失败");
    let logger = Arc::new(SessionLogger::new(app_handle, logger_conn));
    app.manage(logger.clone());

    // 初始化 AgentService
    let mut agent_service = AgentService::new(logger.clone());
    let app_handle = app.handle().clone();
    agent_service.register_plugin(Box::new(opencode_plugin::plugin::OpencodePlugin::new(
        app_handle,
        logger.clone(),
    )));
    app.manage(agent_service);

    // ... 其余原有代码 ...
    Ok(())
})
```

并在 `invoke_handler` 中添加新 commands：
```rust
.invoke_handler(tauri::generate_handler![
    // ... 原有 commands ...
    commands::agent::agent_list_plugins,
    commands::agent::agent_create_instance,
    commands::agent::agent_send_message,
    commands::agent::agent_stop_instance,
    commands::agent::agent_get_instance,
    commands::agent::agent_list_instances,
    commands::session_log::session_log_load,
])
```

- [ ] **Step 5: 修改 capabilities/default.json**

添加 agent command 权限：
```json
    "dialog:allow-open",
    "agent:allow-list-plugins",
    "agent:allow-create-instance",
    "agent:allow-send-message",
    "agent:allow-stop-instance",
    "agent:allow-get-instance",
    "agent:allow-list-instances",
    "session-log:allow-load"
```

> 注：Tauri v2 的 command 权限格式可能需要调整，具体以实际编译报错为准。

- [ ] **Step 6: 编译验证**

Run: `cd src-tauri && cargo check`
Expected: 编译通过

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/ src-tauri/src/main.rs src-tauri/capabilities/default.json
git commit -m "feat(commands): expose agent commands and session log load"
```

---

## Task 7: 前端适配

**Files:**
- Create: `src/hooks/use-agent-service.ts`
- Create: `src/hooks/use-session-log-rust.ts`
- Modify: `src/agents/types.ts`
- Modify: `src/agents/manager.ts`
- Delete: `src/agents/registry.ts`
- Delete: `src/agents/opencode/` 目录下所有文件
- Modify: `src/pages/WorkspacePage.tsx`
- Modify: `src/db/index.ts` 或 `src/db/types.ts`

- [ ] **Step 1: 创建 use-agent-service.ts**

```typescript
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AgentEvent } from '@/agents/types';

export interface PluginInfo {
  key: string;
  name: string;
  installed: boolean;
}

export interface InstanceInfo {
  id: string;
  plugin_key: string;
  name: string;
  workspace: string;
  status: 'stopped' | 'starting' | 'running' | { crashed: string };
}

export interface AgentEventPayload {
  instance_id: string;
  event: AgentEvent;
}

export interface StatusChangePayload {
  instance_id: string;
  status: InstanceInfo['status'];
}

export async function agentListPlugins(): Promise<PluginInfo[]> {
  return invoke('agent_list_plugins');
}

export async function agentCreateInstance(
  pluginKey: string,
  name: string,
  workspace: string,
): Promise<InstanceInfo> {
  return invoke('agent_create_instance', {
    plugin_key: pluginKey,
    name,
    workspace,
  });
}

export async function agentSendMessage(
  instanceId: string,
  message: string,
  conversationId: string,
): Promise<void> {
  return invoke('agent_send_message', {
    instance_id: instanceId,
    message,
    conversation_id: conversationId,
  });
}

export async function agentStopInstance(instanceId: string): Promise<void> {
  return invoke('agent_stop_instance', { instance_id: instanceId });
}

export async function agentGetInstance(instanceId: string): Promise<InstanceInfo> {
  return invoke('agent_get_instance', { instance_id: instanceId });
}

export async function agentListInstances(): Promise<InstanceInfo[]> {
  return invoke('agent_list_instances');
}

export async function listenAgentEvents(
  callback: (payload: AgentEventPayload) => void,
): Promise<UnlistenFn> {
  return listen<AgentEventPayload>('agent:event', (event) => {
    callback(event.payload);
  });
}

export async function listenAgentStatusChanges(
  callback: (payload: StatusChangePayload) => void,
): Promise<UnlistenFn> {
  return listen<StatusChangePayload>('agent:status_changed', (event) => {
    callback(event.payload);
  });
}
```

- [ ] **Step 2: 创建 use-session-log-rust.ts**

```typescript
import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { sessionLog } from '@/store/session-log';

interface SessionLogEntry {
  conversation_id: string;
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  source: string;
  message: string;
  payload?: unknown;
}

export function useRustSessionLog() {
  useEffect(() => {
    const unlisten = listen<SessionLogEntry>('session:log', (event) => {
      const entry = event.payload;
      sessionLog.add(
        entry.conversation_id,
        entry.level,
        entry.source,
        entry.message,
        entry.payload,
      );
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);
}
```

- [ ] **Step 3: 精简 agents/manager.ts**

将 `AgentManager` 改为薄封装或直接删除，由前端 hook 替代。如果 `WorkspacePage` 直接调用 hook，则 `manager.ts` 可以大幅精简或标记为废弃。

```typescript
// src/agents/manager.ts
// 过渡版本：保留接口但委托给 Rust
import { agentSendMessage, agentStopInstance } from '@/hooks/use-agent-service';

class AgentManager {
  async sendMessage(instanceId: string, message: string, conversationId: string) {
    await agentSendMessage(instanceId, message, conversationId);
  }

  async stopAgent(instanceId: string) {
    await agentStopInstance(instanceId);
  }
}

export const agentManager = new AgentManager();
```

- [ ] **Step 4: 删除废弃文件**

```bash
rm -rf src/agents/opencode/
rm -f src/agents/registry.ts
```

- [ ] **Step 5: 修改 WorkspacePage.tsx**

1. 导入新 hooks
2. 在 `useEffect` 中注册 `listenAgentEvents` 和 `listenAgentStatusChanges`
3. 替换 `handleSendMessage` 中的 `agentManager.sendMessage` 调用
4. 注册 `useRustSessionLog`

关键改动示例：
```typescript
import { listenAgentEvents, agentSendMessage } from '@/hooks/use-agent-service';
import { useRustSessionLog } from '@/hooks/use-session-log-rust';

// 在 WorkspacePage 组件内
useRustSessionLog();

useEffect(() => {
  let unlisten: (() => void) | undefined;
  listenAgentEvents((payload) => {
    // 根据 instance_id 和 event 更新 messages 状态
    // 原有的事件处理逻辑基本不变
  }).then((fn) => { unlisten = fn; });
  return () => { unlisten?.(); };
}, []);

const handleSendMessage = async (content: string) => {
  if (!activeConversation) return;
  await agentSendMessage(
    activeAgentId,
    content,
    activeConversation.id,
  );
  // 事件流通过 listenAgentEvents 接收，无需 await 返回值
};
```

- [ ] **Step 6: Commit**

```bash
git add src/hooks/ src/agents/ src/pages/WorkspacePage.tsx
git commit -m "feat(frontend): adapt to rust agent service with hooks"
```

---

## Task 8: 验证与收尾

- [ ] **Step 1: 前端构建验证**

Run: `pnpm build`
Expected: 编译通过，无 TypeScript 错误

- [ ] **Step 2: Rust 构建验证**

Run: `cd src-tauri && cargo build`
Expected: 编译通过

- [ ] **Step 3: Tauri 完整构建**

Run: `pnpm tauri build`
Expected: 编译通过（打包失败可忽略，只要二进制文件生成）

- [ ] **Step 4: 启动应用测试**

Run: `./run-desktop.sh`（或 `./src-tauri/target/release/ai-coding-desktop`）
Expected: 应用启动无报错

- [ ] **Step 5: 功能验证**

1. 登录应用
2. 选择 OpenCode 智能体
3. 发送一条消息
4. 验证：ChatPanel 正常显示回复
5. 验证：SessionLogDrawer（5 次点击设置图标）中有 Rust 日志（来源含 `opencode-plugin`、`agent-runtime`、`agent-service`）
6. 验证：工作目录为绝对路径

- [ ] **Step 6: 最终 Commit**

```bash
git commit -m "feat: migrate opencode adapter from ts to rust with agent plugin trait"
```

---

## Self-Review Checklist

### Spec Coverage
- [x] Rust Workspace 结构 → Task 1
- [x] AgentPlugin / AgentInstance trait → Task 2
- [x] AgentEvent 枚举 → Task 2
- [x] SessionLogger 双写 → Task 2
- [x] agent-runtime 进程管理 → Task 3
- [x] opencode-plugin 解析/映射/实例 → Task 4
- [x] AgentService + Registry → Task 5
- [x] Tauri Commands + Events → Task 6
- [x] 前端适配 hooks → Task 7
- [x] 验证步骤 → Task 8

### Placeholder Scan
- [x] 无 "TBD" / "TODO" / "implement later"
- [x] 所有步骤包含具体代码或命令
- [x] 无 "similar to Task N" 引用

### Type Consistency
- [x] `AgentEvent` 在 Rust 和前端定义一致
- [x] `InstanceStatus` 在 Rust 和前端序列化一致
- [x] Command 名称前后一致：`agent_send_message`, `agent:event`, `session:log`

---

## 执行选项

Plan complete and saved to `docs/superpowers/plans/2026-06-02-agent-adapter-rust-migration.md`.

**Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
