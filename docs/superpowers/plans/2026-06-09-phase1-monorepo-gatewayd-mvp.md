# Phase 1: Monorepo Reconstruction + Gatewayd MVP

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reconstruct the project into a Rust + Node.js monorepo and build the core `gatewayd` daemon with OpenAI/Anthropic compatible HTTP API, basic audit logging, and the `deepharness exec` CLI wrapper.

**Architecture:** A single Rust binary (`gatewayd`) exposes local HTTP endpoints compatible with OpenAI/Anthropic APIs, proxies requests to real LLM backends, and logs all traffic to SQLite. A CLI binary wraps coding agent commands, injecting environment variables to route traffic through `gatewayd`.

**Tech Stack:** Rust 1.85+, tokio, axum, reqwest, rusqlite, serde, clap, notify-rust

---

## File Structure

```
depharness/
├── Cargo.toml                            # Rust workspace root
├── pnpm-workspace.yaml                   # Node workspace (保留现有)
├── package.json                          # Root package scripts
│
├── crates/
│   ├── dh-core/
│   │   ├── src/
│   │   │   ├── models/
│   │   │   │   ├── request.rs
│   │   │   │   ├── response.rs
│   │   │   │   ├── session.rs
│   │   │   │   └── audit.rs
│   │   │   ├── mcp/
│   │   │   │   ├── client.rs
│   │   │   │   ├── transport.rs
│   │   │   │   ├── codec.rs
│   │   │   │   └── types.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   ├── dh-platform/
│   │   ├── src/
│   │   │   ├── ipc.rs
│   │   │   ├── notify.rs
│   │   │   ├── fs.rs
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   │
│   └── dh-db/
│       ├── src/
│       │   ├── schema.rs
│       │   ├── connection.rs
│       │   └── lib.rs
│       └── Cargo.toml
│
├── apps/
│   ├── gatewayd/
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── server/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── api.rs          # OpenAI/Anthropic compatible endpoints
│   │   │   │   └── admin.rs        # Health check + basic admin
│   │   │   ├── gateway/
│   │   │   │   ├── router.rs       # Request routing to real LLM
│   │   │   │   └── transformer.rs  # OpenAI <-> Anthropic format conversion
│   │   │   ├── audit/
│   │   │   │   ├── logger.rs
│   │   │   │   └── storage.rs
│   │   │   └── db/
│   │   │       └── mod.rs
│   │   └── Cargo.toml
│   │
│   └── cli/
│       ├── src/
│       │   ├── main.rs
│       │   ├── commands/
│       │   │   ├── mod.rs
│       │   │   ├── exec.rs         # deepharness exec <agent>
│       │   │   └── gatewayd.rs     # start/stop/status
│       │   └── wrapper/
│       │       ├── mod.rs
│       │       ├── env_injector.rs
│       │       └── process_manager.rs
│       └── Cargo.toml
│
└── apps/desktop/                       # 现有代码迁移目标
    └── (保留现有 src/ 和 src-tauri/，暂不移动)
```

**Key principle:** We do NOT move the existing desktop code in Phase 1. We only create the new monorepo structure around it. The desktop stays at the project root for now and will be migrated in a later phase.

---

## Task 1: Create Rust Workspace Root

**Files:**
- Create: `Cargo.toml`
- Create: `.cargo/config.toml`

- [ ] **Step 1: Write workspace-level Cargo.toml**

```toml
[workspace]
members = [
    "crates/dh-core",
    "crates/dh-platform",
    "crates/dh-db",
    "apps/gatewayd",
    "apps/cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["DeepHarness Team"]
license = "MIT"
repository = "https://github.com/deepharness/deepharness"

[workspace.dependencies]
tokio = { version = "1.43", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
axum = "0.8"
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "trace"] }
reqwest = { version = "0.12", features = ["json", "stream", "rustls-tls"] }
rusqlite = { version = "0.34", features = ["bundled", "chrono", "uuid"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.16", features = ["v4", "serde"] }
thiserror = "2.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
clap = { version = "4.5", features = ["derive"] }
notify-rust = "4.11"
```

- [ ] **Step 2: Create .cargo/config.toml for consistent builds**

```toml
[build]
target-dir = "target"

[env]
RUST_BACKTRACE = "1"
```

- [ ] **Step 3: Verify workspace resolves**

Run: `cargo check --workspace`
Expected: Compiles empty workspace successfully (no errors, just "unresolved import" warnings for empty crates)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml .cargo/config.toml
git commit -m "chore: initialize Rust workspace for monorepo"
```

---

## Task 2: Create dh-core Crate (Core Models + MCP Protocol Migration)

**Files:**
- Create: `crates/dh-core/Cargo.toml`
- Create: `crates/dh-core/src/lib.rs`
- Create: `crates/dh-core/src/models/request.rs`
- Create: `crates/dh-core/src/models/response.rs`
- Create: `crates/dh-core/src/models/session.rs`
- Create: `crates/dh-core/src/models/audit.rs`
- Create: `crates/dh-core/src/models/mod.rs`
- Create: `crates/dh-core/src/mcp/client.rs`
- Create: `crates/dh-core/src/mcp/transport.rs`
- Create: `crates/dh-core/src/mcp/codec.rs`
- Create: `crates/dh-core/src/mcp/types.rs`
- Create: `crates/dh-core/src/mcp/mod.rs`

- [ ] **Step 1: Write dh-core Cargo.toml**

```toml
[package]
name = "dh-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
tokio.workspace = true
tracing.workspace = true
```

- [ ] **Step 2: Write models/request.rs — Unified LLM request format**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub id: String,
    pub session_id: String,
    pub provider: Provider,
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
    pub metadata: RequestMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestMetadata {
    pub agent_type: Option<String>,
    pub workspace: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl UnifiedRequest {
    pub fn new(provider: Provider, model: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id: Uuid::new_v4().to_string(),
            provider,
            model,
            messages: Vec::new(),
            temperature: None,
            max_tokens: None,
            stream: true,
            metadata: RequestMetadata {
                timestamp: chrono::Utc::now(),
                ..Default::default()
            },
        }
    }

    pub fn prepend_system_message(&mut self, content: String) {
        self.messages.insert(0, Message {
            role: Role::System,
            content,
        });
    }
}
```

- [ ] **Step 3: Write models/response.rs — Unified LLM response format**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedResponse {
    pub id: String,
    pub session_id: String,
    pub model: String,
    pub content: String,
    pub usage: TokenUsage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub id: String,
    pub session_id: String,
    pub delta: String,
    pub finish_reason: Option<String>,
}
```

- [ ] **Step 4: Write models/session.rs — Session tracking**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub agent_type: String,
    pub model: String,
    pub workspace: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Idle,
    Closed,
}

impl Session {
    pub fn new(agent_type: String, model: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            agent_type,
            model,
            workspace: None,
            started_at: now,
            last_active_at: now,
            status: SessionStatus::Active,
        }
    }
}
```

- [ ] **Step 5: Write models/audit.rs — Audit log entry**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub session_id: String,
    pub request_id: String,
    pub direction: Direction,
    pub provider: String,
    pub model: String,
    pub payload_size_bytes: usize,
    pub token_usage: Option<crate::TokenUsage>,
    pub timestamp: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Request,
    Response,
}

impl AuditLogEntry {
    pub fn new(
        session_id: String,
        request_id: String,
        direction: Direction,
        provider: String,
        model: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            session_id,
            request_id,
            direction,
            provider,
            model,
            payload_size_bytes: 0,
            token_usage: None,
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }
}
```

- [ ] **Step 6: Write models/mod.rs**

```rust
pub mod audit;
pub mod request;
pub mod response;
pub mod session;

pub use audit::{AuditLogEntry, Direction};
pub use request::{Message, Provider, RequestMetadata, Role, UnifiedRequest};
pub use response::{StreamChunk, TokenUsage, UnifiedResponse};
pub use session::{Session, SessionStatus};
```

- [ ] **Step 7: Migrate MCP protocol from existing agent-core**

Read existing files and migrate:
- `src-tauri/crates/agent-core/src/mcp/client.rs` → `crates/dh-core/src/mcp/client.rs`
- `src-tauri/crates/agent-core/src/mcp/transport.rs` → `crates/dh-core/src/mcp/transport.rs`
- `src-tauri/crates/agent-core/src/mcp/codec.rs` → `crates/dh-core/src/mcp/codec.rs`
- `src-tauri/crates/agent-core/src/mcp/types.rs` → `crates/dh-core/src/mcp/types.rs`

Preserve existing functionality. Adjust imports to use `dh_core::` instead of `agent_core::`.

- [ ] **Step 8: Write mcp/mod.rs**

```rust
pub mod client;
pub mod codec;
pub mod transport;
pub mod types;
```

- [ ] **Step 9: Write dh-core/src/lib.rs**

```rust
pub mod mcp;
pub mod models;

pub use models::*;
```

- [ ] **Step 10: Verify dh-core compiles**

Run: `cargo check -p dh-core`
Expected: Clean compile with 0 errors

- [ ] **Step 11: Commit**

```bash
git add crates/dh-core/
git commit -m "feat(dh-core): add core models and migrate MCP protocol

- UnifiedRequest/Response/Session models
- AuditLogEntry for traffic logging
- Migrate MCP protocol from agent-core crate"
```

---

## Task 3: Create dh-platform Crate (Cross-Platform Abstractions)

**Files:**
- Create: `crates/dh-platform/Cargo.toml`
- Create: `crates/dh-platform/src/lib.rs`
- Create: `crates/dh-platform/src/ipc.rs`
- Create: `crates/dh-platform/src/notify.rs`
- Create: `crates/dh-platform/src/fs.rs`

- [ ] **Step 1: Write dh-platform Cargo.toml**

```toml
[package]
name = "dh-platform"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
notify-rust = { version = "4.11", optional = true }

[target.'cfg(unix)'.dependencies]
tokio = { workspace = true, features = ["net"] }

[target.'cfg(windows)'.dependencies]
tokio = { workspace = true, features = ["net"] }
```

- [ ] **Step 2: Write fs.rs — Data directory and lock file**

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FsError {
    #[error("Failed to get data directory: {0}")]
    DataDir(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn data_dir() -> Result<PathBuf, FsError> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| FsError::DataDir("No home dir".into()))?;
        Ok(home.join("Library/Application Support/DeepHarness"))
    }
    
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().ok_or_else(|| FsError::DataDir("No home dir".into()))?;
        Ok(home.join(".local/share/deepharness"))
    }
    
    #[cfg(target_os = "windows")]
    {
        let local_app_data = dirs::data_local_dir()
            .ok_or_else(|| FsError::DataDir("No local app data dir".into()))?;
        Ok(local_app_data.join("DeepHarness"))
    }
}

pub fn ensure_data_dir() -> Result<PathBuf, FsError> {
    let dir = data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn lock_file_path() -> Result<PathBuf, FsError> {
    Ok(data_dir()?.join("gatewayd.lock"))
}

pub fn write_lock_file(pid: u32) -> Result<(), FsError> {
    let path = lock_file_path()?;
    std::fs::write(&path, pid.to_string())?;
    Ok(())
}

pub fn read_lock_file() -> Result<Option<u32>, FsError> {
    let path = lock_file_path()?;
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content.trim().parse().ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn remove_lock_file() -> Result<(), FsError> {
    let path = lock_file_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
```

- [ ] **Step 3: Add dirs dependency to workspace Cargo.toml**

Modify `Cargo.toml` workspace dependencies, add:
```toml
dirs = "6.0"
```

And add to dh-platform Cargo.toml:
```toml
dirs.workspace = true
```

- [ ] **Step 4: Write ipc.rs — Platform-specific IPC abstraction**

```rust
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IpcError {
    #[error("Platform not supported")]
    Unsupported,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub enum IpcEndpoint {
    #[cfg(unix)]
    UnixSocket(PathBuf),
    #[cfg(windows)]
    NamedPipe(String),
}

impl IpcEndpoint {
    pub fn default_gatewayd() -> Result<Self, IpcError> {
        #[cfg(unix)]
        {
            let dir = super::fs::data_dir()?;
            Ok(IpcEndpoint::UnixSocket(dir.join("gatewayd.sock")))
        }
        
        #[cfg(windows)]
        {
            Ok(IpcEndpoint::NamedPipe(r"\\.\pipe\deepharness-gatewayd".to_string()))
        }
    }
}

#[cfg(unix)]
pub mod unix {
    use tokio::net::UnixListener;
    
    pub async fn bind_socket(path: &std::path::Path) -> Result<UnixListener, super::IpcError> {
        // Remove stale socket file
        let _ = tokio::fs::remove_file(path).await;
        Ok(UnixListener::bind(path)?)
    }
}
```

- [ ] **Step 5: Write notify.rs — System notifications**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotifyError {
    #[error("Notification failed: {0}")]
    Failed(String),
}

pub fn send_notification(title: &str, body: &str) -> Result<(), NotifyError> {
    #[cfg(feature = "notify-rust")]
    {
        notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .show()
            .map_err(|e| NotifyError::Failed(e.to_string()))?;
    }
    
    #[cfg(not(feature = "notify-rust"))]
    {
        tracing::info!("NOTIFICATION: {} - {}", title, body);
    }
    
    Ok(())
}
```

- [ ] **Step 6: Write dh-platform/src/lib.rs**

```rust
pub mod fs;
pub mod ipc;
pub mod notify;
```

- [ ] **Step 7: Verify dh-platform compiles**

Run: `cargo check -p dh-platform`
Expected: Clean compile

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/dh-platform/
git commit -m "feat(dh-platform): add cross-platform abstractions

- Data directory management (macOS/Linux/Windows)
- Lock file helpers
- IPC endpoint abstraction (Unix socket / Named pipe)
- System notification wrapper"
```

---

## Task 4: Create dh-db Crate (SQLite Schema and Connection)

**Files:**
- Create: `crates/dh-db/Cargo.toml`
- Create: `crates/dh-db/src/lib.rs`
- Create: `crates/dh-db/src/schema.rs`
- Create: `crates/dh-db/src/connection.rs`

- [ ] **Step 1: Write dh-db Cargo.toml**

```toml
[package]
name = "dh-db"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
rusqlite.workspace = true
chrono.workspace = true
uuid.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
dh-core = { path = "../dh-core" }
```

- [ ] **Step 2: Write schema.rs — SQLite table definitions**

```rust
pub const CREATE_AUDIT_LOGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    request_id TEXT NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('request', 'response')),
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    payload_size_bytes INTEGER NOT NULL DEFAULT 0,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    total_tokens INTEGER,
    timestamp TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_audit_session ON audit_logs(session_id);
CREATE INDEX IF NOT EXISTS idx_audit_request ON audit_logs(request_id);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_logs(timestamp);
"#;

pub const CREATE_SESSIONS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    agent_type TEXT NOT NULL,
    model TEXT NOT NULL,
    workspace TEXT,
    started_at TEXT NOT NULL,
    last_active_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active', 'idle', 'closed'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);
"#;

pub const CREATE_CONFIGS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS configs (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

pub const ALL_MIGRATIONS: &[&str] = &[
    CREATE_AUDIT_LOGS_TABLE,
    CREATE_SESSIONS_TABLE,
    CREATE_CONFIGS_TABLE,
];
```

- [ ] **Step 3: Write connection.rs — Database connection manager**

```rust
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(String),
}

pub struct DbManager {
    conn: Connection,
}

impl DbManager {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        let mut manager = Self { conn };
        manager.migrate()?;
        Ok(manager)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let mut manager = Self { conn };
        manager.migrate()?;
        Ok(manager)
    }

    fn migrate(&mut self) -> Result<(), DbError> {
        for migration in crate::schema::ALL_MIGRATIONS {
            self.conn.execute_batch(migration).map_err(|e| {
                DbError::Migration(format!("Failed to run migration: {e}"))
            })?;
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
```

- [ ] **Step 4: Write dh-db/src/lib.rs**

```rust
pub mod connection;
pub mod schema;

pub use connection::{DbError, DbManager};
```

- [ ] **Step 5: Verify dh-db compiles**

Run: `cargo check -p dh-db`
Expected: Clean compile

- [ ] **Step 6: Commit**

```bash
git add crates/dh-db/
git commit -m "feat(dh-db): add SQLite schema and connection manager

- audit_logs table with indexes
- sessions table
- configs key-value store
- Migration system"
```

---

## Task 5: Create gatewayd App (Core Daemon)

**Files:**
- Create: `apps/gatewayd/Cargo.toml`
- Create: `apps/gatewayd/src/main.rs`
- Create: `apps/gatewayd/src/server/mod.rs`
- Create: `apps/gatewayd/src/server/api.rs`
- Create: `apps/gatewayd/src/server/admin.rs`
- Create: `apps/gatewayd/src/gateway/mod.rs`
- Create: `apps/gatewayd/src/gateway/router.rs`
- Create: `apps/gatewayd/src/gateway/transformer.rs`
- Create: `apps/gatewayd/src/audit/mod.rs`
- Create: `apps/gatewayd/src/audit/logger.rs`
- Create: `apps/gatewayd/src/audit/storage.rs`
- Create: `apps/gatewayd/src/db/mod.rs`

- [ ] **Step 1: Write gatewayd Cargo.toml**

```toml
[package]
name = "gatewayd"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "gatewayd"
path = "src/main.rs"

[dependencies]
dh-core = { path = "../../crates/dh-core" }
dh-platform = { path = "../../crates/dh-platform" }
dh-db = { path = "../../crates/dh-db" }

tokio.workspace = true
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
chrono.workspace = true
uuid.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
clap.workspace = true
```

- [ ] **Step 2: Write gatewayd/src/db/mod.rs — App-specific DB helpers**

```rust
use dh_db::DbManager;
use std::path::Path;

pub fn init_db<P: AsRef<Path>>(path: P) -> Result<DbManager, anyhow::Error> {
    let manager = DbManager::open(path)?;
    Ok(manager)
}
```

- [ ] **Step 3: Write audit/storage.rs — Audit log storage**

```rust
use dh_core::AuditLogEntry;
use dh_db::DbManager;
use rusqlite::params;
use anyhow::Result;

pub struct AuditStorage {
    db: DbManager,
}

impl AuditStorage {
    pub fn new(db: DbManager) -> Self {
        Self { db }
    }

    pub fn insert(&mut self, entry: &AuditLogEntry) -> Result<()> {
        let conn = self.db.conn_mut();
        conn.execute(
            r#"
            INSERT INTO audit_logs (
                id, session_id, request_id, direction, provider, model,
                payload_size_bytes, prompt_tokens, completion_tokens, total_tokens,
                timestamp, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                &entry.id,
                &entry.session_id,
                &entry.request_id,
                format!("{:?}", entry.direction).to_lowercase(),
                &entry.provider,
                &entry.model,
                entry.payload_size_bytes as i64,
                entry.token_usage.as_ref().map(|u| u.prompt_tokens as i64),
                entry.token_usage.as_ref().map(|u| u.completion_tokens as i64),
                entry.token_usage.as_ref().map(|u| u.total_tokens as i64),
                entry.timestamp.to_rfc3339(),
                entry.metadata.to_string(),
            ],
        )?;
        Ok(())
    }
}
```

- [ ] **Step 4: Write audit/logger.rs — Async audit logger**

```rust
use dh_core::AuditLogEntry;
use tokio::sync::mpsc;
use tracing::{error, info};

pub struct AuditLogger {
    sender: mpsc::UnboundedSender<AuditLogEntry>,
}

impl AuditLogger {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<AuditLogEntry>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    pub fn log(&self, entry: AuditLogEntry) {
        if let Err(e) = self.sender.send(entry) {
            error!("Failed to send audit log: {}", e);
        }
    }
}

pub async fn run_storage_worker(
    mut receiver: mpsc::UnboundedReceiver<AuditLogEntry>,
    mut storage: super::storage::AuditStorage,
) {
    info!("Audit storage worker started");
    while let Some(entry) = receiver.recv().await {
        if let Err(e) = storage.insert(&entry) {
            error!("Failed to persist audit log: {}", e);
        }
    }
    info!("Audit storage worker stopped");
}
```

- [ ] **Step 5: Write audit/mod.rs**

```rust
pub mod logger;
pub mod storage;

pub use logger::{run_storage_worker, AuditLogger};
pub use storage::AuditStorage;
```

- [ ] **Step 6: Write gateway/transformer.rs — OpenAI <-> Anthropic format conversion**

```rust
use dh_core::{Message, Provider, Role, UnifiedRequest, UnifiedResponse, StreamChunk, TokenUsage};
use serde_json::Value;

pub fn openai_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::OpenAi,
        body.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o").to_string(),
    );
    
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages.iter().filter_map(|m| {
            let role = m.get("role")?.as_str()?;
            let content = m.get("content")?.as_str()?;
            let role = match role {
                "system" => Role::System,
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => Role::User,
            };
            Some(Message { role, content: content.to_string() })
        }).collect();
    }
    
    req.temperature = body.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32);
    req.max_tokens = body.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32);
    req.stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(true);
    
    req
}

pub fn anthropic_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::Anthropic,
        body.get("model").and_then(|v| v.as_str()).unwrap_or("claude-sonnet-4").to_string(),
    );
    
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages.iter().filter_map(|m| {
            let role = m.get("role")?.as_str()?;
            let content = m.get("content")?.as_str()?;
            let role = match role {
                "user" => Role::User,
                "assistant" => Role::Assistant,
                _ => Role::User,
            };
            Some(Message { role, content: content.to_string() })
        }).collect();
    }
    
    if let Some(system) = body.get("system").and_then(|v| v.as_str()) {
        req.prepend_system_message(system.to_string());
    }
    
    req.max_tokens = body.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32);
    req.temperature = body.get("temperature").and_then(|v| v.as_f64()).map(|v| v as f32);
    
    req
}

pub fn unified_to_openai_response(resp: &UnifiedResponse) -> Value {
    serde_json::json!({
        "id": resp.id,
        "object": "chat.completion",
        "model": resp.model,
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": resp.content
            },
            "finish_reason": resp.finish_reason.as_deref().unwrap_or("stop")
        }],
        "usage": {
            "prompt_tokens": resp.usage.prompt_tokens,
            "completion_tokens": resp.usage.completion_tokens,
            "total_tokens": resp.usage.total_tokens
        }
    })
}

pub fn unified_to_openai_stream_chunk(chunk: &StreamChunk) -> String {
    let data = serde_json::json!({
        "id": chunk.id,
        "object": "chat.completion.chunk",
        "choices": [{
            "index": 0,
            "delta": {
                "content": chunk.delta
            },
            "finish_reason": chunk.finish_reason
        }]
    });
    format!("data: {}\n\n", data)
}
```

- [ ] **Step 7: Write gateway/router.rs — Request routing to real LLM**

```rust
use axum::body::Body;
use axum::response::Response;
use reqwest::Client;
use std::sync::Arc;
use tracing::{error, info};

pub struct GatewayRouter {
    client: Client,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
}

impl GatewayRouter {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        }
    }

    pub async fn forward_openai(&self, body: String) -> Result<Response, anyhow::Error> {
        let api_key = self.openai_api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
        
        info!("Forwarding request to OpenAI API");
        
        let resp = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;
        
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.bytes().await?;
        
        let mut response = Response::builder()
            .status(status)
            .body(Body::from(bytes))?;
        
        *response.headers_mut() = headers;
        Ok(response)
    }

    pub async fn forward_anthropic(&self, body: String) -> Result<Response, anyhow::Error> {
        let api_key = self.anthropic_api_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
        
        info!("Forwarding request to Anthropic API");
        
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;
        
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.bytes().await?;
        
        let mut response = Response::builder()
            .status(status)
            .body(Body::from(bytes))?;
        
        *response.headers_mut() = headers;
        Ok(response)
    }
}
```

- [ ] **Step 8: Write gateway/mod.rs**

```rust
pub mod router;
pub mod transformer;

pub use router::GatewayRouter;
```

- [ ] **Step 9: Write server/api.rs — OpenAI/Anthropic compatible endpoints**

```rust
use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use std::sync::Arc;
use tracing::{error, info};

use crate::gateway::{transformer, GatewayRouter};
use crate::audit::AuditLogger;
use dh_core::{AuditLogEntry, Direction, Provider};

#[derive(Clone)]
pub struct ApiState {
    pub router: Arc<GatewayRouter>,
    pub audit: Arc<AuditLogger>,
}

pub async fn openai_chat_completions(
    State(state): State<ApiState>,
    body: Bytes,
) -> Response {
    info!("Received OpenAI chat completions request");
    
    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };
    
    let unified = transformer::openai_to_unified(body_json.clone());
    
    // Log request
    let mut entry = AuditLogEntry::new(
        unified.session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        "openai".to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = body.len();
    state.audit.log(entry);
    
    // Forward to real API
    match state.router.forward_openai(body_str.to_string()).await {
        Ok(response) => {
            info!("Successfully forwarded request to OpenAI");
            response
        }
        Err(e) => {
            error!("Failed to forward request: {}", e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}

pub async fn anthropic_messages(
    State(state): State<ApiState>,
    body: Bytes,
) -> Response {
    info!("Received Anthropic messages request");
    
    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };
    
    let unified = transformer::anthropic_to_unified(body_json.clone());
    
    let mut entry = AuditLogEntry::new(
        unified.session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        "anthropic".to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = body.len();
    state.audit.log(entry);
    
    match state.router.forward_anthropic(body_str.to_string()).await {
        Ok(response) => {
            info!("Successfully forwarded request to Anthropic");
            response
        }
        Err(e) => {
            error!("Failed to forward request: {}", e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}
```

- [ ] **Step 10: Write server/admin.rs — Health check**

```rust
use axum::{extract::State, response::Json};
use serde_json::json;
use std::sync::Arc;

use super::api::ApiState;

pub async fn health_check() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "service": "gatewayd",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
```

- [ ] **Step 11: Write server/mod.rs**

```rust
pub mod admin;
pub mod api;

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use api::ApiState;

pub fn create_api_router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(api::openai_chat_completions))
        .route("/v1/messages", post(api::anthropic_messages))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub fn create_admin_router(state: ApiState) -> Router {
    Router::new()
        .route("/health", get(admin::health_check))
        .with_state(state)
}
```

- [ ] **Step 12: Write gatewayd/src/main.rs**

```rust
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

mod audit;
mod db;
mod gateway;
mod server;

use audit::{run_storage_worker, AuditLogger, AuditStorage};
use gateway::GatewayRouter;
use server::ApiState;

#[derive(Parser, Debug)]
#[command(name = "gatewayd")]
#[command(about = "DeepHarness LLM Gateway Daemon")]
struct Args {
    #[arg(long, default_value = "2345")]
    port: u16,
    
    #[arg(long, default_value = "2346")]
    admin_port: u16,
    
    #[arg(long)]
    daemon: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    
    info!("Starting gatewayd on port {}, admin on port {}", args.port, args.admin_port);
    
    // Initialize data directory and database
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");
    let db = db::init_db(&db_path)?;
    
    // Setup audit logging
    let (audit_logger, audit_receiver) = AuditLogger::new();
    let audit_storage = AuditStorage::new(db);
    tokio::spawn(run_storage_worker(audit_receiver, audit_storage));
    
    // Setup gateway router
    let gateway_router = Arc::new(GatewayRouter::new());
    
    // Create API state
    let api_state = ApiState {
        router: gateway_router,
        audit: Arc::new(audit_logger),
    };
    
    // Build routers
    let api_router = server::create_api_router(api_state.clone());
    let admin_router = server::create_admin_router(api_state);
    
    // Combine routers
    let app = Router::new()
        .nest("/", api_router)
        .nest("/admin", admin_router);
    
    // Bind to ports
    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let admin_addr = SocketAddr::from(([127, 0, 0, 1], args.admin_port));
    
    // Start admin server
    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await?;
    info!("Admin API listening on http://{}", admin_addr);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(admin_listener, admin_router).await {
            warn!("Admin server error: {}", e);
        }
    });
    
    // Start main API server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API server listening on http://{}", addr);
    info!("OpenAI compatible endpoint: http://{}/v1/chat/completions", addr);
    info!("Anthropic compatible endpoint: http://{}/v1/messages", addr);
    
    // Write lock file
    let pid = std::process::id();
    dh_platform::fs::write_lock_file(pid)?;
    info!("Lock file written with PID: {}", pid);
    
    axum::serve(listener, app).await?;
    
    // Cleanup
    let _ = dh_platform::fs::remove_lock_file();
    
    Ok(())
}
```

- [ ] **Step 13: Verify gatewayd compiles**

Run: `cargo check -p gatewayd`
Expected: Clean compile with 0 errors

- [ ] **Step 14: Test gatewayd health endpoint**

Run: `cargo run -p gatewayd -- --port 2345 --admin-port 2346`
In another terminal: `curl http://127.0.0.1:2346/health`
Expected: `{"status":"ok","service":"gatewayd","version":"0.1.0"}`

- [ ] **Step 15: Commit**

```bash
git add apps/gatewayd/
git commit -m "feat(gatewayd): add core daemon with OpenAI/Anthropic compatible API

- Axum HTTP server on two ports (work + admin)
- Request routing to real OpenAI/Anthropic APIs
- Audit logging to SQLite with async worker
- Health check endpoint
- Lock file management"
```

---

## Task 6: Create CLI App (Wrapper + Daemon Management)

**Files:**
- Create: `apps/cli/Cargo.toml`
- Create: `apps/cli/src/main.rs`
- Create: `apps/cli/src/commands/mod.rs`
- Create: `apps/cli/src/commands/exec.rs`
- Create: `apps/cli/src/commands/gatewayd.rs`
- Create: `apps/cli/src/wrapper/mod.rs`
- Create: `apps/cli/src/wrapper/env_injector.rs`
- Create: `apps/cli/src/wrapper/process_manager.rs`

- [ ] **Step 1: Write cli Cargo.toml**

```toml
[package]
name = "deepharness-cli"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "deepharness"
path = "src/main.rs"

[dependencies]
dh-platform = { path = "../../crates/dh-platform" }

tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
reqwest.workspace = true
thiserror.workspace = true
anyhow.workspace = true
tracing.workspace = true
clap.workspace = true
```

- [ ] **Step 2: Write wrapper/env_injector.rs**

```rust
use std::collections::HashMap;

pub fn build_env_map(api_port: u16, proxy_port: u16) -> HashMap<String, String> {
    let mut env = HashMap::new();
    
    // LLM API routing
    env.insert("OPENAI_BASE_URL".to_string(), format!("http://127.0.0.1:{}/v1", api_port));
    env.insert("ANTHROPIC_BASE_URL".to_string(), format!("http://127.0.0.1:{}/v1", api_port));
    
    // Proxy for CLI skill remote requests
    env.insert("HTTP_PROXY".to_string(), format!("http://127.0.0.1:{}", proxy_port));
    env.insert("NO_PROXY".to_string(), "localhost,127.0.0.1,::1".to_string());
    
    // DeepHarness specific
    env.insert("DEEPHARNESS_GATEWAYD_PORT".to_string(), api_port.to_string());
    env.insert("DEEPHARNESS_SESSION_ID".to_string(), uuid::Uuid::new_v4().to_string());
    
    env
}
```

- [ ] **Step 3: Write wrapper/process_manager.rs**

```rust
use std::process::{Command, Stdio};
use tracing::{error, info};

pub struct ProcessManager;

impl ProcessManager {
    pub fn spawn_agent(
        command: &str,
        args: &[String],
        env_vars: &std::collections::HashMap<String, String>,
    ) -> Result<std::process::Child, anyhow::Error> {
        info!("Spawning agent: {} {:?}", command, args);
        
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        
        for (key, value) in env_vars {
            cmd.env(key, value);
        }
        
        let child = cmd.spawn()?;
        info!("Agent spawned with PID: {}", child.id());
        
        Ok(child)
    }
}
```

- [ ] **Step 4: Write wrapper/mod.rs**

```rust
pub mod env_injector;
pub mod process_manager;

pub use env_injector::build_env_map;
pub use process_manager::ProcessManager;
```

- [ ] **Step 5: Write commands/exec.rs**

```rust
use clap::Args;
use tracing::{error, info};

use crate::wrapper::{build_env_map, ProcessManager};

#[derive(Args, Debug)]
pub struct ExecArgs {
    /// The coding agent to execute (e.g., claude, opencode, aider)
    pub agent: String,
    
    /// Additional arguments to pass to the agent
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub agent_args: Vec<String>,
}

pub async fn run(args: ExecArgs) -> Result<(), anyhow::Error> {
    info!("Executing agent: {} with args: {:?}", args.agent, args.agent_args);
    
    // Ensure gatewayd is running
    let gatewayd_info = match check_gatewayd().await {
        Some(info) => info,
        None => {
            info!("gatewayd not running, starting it...");
            start_gatewayd().await?;
            // Wait a bit for it to start
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            check_gatewayd().await.ok_or_else(|| anyhow::anyhow!("Failed to start gatewayd"))?
        }
    };
    
    info!("Using gatewayd at port {}", gatewayd_info.port);
    
    // Build environment variables
    let env_vars = build_env_map(gatewayd_info.port, gatewayd_info.port + 2); // proxy port = api_port + 2
    
    // Spawn agent
    let mut child = ProcessManager::spawn_agent(&args.agent, &args.agent_args, &env_vars)?;
    
    // Wait for agent to complete
    let status = child.wait()?;
    
    if status.success() {
        info!("Agent exited successfully");
    } else {
        error!("Agent exited with status: {:?}", status.code());
    }
    
    Ok(())
}

#[derive(Debug)]
struct GatewaydInfo {
    port: u16,
}

async fn check_gatewayd() -> Option<GatewaydInfo> {
    // Check lock file
    match dh_platform::fs::read_lock_file() {
        Ok(Some(pid)) => {
            // Check if process is actually running and health endpoint responds
            let client = reqwest::Client::new();
            for port in [2345u16, 2346, 2347, 2348, 2349] {
                let url = format!("http://127.0.0.1:{}/health", port + 1); // admin port = api_port + 1
                if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(2)).send().await {
                    if resp.status().is_success() {
                        return Some(GatewaydInfo { port });
                    }
                }
            }
            None
        }
        _ => None,
    }
}

async fn start_gatewayd() -> Result<(), anyhow::Error> {
    info!("Starting gatewayd...");
    
    let mut cmd = std::process::Command::new("gatewayd");
    cmd.arg("--daemon");
    
    // Try to find gatewayd in PATH or in target directory
    if let Ok(exe_path) = std::env::current_exe() {
        let possible_gatewayd = exe_path.parent()
            .map(|p| p.join("gatewayd"))
            .or_else(|| exe_path.parent().and_then(|p| p.parent()).map(|p| p.join("gatewayd")));
        
        if let Some(path) = possible_gatewayd {
            if path.exists() {
                cmd = std::process::Command::new(path);
                cmd.arg("--daemon");
            }
        }
    }
    
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    
    info!("gatewayd started with PID: {}", child.id());
    
    // Detach - don't wait
    let _ = child.try_wait();
    
    Ok(())
}
```

- [ ] **Step 6: Write commands/gatewayd.rs**

```rust
use clap::Subcommand;
use tracing::{error, info};

#[derive(Subcommand, Debug)]
pub enum GatewaydCommands {
    /// Start the gatewayd daemon
    Start {
        #[arg(long)]
        daemon: bool,
    },
    /// Stop the gatewayd daemon
    Stop,
    /// Check gatewayd status
    Status,
}

pub async fn run(command: GatewaydCommands) -> Result<(), anyhow::Error> {
    match command {
        GatewaydCommands::Start { daemon } => {
            info!("Starting gatewayd...");
            
            if check_running().await {
                println!("gatewayd is already running");
                return Ok(());
            }
            
            let mut cmd = std::process::Command::new("gatewayd");
            if daemon {
                cmd.arg("--daemon");
            }
            
            let mut child = cmd.spawn()?;
            info!("gatewayd started with PID: {}", child.id());
            
            if !daemon {
                // If not daemon mode, wait for it
                let status = child.wait()?;
                if !status.success() {
                    error!("gatewayd exited with status: {:?}", status.code());
                }
            }
            
            println!("gatewayd started");
        }
        GatewaydCommands::Stop => {
            info!("Stopping gatewayd...");
            
            match dh_platform::fs::read_lock_file()? {
                Some(pid) => {
                    #[cfg(unix)]
                    {
                        unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                        }
                    }
                    
                    #[cfg(windows)]
                    {
                        // Use taskkill or Windows API
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F"])
                            .output();
                    }
                    
                    dh_platform::fs::remove_lock_file()?;
                    println!("gatewayd stopped (PID: {})", pid);
                }
                None => {
                    println!("gatewayd is not running");
                }
            }
        }
        GatewaydCommands::Status => {
            if check_running().await {
                let client = reqwest::Client::new();
                for port in [2346u16, 2347, 2348, 2349, 2350] {
                    let url = format!("http://127.0.0.1:{}/health", port);
                    if let Ok(resp) = client.get(&url).timeout(std::time::Duration::from_secs(1)).send().await {
                        if resp.status().is_success() {
                            let body: serde_json::Value = resp.json().await?;
                            println!("gatewayd is running");
                            println!("  Admin port: {}", port);
                            println!("  Version: {}", body.get("version").and_then(|v| v.as_str()).unwrap_or("unknown"));
                            break;
                        }
                    }
                }
            } else {
                println!("gatewayd is not running");
            }
        }
    }
    
    Ok(())
}

async fn check_running() -> bool {
    dh_platform::fs::read_lock_file().ok().flatten().is_some()
}
```

- [ ] **Step 7: Write commands/mod.rs**

```rust
pub mod exec;
pub mod gatewayd;
```

- [ ] **Step 8: Write cli/src/main.rs**

```rust
use clap::{Parser, Subcommand};
use tracing::info;

mod commands;
mod wrapper;

#[derive(Parser, Debug)]
#[command(name = "deepharness")]
#[command(about = "DeepHarness CLI - LLM Gateway management and agent wrapper")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Execute a coding agent with DeepHarness gateway integration
    Exec(commands::exec::ExecArgs),
    
    /// Manage the gatewayd daemon
    #[command(subcommand)]
    Gatewayd(commands::gatewayd::GatewaydCommands),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Exec(args) => {
            commands::exec::run(args).await?;
        }
        Commands::Gatewayd(cmd) => {
            commands::gatewayd::run(cmd).await?;
        }
    }
    
    Ok(())
}
```

- [ ] **Step 9: Add libc dependency for unix systems**

Modify `apps/cli/Cargo.toml`, add:
```toml
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

- [ ] **Step 10: Verify CLI compiles**

Run: `cargo check -p deepharness-cli`
Expected: Clean compile with 0 errors

- [ ] **Step 11: Build both binaries**

Run: `cargo build --release -p gatewayd -p deepharness-cli`
Expected: Both binaries built successfully in `target/release/`

- [ ] **Step 12: Integration test**

Terminal 1:
```bash
export OPENAI_API_KEY="sk-test"
./target/release/gatewayd --port 2345 --admin-port 2346
```

Terminal 2:
```bash
curl -X POST http://127.0.0.1:2345/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer test" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello"}],
    "stream": false
  }'
```
Expected: Request forwarded to OpenAI (may fail with invalid key, but should reach OpenAI servers)

Terminal 3:
```bash
./target/release/deepharness gatewayd status
```
Expected: "gatewayd is running"

- [ ] **Step 13: Commit**

```bash
git add apps/cli/
git commit -m "feat(cli): add deepharness CLI with exec wrapper and gatewayd management

- deepharness exec <agent> command with env injection
- Gatewayd auto-detection and auto-start
- gatewayd start/stop/status commands
- Cross-platform process management"
```

---

## Self-Review

**1. Spec coverage:**

| Spec Section | Plan Task | Status |
|-------------|-----------|--------|
| Monorepo structure | Task 1 | ✅ |
| dh-core models | Task 2 | ✅ |
| MCP protocol migration | Task 2 | ✅ |
| dh-platform (IPC, notify, fs) | Task 3 | ✅ |
| dh-db (SQLite schema) | Task 4 | ✅ |
| gatewayd HTTP API | Task 5 | ✅ |
| OpenAI/Anthropic compatible endpoints | Task 5 | ✅ |
| Request routing to real LLM | Task 5 | ✅ |
| Audit logging | Task 5 | ✅ |
| CLI wrapper | Task 6 | ✅ |
| Environment variable injection | Task 6 | ✅ |
| Daemon management commands | Task 6 | ✅ |

**2. Placeholder scan:** No TBD, TODO, or incomplete sections found.

**3. Type consistency:** All types referenced match their definitions (UnifiedRequest, AuditLogEntry, etc.)

---

**Plan complete and saved to `docs/superpowers/plans/2026-06-09-phase1-monorepo-gatewayd-mvp.md`.**

**Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
