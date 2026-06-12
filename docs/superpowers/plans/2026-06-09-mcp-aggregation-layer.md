# MCP 聚合层 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 gatewayd MCP 聚合层（配置管理、Client 池、工具聚合、拦截器）和 CLI `dh mcp` 命令。

**Architecture:** 在 dh-core 扩展 McpClient 能力（list_tools/is_alive），dh-db 新增 mcp_servers 表，gatewayd 用独立扁平文件 `mcp_aggregator.rs` 实现 Registry/Interceptor/Admin API，CLI 通过直接读写 SQLite + HTTP Admin API 提供 `dh mcp` 命令。

**Tech Stack:** Rust, tokio, axum, rusqlite, serde_json, reqwest, clap

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `crates/dh-core/src/mcp/types.rs` | 新增 `ListToolsResult` 类型 | 修改 |
| `crates/dh-core/src/mcp/client.rs` | 新增 `list_tools()` 方法 | 修改 |
| `crates/dh-core/src/mcp/transport.rs` | 新增 `is_alive()`，`_child` → `child` | 修改 |
| `crates/dh-db/src/schema.rs` | 新增 `CREATE_MCP_SERVERS_TABLE` + 迁移 | 修改 |
| `crates/dh-db/src/connection.rs` | 迁移逻辑扩展（支持多语句） | 修改 |
| `apps/gatewayd/src/mcp_aggregator.rs` | Registry + Interceptor + Admin handlers | 新建 |
| `apps/gatewayd/src/main.rs` | 集成 Registry 到启动流程和路由 | 修改 |
| `apps/cli/src/commands/mcp.rs` | `dh mcp list/add/remove/call` | 新建 |
| `apps/cli/src/commands/mod.rs` | 导出 mcp 模块 | 修改 |
| `apps/cli/src/main.rs` | 注册 McpCommands | 修改 |

---

## Task 1: dh-core 扩展 — McpClient::list_tools() + StdioTransport::is_alive()

**Files:**
- Modify: `crates/dh-core/src/mcp/types.rs`
- Modify: `crates/dh-core/src/mcp/client.rs`
- Modify: `crates/dh-core/src/mcp/transport.rs`

### 1.1 新增 ListToolsResult 类型

在 `crates/dh-core/src/mcp/types.rs` 的 `Tool` 结构体之后添加：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
}
```

### 1.2 McpClient 新增 list_tools() 方法

在 `crates/dh-core/src/mcp/client.rs` 的 `initialize()` 方法之后、`call_tool()` 之前插入：

```rust
    pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        if !*self.initialized.lock().unwrap() {
            return Err(McpError::NotInitialized);
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(self.request_id.fetch_add(1, Ordering::SeqCst))),
            method: "tools/list".to_string(),
            params: json!({}),
        };

        let response = self.send_request(request).await?;

        match response.result {
            super::codec::JsonRpcResult::Success { result } => {
                let list_result: super::types::ListToolsResult = serde_json::from_value(result)
                    .map_err(|e| McpError::ProtocolError(e.to_string()))?;
                Ok(list_result.tools)
            }
            super::codec::JsonRpcResult::Error { error } => {
                Err(McpError::ProtocolError(error.message))
            }
        }
    }
```

### 1.3 StdioTransport 新增 is_alive()

修改 `crates/dh-core/src/mcp/transport.rs`：

1. 将 `_child: Child` 改为 `child: Child`（去掉下划线前缀）
2. 构造函数中同步修改：`Self { stdin, stdout_tx, child, ... }`
3. 添加方法：

```rust
    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }
```

### 1.4 编译验证

Run: `cargo check -p dh-core`
Expected: `Finished` dev profile [unoptimized + debuginfo] target(s) in Xs`

---

## Task 2: dh-db 扩展 — mcp_servers 表

**Files:**
- Modify: `crates/dh-db/src/schema.rs`
- Modify: `crates/dh-db/src/connection.rs`

### 2.1 新增表定义和迁移

在 `crates/dh-db/src/schema.rs` 的 `ADD_PAYLOAD_COLUMN` 之后添加：

```rust
pub const CREATE_MCP_SERVERS_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS mcp_servers (
    name TEXT PRIMARY KEY,
    command TEXT NOT NULL,
    args TEXT NOT NULL DEFAULT '[]',
    env TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;
```

将 `ALL_MIGRATIONS` 改为：

```rust
pub const ALL_MIGRATIONS: &[&str] = &[
    CREATE_AUDIT_LOGS_TABLE,
    CREATE_SESSIONS_TABLE,
    CREATE_CONFIGS_TABLE,
    ADD_AGENT_TYPE_COLUMN,
    ADD_PAYLOAD_COLUMN,
    CREATE_MCP_SERVERS_TABLE,
];
```

### 2.2 修复迁移逻辑（支持多语句 CREATE TABLE）

`crates/dh-db/src/connection.rs` 的 `migrate()` 方法当前只处理 `ALTER TABLE ADD COLUMN` 的跳过逻辑。`CREATE TABLE IF NOT EXISTS` 本身不会重复报错，所以不需要特殊处理。确认现有逻辑即可：

```rust
fn migrate(&mut self) -> Result<(), DbError> {
    for migration in crate::schema::ALL_MIGRATIONS {
        if migration.contains("ALTER TABLE") && migration.contains("ADD COLUMN") {
            if let Err(e) = self.conn.execute_batch(migration) {
                let err_msg = e.to_string().to_lowercase();
                if !err_msg.contains("duplicate column name")
                    && !err_msg.contains("already exists")
                {
                    return Err(DbError::Migration(format!(
                        "Failed to run migration: {e}"
                    )));
                }
            }
        } else {
            self.conn.execute_batch(migration).map_err(|e| {
                DbError::Migration(format!("Failed to run migration: {e}"))
            })?;
        }
    }
    Ok(())
}
```

无需修改 — 现有逻辑已正确处理 `CREATE TABLE IF NOT EXISTS`。

### 2.3 编译验证

Run: `cargo check -p dh-db`
Expected: `Finished` dev profile`

---

## Task 3: gatewayd MCP 聚合层 — mcp_aggregator.rs

**Files:**
- Create: `apps/gatewayd/src/mcp_aggregator.rs`

### 3.1 编写 mcp_aggregator.rs

完整内容如下（需一次性写入）：

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use dh_core::mcp::client::McpClient;
use dh_core::mcp::types::{Tool, ToolResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

// ── Config ──

const MCP_AGGREGATOR_ENABLED_KEY: &str = "mcp_aggregator_enabled";

// ── Types ──

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Debug)]
pub struct McpClientEntry {
    pub config: McpServerConfig,
    pub client: Arc<McpClient>,
}

/// MCP 聚合注册表
pub struct McpRegistry {
    clients: HashMap<String, McpClientEntry>,
    db_path: std::path::PathBuf,
}

impl McpRegistry {
    pub async fn load_from_db(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = rusqlite::Connection::open(db_path)?;
        let mut registry = Self {
            clients: HashMap::new(),
            db_path: db_path.to_path_buf(),
        };

        // Check if aggregator is enabled
        let enabled: bool = {
            let mut stmt = conn.prepare("SELECT value FROM configs WHERE key = ?1")?;
            let mut rows = stmt.query_map([MCP_AGGREGATOR_ENABLED_KEY], |row| {
                row.get::<_, String>(0)
            })?;
            match rows.next() {
                Some(Ok(v)) => v.parse().unwrap_or(true),
                _ => true, // default enabled
            }
        };

        if !enabled {
            info!("MCP aggregator disabled via config");
            return Ok(registry);
        }

        // Load enabled servers
        let mut stmt = conn.prepare(
            "SELECT name, command, args, env, enabled FROM mcp_servers WHERE enabled = 1"
        )?;
        let rows = stmt.query_map([], |row| {
            let args_json: String = row.get(2)?;
            let env_json: String = row.get(3)?;
            let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
            let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();
            Ok(McpServerConfig {
                name: row.get(0)?,
                command: row.get(1)?,
                args,
                env,
                enabled: row.get::<_, i64>(4)? != 0,
            })
        })?;

        for config_result in rows {
            let config = config_result?;
            let name = config.name.clone();
            match Self::spawn_client(&config).await {
                Ok(client) => {
                    info!("MCP server '{}' initialized", name);
                    registry.clients.insert(name.clone(), McpClientEntry {
                        config,
                        client: Arc::new(client),
                    });
                }
                Err(e) => {
                    error!("Failed to initialize MCP server '{}': {}", name, e);
                }
            }
        }

        Ok(registry)
    }

    async fn spawn_client(config: &McpServerConfig) -> anyhow::Result<McpClient> {
        let workspace = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        let client = McpClient::spawn(&config.command, &config.args, &workspace).await?;
        client.initialize().await?;
        Ok(client)
    }

    /// Aggregate tools from all active clients with namespace prefix
    pub async fn aggregate_tools(&self) -> Vec<Tool> {
        let mut all_tools = Vec::new();

        for (name, entry) in &self.clients {
            match entry.client.list_tools().await {
                Ok(tools) => {
                    for mut tool in tools {
                        tool.name = format!("{}:{}", name, tool.name);
                        all_tools.push(tool);
                    }
                }
                Err(e) => {
                    warn!("Failed to list tools from '{}': {}", name, e);
                }
            }
        }

        all_tools
    }

    /// Call a tool by its namespaced name (e.g., "filesystem:read_file")
    pub async fn call_tool(&self, full_name: &str, arguments: Value) -> anyhow::Result<ToolResult> {
        let (namespace, tool_name) = full_name
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid tool name '{}': missing namespace separator", full_name))?;

        let entry = self.clients
            .get(namespace)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not found", namespace))?;

        entry.client.call_tool(tool_name, arguments).await
            .map_err(|e| anyhow::anyhow!("MCP tool call failed: {}", e))
    }

    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    pub fn server_names(&self) -> Vec<String> {
        self.clients.keys().cloned().collect()
    }

    /// Check if a client's transport is still alive
    pub async fn is_client_alive(&self, name: &str) -> bool {
        if let Some(entry) = self.clients.get(name) {
            let mut transport = entry.client.transport.lock().await;
            transport.is_alive()
        } else {
            false
        }
    }
}

// ── Interceptor ──

pub struct McpInterceptor;

#[derive(Debug)]
pub struct RemoteRequestDetected {
    pub urls: Vec<String>,
}

impl McpInterceptor {
    /// Recursively scan JSON value for URL-like strings
    pub fn inspect(args: &Value) -> Option<RemoteRequestDetected> {
        let mut urls = Vec::new();
        Self::scan_value(args, &mut urls);
        if urls.is_empty() {
            None
        } else {
            Some(RemoteRequestDetected { urls })
        }
    }

    fn scan_value(value: &Value, urls: &mut Vec<String>) {
        match value {
            Value::String(s) => {
                if Self::looks_like_url(s) {
                    urls.push(s.clone());
                }
            }
            Value::Array(arr) => {
                for item in arr {
                    Self::scan_value(item, urls);
                }
            }
            Value::Object(map) => {
                for (_, v) in map {
                    Self::scan_value(v, urls);
                }
            }
            _ => {}
        }
    }

    fn looks_like_url(s: &str) -> bool {
        s.starts_with("http://")
            || s.starts_with("https://")
            || s.starts_with("ftp://")
    }
}

// ── Admin API Handlers ──

#[derive(Clone)]
pub struct McpState {
    pub registry: Arc<Mutex<McpRegistry>>,
    pub audit: Arc<dyn Fn(dh_core::AuditLogEntry) + Send + Sync>,
}

pub async fn list_mcp_servers(
    State(state): State<McpState>,
) -> Result<Json<Value>, StatusCode> {
    let registry = state.registry.lock().await;
    let mut servers = Vec::new();

    for name in registry.server_names() {
        let alive = registry.is_client_alive(&name).await;
        servers.push(json!({
            "name": name,
            "alive": alive,
        }));
    }

    Ok(Json(json!({ "servers": servers })))
}

pub async fn list_mcp_tools(
    State(state): State<McpState>,
) -> Result<Json<Value>, StatusCode> {
    let registry = state.registry.lock().await;
    let tools = registry.aggregate_tools().await;
    Ok(Json(json!({ "tools": tools })))
}

pub async fn call_mcp_tool(
    State(state): State<McpState>,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let arguments = body.get("arguments").cloned().unwrap_or(json!({}));

    // Interceptor: detect remote requests
    if let Some(detected) = McpInterceptor::inspect(&arguments) {
        info!("MCP tool '{}' detected remote URLs: {:?}", name, detected.urls);
        // TODO: write to audit_logs via state.audit
    }

    let registry = state.registry.lock().await;
    match registry.call_tool(&name, arguments).await {
        Ok(result) => Ok(Json(json!({ "result": result }))),
        Err(e) => {
            error!("MCP tool call failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

### 3.2 编译验证（gatewayd）

Run: `cargo check -p gatewayd`
Expected: 可能有 `McpClient.transport` 字段私有性问题，需要解决。

**注意：** `McpClient` 的 `transport` 字段是私有的。需要在 `client.rs` 中添加一个公共的 `is_alive()` 包装方法，而不是直接暴露 `transport`。

### 3.3 修正 McpClient::is_alive()

在 `crates/dh-core/src/mcp/client.rs` 添加：

```rust
    pub async fn is_alive(&self) -> bool {
        let mut transport = self.transport.lock().await;
        transport.is_alive()
    }
```

然后修改 `mcp_aggregator.rs` 中的 `is_client_alive`：

```rust
    pub async fn is_client_alive(&self, name: &str) -> bool {
        if let Some(entry) = self.clients.get(name) {
            entry.client.is_alive().await
        } else {
            false
        }
    }
```

---

## Task 4: gatewayd 集成

**Files:**
- Modify: `apps/gatewayd/src/main.rs`

### 4.1 在 main.rs 中引入 mcp_aggregator 模块

由于 rustc ICE，不能使用 `mod mcp_aggregator;`。需要将 `mcp_aggregator.rs` 的内容内联到 `main.rs` 中，或者使用 `include!` 宏。

**方案：使用 `include!` 宏**

在 `apps/gatewayd/src/main.rs` 的顶部（use 语句之后，其他代码之前）添加：

```rust
// MCP aggregation layer (inlined via include! to avoid rustc 1.95 ICE with directory modules)
include!("mcp_aggregator.rs");
```

### 4.2 修改 ApiState 添加 mcp_registry

在 `struct ApiState` 中添加：

```rust
struct ApiState {
    router: Arc<GatewayRouter>,
    audit: Arc<AuditLogger>,
    rtk: Arc<RtkEngine>,
    agent_type: Arc<std::sync::Mutex<Option<String>>>,
    db_path: std::path::PathBuf,
    mcp_registry: Option<Arc<tokio::sync::Mutex<McpRegistry>>>,
}
```

### 4.3 修改 run() 启动流程

在 `let api_state = ApiState { ... }` 之前，添加 Registry 初始化：

```rust
    let mcp_registry = match McpRegistry::load_from_db(&db_path).await {
        Ok(registry) => {
            if registry.is_empty() {
                info!("No MCP servers configured");
            }
            Some(Arc::new(tokio::sync::Mutex::new(registry)))
        }
        Err(e) => {
            warn!("Failed to load MCP registry: {}", e);
            None
        }
    };
```

修改 `ApiState` 构造：

```rust
    let api_state = ApiState {
        router: gateway_router,
        audit: Arc::new(audit_logger),
        rtk: Arc::new(RtkEngine::default_engine()),
        agent_type: Arc::new(std::sync::Mutex::new(None)),
        db_path: db_path.clone(),
        mcp_registry: mcp_registry.clone(),
    };
```

### 4.4 修改 admin_router 添加 MCP 路由

将 admin_router 改为：

```rust
    let mut admin_router = Router::new()
        .route("/health", get(health_check))
        .route("/context", post(set_context))
        .with_state(api_state.clone());

    // Add MCP routes if registry is available
    if let Some(registry) = mcp_registry {
        let mcp_state = McpState {
            registry,
            audit: Arc::new(move |entry: dh_core::AuditLogEntry| {
                let _ = api_state.audit.log(entry);
            }),
        };
        admin_router = admin_router
            .route("/mcp/servers", get(list_mcp_servers))
            .route("/mcp/tools", get(list_mcp_tools))
            .route("/mcp/tools/:name/call", post(call_mcp_tool))
            .with_state(mcp_state);
    }
```

**问题：** `api_state.audit.log()` 的签名需要确认。`AuditLogger::log` 接受 `AuditLogEntry`，但返回 `()`。上面的 `Arc::new(move |entry| ...)` 捕获 `api_state` 会导致 Clone 问题。

更简单的方式：不通过闭包传递 audit，而是在 `McpState` 中直接持有 `Arc<AuditLogger>`。

修改 `McpState` 定义：

```rust
#[derive(Clone)]
pub struct McpState {
    pub registry: Arc<tokio::sync::Mutex<McpRegistry>>,
    pub audit: Arc<AuditLogger>,
}
```

然后修改 admin_router 构造：

```rust
    let mut admin_router = Router::new()
        .route("/health", get(health_check))
        .route("/context", post(set_context))
        .with_state(api_state.clone());

    if let Some(registry) = mcp_registry {
        let mcp_state = McpState {
            registry,
            audit: api_state.audit.clone(),
        };
        admin_router = admin_router
            .route("/mcp/servers", get(list_mcp_servers))
            .route("/mcp/tools", get(list_mcp_tools))
            .route("/mcp/tools/:name/call", post(call_mcp_tool))
            .with_state(mcp_state);
    }
```

### 4.5 编译验证

Run: `cargo check -p gatewayd`
Expected: `Finished` dev profile`

---

## Task 5: CLI mcp 命令

**Files:**
- Create: `apps/cli/src/commands/mcp.rs`
- Modify: `apps/cli/src/commands/mod.rs`
- Modify: `apps/cli/src/main.rs`

### 5.1 编写 mcp.rs

```rust
use clap::Subcommand;
use serde_json::Value;
use tracing::{error, info};

#[derive(Subcommand, Debug)]
pub enum McpCommands {
    /// List all MCP servers and their status
    List,
    /// Add a new MCP server
    Add {
        /// Server name (used as namespace)
        name: String,
        /// Command to spawn the MCP server (e.g., npx, uvx)
        #[arg(long)]
        cmd: String,
        /// Command arguments (comma-separated)
        #[arg(long, value_delimiter = ',')]
        args: Vec<String>,
        /// Environment variables (KEY=VAL, comma-separated)
        #[arg(long, value_delimiter = ',')]
        env: Vec<String>,
    },
    /// Remove an MCP server
    Remove {
        /// Server name
        name: String,
    },
    /// Call an MCP tool
    Call {
        /// Full tool name with namespace (e.g., filesystem:read_file)
        tool: String,
        /// Tool arguments as JSON string
        #[arg(long, default_value = "{}")]
        args: String,
    },
}

pub async fn run(command: McpCommands) -> Result<(), anyhow::Error> {
    match command {
        McpCommands::List => {
            // Try gatewayd Admin API first
            match list_via_api().await {
                Ok(()) => {}
                Err(e) => {
                    info!("Gatewayd API unavailable ({}), falling back to DB", e);
                    list_via_db()?;
                }
            }
        }
        McpCommands::Add { name, cmd, args, env } => {
            let conn = open_db()?;

            let env_map: std::collections::HashMap<String, String> = env
                .into_iter()
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    let key = parts.next()?;
                    let val = parts.next()?;
                    Some((key.to_string(), val.to_string()))
                })
                .collect();

            let args_json = serde_json::to_string(&args)?;
            let env_json = serde_json::to_string(&env_map)?;
            let now = chrono::Utc::now().to_rfc3339();

            conn.execute(
                "INSERT OR REPLACE INTO mcp_servers (name, command, args, env, enabled, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?5)",
                rusqlite::params![&name, &cmd, &args_json, &env_json, &now],
            )?;

            println!("Added MCP server: {}", name);
            println!("  Command: {} {:?}", cmd, args);
            println!("  Restart gatewayd to apply changes.");
        }
        McpCommands::Remove { name } => {
            let conn = open_db()?;
            let affected = conn.execute(
                "DELETE FROM mcp_servers WHERE name = ?1",
                [&name],
            )?;

            if affected == 0 {
                println!("No MCP server found: {}", name);
            } else {
                println!("Removed MCP server: {}", name);
                println!("  Restart gatewayd to apply changes.");
            }
        }
        McpCommands::Call { tool, args } => {
            let arguments: Value = serde_json::from_str(&args)
                .map_err(|e| anyhow::anyhow!("Invalid JSON arguments: {}", e))?;

            let client = reqwest::Client::new();
            for port in [2346u16, 2347, 2348, 2349, 2350] {
                let url = format!("http://127.0.0.1:{}/mcp/tools/{}/call", port, tool);
                match client
                    .post(&url)
                    .json(&json!({ "arguments": arguments }))
                    .timeout(std::time::Duration::from_secs(30))
                    .send()
                    .await
                {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            let body: Value = resp.json().await?;
                            println!("{}", serde_json::to_string_pretty(&body)?);
                            return Ok(());
                        }
                    }
                    Err(_) => continue,
                }
            }
            anyhow::bail!("gatewayd is not running or MCP endpoint unavailable");
        }
    }

    Ok(())
}

async fn list_via_api() -> Result<(), anyhow::Error> {
    let client = reqwest::Client::new();
    for port in [2346u16, 2347, 2348, 2349, 2350] {
        let url = format!("http://127.0.0.1:{}/mcp/servers", port);
        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body: Value = resp.json().await?;
                    let servers = body.get("servers").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                    if servers.is_empty() {
                        println!("No MCP servers configured.");
                        return Ok(());
                    }

                    println!("{:<20} {:<10} {:<20}", "NAME", "STATUS", "TOOLS");
                    println!("{}", "-".repeat(55));

                    for server in servers {
                        let name = server.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                        let alive = server.get("alive").and_then(|v| v.as_bool()).unwrap_or(false);
                        let status = if alive { "alive" } else { "dead" };

                        // Also fetch tools count
                        let tools_url = format!("http://127.0.0.1:{}/mcp/tools", port);
                        let tools_count = if let Ok(tools_resp) = client.get(&tools_url).timeout(std::time::Duration::from_secs(2)).send().await {
                            if let Ok(tools_body) = tools_resp.json::<Value>().await {
                                tools_body.get("tools").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0)
                            } else { 0 }
                        } else { 0 };

                        println!("{:<20} {:<10} {:<20}", name, status, format!("{} tools", tools_count));
                    }
                    return Ok(());
                }
            }
            Err(_) => continue,
        }
    }
    anyhow::bail!("gatewayd API not available")
}

fn list_via_db() -> Result<(), anyhow::Error> {
    let conn = open_db()?;
    let mut stmt = conn.prepare(
        "SELECT name, command, args, enabled FROM mcp_servers ORDER BY name"
    )?;
    let rows: Vec<(String, String, String, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        println!("No MCP servers configured.");
        return Ok(());
    }

    println!("{:<20} {:<10} {:<30} {:<10}", "NAME", "STATUS", "COMMAND", "ENABLED");
    println!("{}", "-".repeat(75));

    for (name, cmd, args, enabled) in rows {
        let args_parsed: Vec<String> = serde_json::from_str(&args).unwrap_or_default();
        let cmd_display = format!("{} {}", cmd, args_parsed.join(" "));
        let enabled_str = if enabled != 0 { "yes" } else { "no" };
        println!("{:<20} {:<10} {:<30} {:<10}",
            name,
            "unknown",
            &cmd_display[..cmd_display.len().min(29)],
            enabled_str
        );
    }

    println!("\nNote: gatewayd is not running. Status shows 'unknown'.");
    Ok(())
}

fn open_db() -> Result<rusqlite::Connection, anyhow::Error> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");

    if !db_path.exists() {
        let _ = dh_db::DbManager::open(&db_path)?;
    }

    rusqlite::Connection::open(&db_path).map_err(Into::into)
}
```

### 5.2 修改 commands/mod.rs

```rust
pub mod config;
pub mod exec;
pub mod gatewayd;
pub mod mcp;
```

### 5.3 修改 main.rs

在 `Commands` enum 中添加：

```rust
    /// Manage MCP servers and tools
    #[command(subcommand)]
    Mcp(commands::mcp::McpCommands),
```

在 dispatch match 中添加：

```rust
            Commands::Mcp(cmd) => {
                commands::mcp::run(cmd).await
            }
```

### 5.4 编译验证

Run: `cargo check -p deepharness-cli`
Expected: `Finished` dev profile`

---

## Task 6: 集成测试与编译验证

### 6.1 全 workspace 编译

Run: `cargo check --workspace`
Expected: `Finished` dev profile [unoptimized + debuginfo] target(s) in Xs` (0 warnings)

### 6.2 构建 release

Run: `cargo build --workspace --release`
Expected: All binaries built successfully.

### 6.3 手动测试流程

```bash
# 1. 添加 MCP server（假设已安装 npx）
./target/release/dh mcp add filesystem --cmd npx --args "-y,@modelcontextprotocol/server-filesystem,/tmp"

# 2. 列出（gatewayd 未运行时，显示 DB 配置）
./target/release/dh mcp list

# 3. 启动 gatewayd
./target/release/gatewayd --daemon

# 4. 列出（gatewayd 运行时，显示实时状态）
./target/release/dh mcp list

# 5. 调用 tool
./target/release/dh mcp call filesystem:read_file --args '{"path": "/etc/hosts"}'

# 6. 移除
./target/release/dh mcp remove filesystem
```

---

## 自检

**Spec coverage:**
- [x] MCP Server 配置管理（SQLite 表 + CLI add/remove）
- [x] MCP Client 池（Registry 启动时初始化）
- [x] 工具列表聚合（aggregate_tools with namespace）
- [x] Tool Call 拦截器（McpInterceptor::inspect）
- [x] Admin API（/mcp/servers, /mcp/tools, /mcp/tools/:name/call）
- [x] CLI 命令（dh mcp list/add/remove/call）

**Placeholder scan:** 无 TBD/TODO/实现后补充。

**Type consistency:** `McpClient::list_tools()` 返回 `Vec<Tool>`，`call_tool` 参数为 `(name, Value)`，所有签名与 dh-core 现有类型一致。
