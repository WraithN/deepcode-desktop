# Agent Gateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a hybrid Agent Gateway layer that manages local OpenCode sidecar processes via Rust backend and handles HTTP/SSE communication from the frontend, with per-agent-instance SQLite data isolation.

**Architecture:** Rust SidecarManager handles process lifecycle (start/stop/health-check/port-allocation, max 6 instances). Frontend AgentGateway directly calls OpenCode HTTP API with SSE streaming. Each agent instance has its own SQLite file in `app_data/agents/{instance_id}/data.db`. OpenCode adapter converts OpenCode-specific events to generic AgentEvent types.

**Tech Stack:** Rust (tauri, std::process, rusqlite), TypeScript (React, Tauri API, fetch EventSource), SQLite

---

## File Structure

### Rust Backend (`src-tauri/src/`)
- `sidecar_manager.rs` — Process management, port allocation, health checks
- `agent_db.rs` — Per-agent SQLite CRUD operations
- `commands.rs` — Tauri commands exposed to frontend
- `main.rs` — Entry point, wire up modules and state

### Frontend (`src/`)
- `src/agents/types.ts` — AgentAdapter interface, AgentEvent types, AgentStatus
- `src/agents/registry.ts` — Adapter registry mapping agentKey -> Adapter class
- `src/agents/manager.ts` — Frontend state: instance list, connection status, message routing
- `src/agents/gateway.ts` — HTTP caller + SSE stream consumer
- `src/agents/opencode/adapter.ts` — OpenCode-specific Adapter implementation
- `src/agents/opencode/parser.ts` — SSE event parser for OpenCode format
- `src/agents/opencode/types.ts` — OpenCode API response types

---

## Task 1: Rust — SidecarManager Foundation

**Files:**
- Create: `src-tauri/src/sidecar_manager.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Define SidecarInstance struct and SidecarManager**

```rust
// src-tauri/src/sidecar_manager.rs
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::Manager;

#[derive(Debug, Clone)]
pub enum SidecarStatus {
    Starting,
    Running { pid: u32 },
    Crashed { error: Option<String> },
    Stopped,
}

#[derive(Debug, Clone)]
pub struct SidecarInstance {
    pub instance_id: String,
    pub agent_key: String,
    pub port: u16,
    pub workspace: String,
    pub status: SidecarStatus,
    pub pid: Option<u32>,
}

pub struct SidecarManager {
    pub instances: Mutex<HashMap<String, SidecarInstance>>,
    pub port_pool: Mutex<Vec<u16>>,
}

impl SidecarManager {
    pub fn new() -> Self {
        Self {
            instances: Mutex::new(HashMap::new()),
            port_pool: Mutex::new((4000..=4005).collect()),
        }
    }

    pub fn get_available_port(&self) -> Option<u16> {
        let mut pool = self.port_pool.lock().unwrap();
        pool.pop()
    }

    pub fn release_port(&self, port: u16) {
        let mut pool = self.port_pool.lock().unwrap();
        if !pool.contains(&port) {
            pool.push(port);
        }
    }

    pub fn instance_count(&self) -> usize {
        self.instances.lock().unwrap().len()
    }
}
```

- [ ] **Step 2: Add start_sidecar command**

```rust
// Append to src-tauri/src/sidecar_manager.rs

use serde_json::Value;

#[tauri::command]
pub fn start_sidecar(
    state: tauri::State<SidecarManager>,
    instance_id: String,
    agent_key: String,
    workspace: String,
) -> Result<Value, String> {
    let count = state.instance_count();
    if count >= 6 {
        return Err("最多同时运行6个智能体实例".to_string());
    }

    let port = state.get_available_port()
        .ok_or("无可用端口 (4000-4005 已被占用)".to_string())?;

    // For now, only support opencode
    if agent_key != "opencode" {
        return Err(format!("暂不支持智能体: {}", agent_key));
    }

    let mut child = Command::new("opencode")
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--cors")
        .arg("*")
        .current_dir(&workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 opencode 失败: {}", e))?;

    let pid = child.id();

    let instance = SidecarInstance {
        instance_id: instance_id.clone(),
        agent_key,
        port,
        workspace,
        status: SidecarStatus::Running { pid },
        pid: Some(pid),
    };

    state.instances.lock().unwrap().insert(instance_id.clone(), instance);

    // Store child process to keep it alive — we need a different approach
    // For now, we return success and rely on the OS to keep the process
    std::mem::forget(child);

    Ok(serde_json::json!({
        "instanceId": instance_id,
        "port": port,
        "pid": pid,
        "status": "running"
    }))
}
```

- [ ] **Step 3: Add stop_sidecar and get_sidecar commands**

```rust
// Append to src-tauri/src/sidecar_manager.rs

#[tauri::command]
pub fn stop_sidecar(
    state: tauri::State<SidecarManager>,
    instance_id: String,
) -> Result<(), String> {
    let mut instances = state.instances.lock().unwrap();
    if let Some(instance) = instances.remove(&instance_id) {
        state.release_port(instance.port);
        // Try to kill by PID
        if let Some(pid) = instance.pid {
            let _ = Command::new("kill").arg(pid.to_string()).output();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_sidecar_status(
    state: tauri::State<SidecarManager>,
    instance_id: String,
) -> Result<Value, String> {
    let instances = state.instances.lock().unwrap();
    let instance = instances.get(&instance_id)
        .ok_or("实例不存在".to_string())?;

    let status_str = match &instance.status {
        SidecarStatus::Starting => "starting",
        SidecarStatus::Running { .. } => "running",
        SidecarStatus::Crashed { .. } => "crashed",
        SidecarStatus::Stopped => "stopped",
    };

    Ok(serde_json::json!({
        "instanceId": instance.instance_id,
        "agentKey": instance.agent_key,
        "port": instance.port,
        "status": status_str,
        "pid": instance.pid,
        "workspace": instance.workspace,
    }))
}

#[tauri::command]
pub fn check_opencode_installed() -> Result<String, String> {
    let output = Command::new("which")
        .arg("opencode")
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(path)
    } else {
        Err("opencode 未安装".to_string())
    }
}
```

- [ ] **Step 4: Wire up in main.rs**

```rust
// In src-tauri/src/main.rs, add at the top:
mod sidecar_manager;
use sidecar_manager::{SidecarManager, start_sidecar, stop_sidecar, get_sidecar_status, check_opencode_installed};

// In main() function, add to .invoke_handler():
// start_sidecar,
// stop_sidecar,
// get_sidecar_status,
// check_opencode_installed,

// In .setup(), add:
// app.manage(SidecarManager::new());
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/sidecar_manager.rs src-tauri/src/main.rs
git commit -m "feat(rust): add SidecarManager for agent process lifecycle"
```

---

## Task 2: Rust — AgentDbManager (Per-Agent SQLite)

**Files:**
- Create: `src-tauri/src/agent_db.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Define AgentDbManager with per-agent connections**

```rust
// src-tauri/src/agent_db.rs
use rusqlite::{Connection, Result as SqliteResult, params};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

pub struct AgentDbManager {
    pub connections: Mutex<HashMap<String, Connection>>,
}

impl AgentDbManager {
    pub fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
        }
    }

    fn db_path(app_handle: &tauri::AppHandle, instance_id: &str) -> PathBuf {
        let mut path = app_handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
        path.push("agents");
        path.push(instance_id);
        std::fs::create_dir_all(&path).ok();
        path.push("data.db");
        path
    }

    pub fn get_or_create_connection(
        &self,
        app_handle: &tauri::AppHandle,
        instance_id: &str,
    ) -> SqliteResult<Connection> {
        let mut conns = self.connections.lock().unwrap();
        if let Some(conn) = conns.get(instance_id) {
            return Ok(conn.clone());
        }

        let path = Self::db_path(app_handle, instance_id);
        let conn = Connection::open(&path)?;
        Self::init_schema(&conn)?;
        conns.insert(instance_id.to_string(), conn.clone());
        Ok(conn)
    }

    fn init_schema(conn: &Connection) -> SqliteResult<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                model TEXT,
                created_at TEXT,
                updated_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                steps TEXT,
                is_complete INTEGER DEFAULT 0,
                token_in INTEGER,
                token_out INTEGER,
                duration_ms INTEGER,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS modified_files (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                file_path TEXT NOT NULL,
                change_type TEXT NOT NULL,
                diff TEXT,
                created_at TEXT
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS agent_meta (
                key TEXT PRIMARY KEY,
                value TEXT
            )",
            [],
        )?;
        Ok(())
    }

    pub fn delete_agent_db(&self, app_handle: &tauri::AppHandle, instance_id: &str) -> SqliteResult<()> {
        let mut conns = self.connections.lock().unwrap();
        conns.remove(instance_id);
        let path = Self::db_path(app_handle, instance_id);
        let parent = path.parent().unwrap();
        let _ = std::fs::remove_dir_all(parent);
        Ok(())
    }
}
```

- [ ] **Step 2: Add per-agent CRUD commands**

```rust
// Append to src-tauri/src/agent_db.rs

#[tauri::command]
pub fn agent_db_create_conversation(
    app_handle: tauri::AppHandle,
    state: tauri::State<AgentDbManager>,
    instance_id: String,
    data: Value,
) -> Result<Value, String> {
    let conn = state.get_or_create_connection(&app_handle, &instance_id)
        .map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let title = data["title"].as_str().unwrap_or("");
    let model = data["model"].as_str().unwrap_or("");

    conn.execute(
        "INSERT INTO conversations (id, title, model, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, title, model, &now, &now],
    ).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "id": id,
        "title": title,
        "model": model,
        "created_at": now,
        "updated_at": now,
    }))
}

#[tauri::command]
pub fn agent_db_load_conversations(
    app_handle: tauri::AppHandle,
    state: tauri::State<AgentDbManager>,
    instance_id: String,
) -> Result<Vec<Value>, String> {
    let conn = state.get_or_create_connection(&app_handle, &instance_id)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, title, model, created_at, updated_at FROM conversations ORDER BY updated_at DESC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "title": row.get::<_, String>(1)?,
            "model": row.get::<_, String>(2)?,
            "created_at": row.get::<_, String>(3)?,
            "updated_at": row.get::<_, String>(4)?,
        }))
    }).map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
pub fn agent_db_create_message(
    app_handle: tauri::AppHandle,
    state: tauri::State<AgentDbManager>,
    instance_id: String,
    data: Value,
) -> Result<Value, String> {
    let conn = state.get_or_create_connection(&app_handle, &instance_id)
        .map_err(|e| e.to_string())?;
    let id = format!("{}-{}", uuid::Uuid::new_v4(), chrono::Utc::now().timestamp_millis());
    let now = chrono::Utc::now().to_rfc3339();
    let conversation_id = data["conversation_id"].as_str().unwrap_or("");
    let role = data["role"].as_str().unwrap_or("");
    let content = data["content"].as_str().unwrap_or("");

    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![&id, conversation_id, role, content, &now],
    ).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "id": id,
        "conversation_id": conversation_id,
        "role": role,
        "content": content,
        "created_at": now,
    }))
}

#[tauri::command]
pub fn agent_db_load_messages(
    app_handle: tauri::AppHandle,
    state: tauri::State<AgentDbManager>,
    instance_id: String,
    conversation_id: String,
) -> Result<Vec<Value>, String> {
    let conn = state.get_or_create_connection(&app_handle, &instance_id)
        .map_err(|e| e.to_string())?;
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, steps, is_complete, token_in, token_out, duration_ms, created_at FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![&conversation_id], |row| {
        Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?,
            "conversation_id": row.get::<_, String>(1)?,
            "role": row.get::<_, String>(2)?,
            "content": row.get::<_, String>(3)?,
            "steps": row.get::<_, Option<String>>(4)?.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            "is_complete": row.get::<_, i32>(5)? == 1,
            "token_in": row.get::<_, Option<i64>>(6)?,
            "token_out": row.get::<_, Option<i64>>(7)?,
            "duration_ms": row.get::<_, Option<i64>>(8)?,
            "created_at": row.get::<_, String>(9)?,
        }))
    }).map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.map_err(|e| e.to_string())?);
    }
    Ok(results)
}

#[tauri::command]
pub fn agent_db_delete_agent(
    app_handle: tauri::AppHandle,
    state: tauri::State<AgentDbManager>,
    instance_id: String,
) -> Result<(), String> {
    state.delete_agent_db(&app_handle, &instance_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 3: Wire up in main.rs**

Add `mod agent_db;`, `use agent_db::{AgentDbManager, ...};`, register all agent_db_* commands in `invoke_handler`, and `app.manage(AgentDbManager::new())` in setup.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent_db.rs src-tauri/src/main.rs
git commit -m "feat(rust): add AgentDbManager with per-agent SQLite isolation"
```

---

## Task 3: Frontend — Agent Types and Registry

**Files:**
- Create: `src/agents/types.ts`
- Create: `src/agents/registry.ts`
- Create: `src/agents/opencode/types.ts`

- [ ] **Step 1: Write core agent types**

```typescript
// src/agents/types.ts

export interface Question {
  id: string;
  label: string;
  type: 'choice' | 'custom';
  options?: string[];
  required?: boolean;
}

export type AgentEvent =
  | { type: 'thinking'; content: string }
  | { type: 'tool_use'; toolName: string; args: Record<string, unknown> }
  | { type: 'tool_result'; toolName: string; result: string; failed?: boolean }
  | { type: 'ask_permission'; toolName: string; message: string }
  | { type: 'ask_user'; questions: Question[] }
  | { type: 'text_delta'; content: string }
  | { type: 'done' }
  | { type: 'error'; message: string };

export interface AgentStartConfig {
  instanceId: string;
  workspace: string;
  port?: number;
}

export type AgentStatus =
  | { state: 'stopped' }
  | { state: 'starting' }
  | { state: 'running'; port: number; pid: number }
  | { state: 'crashed'; error?: string };

export interface AgentAdapter {
  readonly agentKey: string;
  readonly displayName: string;

  isInstalled(): Promise<boolean>;
  start(config: AgentStartConfig): Promise<void>;
  stop(instanceId: string): Promise<void>;
  sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent, void, unknown>;
  setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void>;
  getStatus(instanceId: string): Promise<AgentStatus>;
}
```

- [ ] **Step 2: Write adapter registry**

```typescript
// src/agents/registry.ts
import type { AgentAdapter } from './types';

class AgentRegistry {
  private adapters = new Map<string, AgentAdapter>();

  register(adapter: AgentAdapter) {
    this.adapters.set(adapter.agentKey, adapter);
  }

  get(agentKey: string): AgentAdapter | undefined {
    return this.adapters.get(agentKey);
  }

  has(agentKey: string): boolean {
    return this.adapters.has(agentKey);
  }
}

export const agentRegistry = new AgentRegistry();
```

- [ ] **Step 3: Write OpenCode API types**

```typescript
// src/agents/opencode/types.ts

export interface OpencodeMessageRequest {
  message: string;
  session_id?: string;
}

export interface OpencodeSSEEvent {
  event: string;
  data: string;
}

export type OpencodeEventType =
  | 'thinking'
  | 'tool_use'
  | 'tool_result'
  | 'permission_request'
  | 'question'
  | 'content_delta'
  | 'done'
  | 'error';
```

- [ ] **Step 4: Commit**

```bash
git add src/agents/types.ts src/agents/registry.ts src/agents/opencode/types.ts
git commit -m "feat(agents): add core agent types, registry, and opencode API types"
```

---

## Task 4: Frontend — OpenCode Adapter and SSE Parser

**Files:**
- Create: `src/agents/opencode/parser.ts`
- Create: `src/agents/opencode/adapter.ts`

- [ ] **Step 1: Write SSE parser for OpenCode format**

```typescript
// src/agents/opencode/parser.ts
import type { AgentEvent } from '../types';

export function parseOpencodeEvent(eventLine: string, dataLine: string): AgentEvent | null {
  const event = eventLine.replace('event: ', '').trim();
  const data = dataLine.replace('data: ', '').trim();

  try {
    const payload = JSON.parse(data);

    switch (event) {
      case 'thinking':
        return { type: 'thinking', content: payload.content || data };
      case 'tool_use':
        return {
          type: 'tool_use',
          toolName: payload.tool_name || payload.name || 'unknown',
          args: payload.args || payload.arguments || {},
        };
      case 'tool_result':
        return {
          type: 'tool_result',
          toolName: payload.tool_name || 'unknown',
          result: payload.result || payload.content || '',
          failed: payload.failed || payload.error != null,
        };
      case 'permission_request':
        return {
          type: 'ask_permission',
          toolName: payload.tool_name || 'unknown',
          message: payload.message || data,
        };
      case 'question':
        return {
          type: 'ask_user',
          questions: payload.questions || [],
        };
      case 'content_delta':
      case 'delta':
        return { type: 'text_delta', content: payload.content || payload.delta || data };
      case 'done':
      case 'complete':
        return { type: 'done' };
      case 'error':
        return { type: 'error', message: payload.message || payload.error || data };
      default:
        // Fallback: treat unknown events as text_delta if they have content
        if (payload.content) {
          return { type: 'text_delta', content: payload.content };
        }
        return null;
    }
  } catch {
    // If data is not JSON, treat as raw text delta
    if (event === 'message' || event === 'delta' || event === 'content') {
      return { type: 'text_delta', content: data };
    }
    return null;
  }
}
```

- [ ] **Step 2: Write OpencodeAdapter**

```typescript
// src/agents/opencode/adapter.ts
import { invoke } from '@tauri-apps/api/core';
import type { AgentAdapter, AgentEvent, AgentStartConfig, AgentStatus } from '../types';
import { parseOpencodeEvent } from './parser';

export class OpencodeAdapter implements AgentAdapter {
  readonly agentKey = 'opencode';
  readonly displayName = 'OpenCode';

  async isInstalled(): Promise<boolean> {
    try {
      const result = await invoke<string>('check_opencode_installed');
      return !!result;
    } catch {
      return false;
    }
  }

  async start(config: AgentStartConfig): Promise<void> {
    await invoke('start_sidecar', {
      instanceId: config.instanceId,
      agentKey: this.agentKey,
      workspace: config.workspace,
    });
  }

  async stop(instanceId: string): Promise<void> {
    await invoke('stop_sidecar', { instanceId });
  }

  async *sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent, void, unknown> {
    const status = await this.getStatus(instanceId);
    if (status.state !== 'running') {
      yield { type: 'error', message: '智能体未运行' };
      return;
    }

    const port = (status as Extract<AgentStatus, { state: 'running' }>).port;
    const url = `http://127.0.0.1:${port}/v1/messages`;

    const response = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message }),
    });

    if (!response.ok) {
      yield { type: 'error', message: `HTTP ${response.status}: ${response.statusText}` };
      return;
    }

    const reader = response.body?.getReader();
    if (!reader) {
      yield { type: 'error', message: '无法读取响应流' };
      return;
    }

    const decoder = new TextDecoder();
    let buffer = '';

    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');
        buffer = lines.pop() || '';

        let currentEvent: string | null = null;
        for (const line of lines) {
          if (line.startsWith('event:')) {
            currentEvent = line;
          } else if (line.startsWith('data:') && currentEvent) {
            const event = parseOpencodeEvent(currentEvent, line);
            if (event) yield event;
            currentEvent = null;
          }
        }
      }

      // Process remaining buffer
      if (buffer.trim()) {
        const lines = buffer.split('\n');
        let currentEvent: string | null = null;
        for (const line of lines) {
          if (line.startsWith('event:')) {
            currentEvent = line;
          } else if (line.startsWith('data:') && currentEvent) {
            const event = parseOpencodeEvent(currentEvent, line);
            if (event) yield event;
            currentEvent = null;
          }
        }
      }
    } finally {
      reader.releaseLock();
    }

    yield { type: 'done' };
  }

  async setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void> {
    const status = await this.getStatus(instanceId);
    if (status.state !== 'running') return;
    const port = (status as Extract<AgentStatus, { state: 'running' }>).port;
    const url = `http://127.0.0.1:${port}/v1/agents/mode`;
    await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ mode }),
    });
  }

  async getStatus(instanceId: string): Promise<AgentStatus> {
    try {
      const result = await invoke<{
        instanceId: string;
        agentKey: string;
        port: number;
        status: string;
        pid: number;
        workspace: string;
      }>('get_sidecar_status', { instanceId });

      if (result.status === 'running') {
        return { state: 'running', port: result.port, pid: result.pid };
      } else if (result.status === 'crashed') {
        return { state: 'crashed' };
      } else if (result.status === 'starting') {
        return { state: 'starting' };
      }
      return { state: 'stopped' };
    } catch {
      return { state: 'stopped' };
    }
  }
}
```

- [ ] **Step 3: Register adapter in registry**

```typescript
// Append to src/agents/registry.ts
import { OpencodeAdapter } from './opencode/adapter';

agentRegistry.register(new OpencodeAdapter());
```

- [ ] **Step 4: Commit**

```bash
git add src/agents/opencode/parser.ts src/agents/opencode/adapter.ts src/agents/registry.ts
git commit -m "feat(agents): add OpenCode adapter with SSE parser"
```

---

## Task 5: Frontend — AgentManager State Management

**Files:**
- Create: `src/agents/manager.ts`

- [ ] **Step 1: Write AgentManager**

```typescript
// src/agents/manager.ts
import { agentRegistry } from './registry';
import type { AgentAdapter, AgentEvent, AgentStatus } from './types';

export interface ManagedAgent {
  instanceId: string;
  agentKey: string;
  displayName: string;
  workspace: string;
  status: AgentStatus;
  adapter: AgentAdapter;
}

class AgentManager {
  private agents = new Map<string, ManagedAgent>();
  private listeners = new Set<() => void>();

  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify() {
    this.listeners.forEach((l) => l());
  }

  getAgents(): ManagedAgent[] {
    return Array.from(this.agents.values());
  }

  getAgent(instanceId: string): ManagedAgent | undefined {
    return this.agents.get(instanceId);
  }

  async addAgent(agentKey: string, instanceId: string, displayName: string, workspace: string): Promise<void> {
    const adapter = agentRegistry.get(agentKey);
    if (!adapter) {
      throw new Error(`未知智能体类型: ${agentKey}`);
    }

    const isInstalled = await adapter.isInstalled();
    if (!isInstalled) {
      throw new Error(`智能体 ${adapter.displayName} 尚未安装`);
    }

    const agent: ManagedAgent = {
      instanceId,
      agentKey,
      displayName,
      workspace,
      status: { state: 'stopped' },
      adapter,
    };
    this.agents.set(instanceId, agent);
    this.notify();
  }

  async startAgent(instanceId: string): Promise<void> {
    const agent = this.agents.get(instanceId);
    if (!agent) return;

    agent.status = { state: 'starting' };
    this.notify();

    try {
      await agent.adapter.start({ instanceId, workspace: agent.workspace });
      const status = await agent.adapter.getStatus(instanceId);
      agent.status = status;
    } catch (error) {
      agent.status = { state: 'crashed', error: String(error) };
    }
    this.notify();
  }

  async stopAgent(instanceId: string): Promise<void> {
    const agent = this.agents.get(instanceId);
    if (!agent) return;

    await agent.adapter.stop(instanceId);
    agent.status = { state: 'stopped' };
    this.notify();
  }

  async removeAgent(instanceId: string): Promise<void> {
    const agent = this.agents.get(instanceId);
    if (!agent) return;

    if (agent.status.state === 'running') {
      await agent.adapter.stop(instanceId);
    }

    this.agents.delete(instanceId);
    this.notify();
  }

  async *sendMessage(instanceId: string, message: string): AsyncGenerator<AgentEvent, void, unknown> {
    const agent = this.agents.get(instanceId);
    if (!agent) {
      yield { type: 'error', message: '智能体不存在' };
      return;
    }

    yield* agent.adapter.sendMessage(instanceId, message);
  }

  async setMode(instanceId: string, mode: 'build' | 'plan'): Promise<void> {
    const agent = this.agents.get(instanceId);
    if (!agent) return;
    await agent.adapter.setMode(instanceId, mode);
  }
}

export const agentManager = new AgentManager();
```

- [ ] **Step 2: Commit**

```bash
git add src/agents/manager.ts
git commit -m "feat(agents): add AgentManager for frontend state management"
```

---

## Task 6: Frontend — Integrate with WorkspacePage

**Files:**
- Modify: `src/pages/WorkspacePage.tsx`

- [ ] **Step 1: Add AgentManager integration to handleSendMessage**

In `src/pages/WorkspacePage.tsx`, replace the mock `handleSendMessage` with real agent gateway call:

```typescript
// In handleSendMessage, replace the setTimeout mock block with:

setIsTyping(true);
const startTime = Date.now();

// Collect all events into steps
const steps: MessageStep[] = [];
let finalContent = '';
let hasError = false;

const activeAgentInstance = agentInstances.find((a) => a.id === activeAgentId);
if (!activeAgentInstance) {
  toast.error('未找到活跃智能体');
  setIsTyping(false);
  return;
}

try {
  for await (const event of agentManager.sendMessage(activeAgentInstance.id, content)) {
    switch (event.type) {
      case 'thinking':
        steps.push({ type: 'thinking', content: event.content });
        break;
      case 'tool_use':
        steps.push({
          type: 'tool_use',
          content: `使用工具 ${event.toolName}...`,
          toolName: event.toolName,
          summary: { file: event.args?.file as string, lines: 0, durationMs: 0 },
        });
        break;
      case 'tool_result':
        steps.push({
          type: 'tool_result',
          content: event.result,
          toolName: event.toolName,
          failed: event.failed,
          summary: { file: event.toolName, lines: 0, durationMs: 0 },
        });
        break;
      case 'ask_permission':
        steps.push({
          type: 'ask_permission',
          content: event.message,
          permissionType: event.toolName,
        });
        break;
      case 'ask_user':
        steps.push({
          type: 'ask_user',
          content: '请回答以下问题：',
          questions: event.questions,
        });
        break;
      case 'text_delta':
        finalContent += event.content;
        break;
      case 'error':
        hasError = true;
        toast.error(`智能体错误: ${event.message}`);
        break;
      case 'done':
        break;
    }
  }
} catch (error) {
  hasError = true;
  toast.error(`通信错误: ${String(error)}`);
}

const duration_ms = Date.now() - startTime;

// Save AI message
if (!hasError) {
  const aiMsg = await db.createMessage({
    conversation_id: activeConversation.id,
    role: 'assistant',
    content: finalContent || '（无内容）',
  });

  if (aiMsg) {
    const enriched: Message = {
      ...aiMsg,
      steps,
      is_complete: true,
      token_in: Math.floor(content.length * 0.8),
      token_out: Math.floor(finalContent.length * 0.9),
      duration_ms,
    };
    setMessages((prev) => [...prev, enriched]);
  }
}

setIsTyping(false);
```

- [ ] **Step 2: Add agent start/stop logic in handleActivateAgent**

When activating an agent, check if it's running and start if needed:

```typescript
const handleActivateAgent = async (id: string) => {
  setActiveAgentId(id);
  const instance = agentInstances.find((a) => a.id === id);
  if (!instance) return;

  // Ensure agent is managed and started
  let managed = agentManager.getAgent(id);
  if (!managed) {
    await agentManager.addAgent(instance.agentKey, id, instance.displayName, instance.workspace);
    managed = agentManager.getAgent(id)!;
  }

  if (managed.status.state === 'stopped') {
    await agentManager.startAgent(id);
  }

  // ... rest of existing logic
};
```

- [ ] **Step 3: Add installation check UI**

In the agent activation flow, show installation warning:

```typescript
// When user tries to activate opencode but it's not installed:
const managed = agentManager.getAgent(id);
if (managed?.status.state === 'crashed') {
  toast.error(`智能体 ${instance.displayName} 启动失败，请检查是否已安装`);
}
```

- [ ] **Step 4: Commit**

```bash
git add src/pages/WorkspacePage.tsx
git commit -m "feat(workspace): integrate AgentGateway into message flow"
```

---

## Task 7: Cleanup Mock Code and Verify Build

**Files:**
- Modify: `src/pages/WorkspacePage.tsx`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Remove generateAIReply mock function**

Remove or comment out the `generateAIReply` function in WorkspacePage.tsx since real responses now come from OpenCode.

- [ ] **Step 2: Verify TypeScript compilation**

```bash
npx tsc --noEmit
```
Expected: No errors.

- [ ] **Step 3: Verify Rust compilation**

```bash
cd src-tauri && cargo check
```
Expected: No errors.

- [ ] **Step 4: Verify frontend build**

```bash
npx vite build
```
Expected: Build succeeds.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: remove mock AI reply, switch to AgentGateway"
```

---

## Self-Review Checklist

**1. Spec coverage:**
- ✅ Sidecar 进程管理 (Task 1)
- ✅ 端口分配 4000-4005 (Task 1)
- ✅ 最多 6 个实例限制 (Task 1)
- ✅ 每智能体独立 SQLite (Task 2)
- ✅ OpenCode 适配器 (Task 4)
- ✅ SSE 流式解析 (Task 4)
- ✅ 模式切换接口 (Task 4)
- ✅ 安装检测 + 报错提示 (Task 4)
- ✅ 前端状态管理 (Task 5)
- ✅ WorkspacePage 集成 (Task 6)

**2. Placeholder scan:**
- ✅ 无 TBD/TODO
- ✅ 所有步骤包含具体代码
- ✅ 类型名称一致

**3. Type consistency:**
- ✅ `AgentEvent` 类型在所有任务中一致
- ✅ `AgentStatus` 类型在所有任务中一致
- ✅ `AgentAdapter` 接口前后一致

**Gap identified:** The Rust `SidecarManager` currently uses `std::mem::forget(child)` which is a hack. In production, we should store `Child` handles in the manager. This is acceptable for the initial implementation but should be noted for future improvement.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-06-01-agent-gateway.md`.**

Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
