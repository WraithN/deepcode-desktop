use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use dh_core::mcp::client::McpClient;
use dh_core::mcp::types::{Tool, ToolResult};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
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
    /// Reserved for future dynamic enable/disable. Currently filtered at DB query time.
    #[allow(dead_code)]
    pub enabled: bool,
}

pub struct McpClientEntry {
    pub config: McpServerConfig,
    pub client: Arc<McpClient>,
}

impl std::fmt::Debug for McpClientEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClientEntry")
            .field("config", &self.config)
            .field("client", &"<McpClient>")
            .finish()
    }
}

/// MCP 聚合注册表
pub struct McpRegistry {
    clients: HashMap<String, McpClientEntry>,
}

impl McpRegistry {
    pub async fn load_from_db(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = rusqlite::Connection::open(db_path)?;
        let mut registry = Self {
            clients: HashMap::new(),
        };

        // Check if aggregator is enabled
        let enabled: bool = {
            let mut stmt = conn.prepare("SELECT value FROM configs WHERE key = ?1")?;
            let mut rows =
                stmt.query_map([MCP_AGGREGATOR_ENABLED_KEY], |row| row.get::<_, String>(0))?;
            match rows.next() {
                Some(Ok(v)) => v.parse().unwrap_or(true),
                _ => true,
            }
        };

        if !enabled {
            info!("MCP aggregator disabled via config");
            return Ok(registry);
        }

        // Load enabled servers
        let mut stmt = conn.prepare(
            "SELECT name, command, args, env, enabled FROM mcp_servers WHERE enabled = 1",
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
                    registry.clients.insert(
                        name.clone(),
                        McpClientEntry {
                            config,
                            client: Arc::new(client),
                        },
                    );
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

        let client =
            McpClient::spawn(&config.command, &config.args, &config.env, &workspace).await?;
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
        let (namespace, tool_name) = full_name.split_once(':').ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid tool name '{}': missing namespace separator",
                full_name
            )
        })?;

        let entry = self
            .clients
            .get(namespace)
            .ok_or_else(|| anyhow::anyhow!("MCP server '{}' not found", namespace))?;

        entry
            .client
            .call_tool(tool_name, arguments)
            .await
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
            entry.client.is_alive().await
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
        s.starts_with("http://") || s.starts_with("https://") || s.starts_with("ftp://")
    }
}

// ── Admin API Handlers ──

use super::ApiState;

pub async fn list_mcp_servers(State(state): State<ApiState>) -> Result<Json<Value>, StatusCode> {
    let registry = state
        .mcp_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let registry = registry.lock().await;
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

pub async fn list_mcp_tools(State(state): State<ApiState>) -> Result<Json<Value>, StatusCode> {
    let registry = state
        .mcp_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let registry = registry.lock().await;
    let tools = registry.aggregate_tools().await;
    Ok(Json(json!({ "tools": tools })))
}

pub async fn call_mcp_tool(
    State(state): State<ApiState>,
    Path(name): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, StatusCode> {
    let arguments = body.get("arguments").cloned().unwrap_or(json!({}));

    // Interceptor: detect remote requests
    if let Some(detected) = McpInterceptor::inspect(&arguments) {
        info!(
            "MCP tool '{}' detected remote URLs: {:?}",
            name, detected.urls
        );
        // Log to audit (non-blocking, fire-and-forget)
        let mut entry = dh_core::AuditLogEntry::new(
            "mcp".to_string(),
            uuid::Uuid::new_v4().to_string(),
            dh_core::Direction::Request,
            "mcp".to_string(),
            name.clone(),
        );
        entry.metadata = json!({
            "detected_urls": detected.urls,
            "tool_arguments": arguments,
        });
        state.audit.log(entry);
    }

    let registry = state
        .mcp_registry
        .as_ref()
        .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let registry = registry.lock().await;
    match registry.call_tool(&name, arguments).await {
        Ok(result) => Ok(Json(json!({ "result": result }))),
        Err(e) => {
            error!("MCP tool call failed: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
