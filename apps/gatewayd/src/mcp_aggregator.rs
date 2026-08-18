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
use std::time::Duration;
use tracing::{error, info, warn};

// ── Config ──

const MCP_AGGREGATOR_ENABLED_KEY: &str = "mcp_aggregator_enabled";

/// DB 中 transport 列的取值：stdio 子进程传输。
const TRANSPORT_KIND_STDIO: &str = "stdio";
/// DB 中 transport 列的取值：MCP Streamable HTTP 传输。
const TRANSPORT_KIND_HTTP: &str = "http";

/// HTTP transport url 必须使用的前缀。
const HTTP_URL_PREFIXES: &[&str] = &["http://", "https://"];

/// spawn_client 在 url 为空时的兜底值（仅用于 Http transport 错误信息可读性）。
const EMPTY_URL_FALLBACK: &str = "";

/// crawler MCP server 在 registry 中的固定名字。
/// 同时也是 dh-backend 返回 JSON 中 url 字段指向的 MCP server 标识。
const CRAWLER_SERVER_NAME: &str = "crawler";

/// dh-backend 拉取 crawler 配置的 API 路径（追加到 platform.url 之后）。
const BACKEND_CRAWLER_CONFIG_PATH: &str = "/api/v1/admin/services/crawler";

/// dh-backend 返回 JSON 中 crawler MCP server 的 url 字段名。
const BACKEND_CRAWLER_URL_FIELD: &str = "url";

/// dh-backend 返回 JSON 中 crawler 最大爬取深度的字段名。
const BACKEND_CRAWLER_MAX_DEPTH_FIELD: &str = "maxDepth";

/// dh-backend 返回 JSON 中 crawler MCP 请求超时的字段名（单位：毫秒）。
const BACKEND_CRAWLER_TIMEOUT_MS_FIELD: &str = "timeoutMs";

/// 拉取 dh-backend crawler 配置的 HTTP 请求超时（秒）。
///
/// 防止 dh-backend 挂起导致 gatewayd 启动被无限阻塞（finding 2 修复）。
const CRAWLER_CONFIG_FETCH_TIMEOUT_SECS: u64 = 10;

/// crawler MCP 客户端请求超时的兜底值（毫秒）。
///
/// 当 dh-backend 未返回 `timeoutMs` 或返回非正值时使用；
/// 与 dh-backend 侧的默认 crawler 超时（60000ms）对齐。
const CRAWLER_MCP_TIMEOUT_FALLBACK_MS: i64 = 60_000;

// ── Types ──

/// MCP server 传输类型。
///
/// - `Stdio`：通过子进程 stdio 通信（command/args/env）。
/// - `Http`：通过 MCP Streamable HTTP 协议访问远程 server（url）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Stdio,
    Http,
}

impl TransportKind {
    /// 从 DB 中 transport 列的字符串解析为枚举。未知值统一回退到 Stdio。
    fn from_db_str(s: &str) -> Self {
        match s {
            TRANSPORT_KIND_HTTP => TransportKind::Http,
            _ => TransportKind::Stdio,
        }
    }
}

#[derive(Debug, Clone)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    /// 传输类型：决定 spawn_client 走 stdio 子进程还是 HTTP 连接。
    pub transport: TransportKind,
    /// HTTP transport 用的 URL；Stdio 行可为 None。
    pub url: Option<String>,
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

const ENV_MCP_BIN_DIR: &str = "GATEWAYD_MCP_BIN_DIR";

/// Allowed command names and absolute-path binary directory for MCP servers.
/// These are intentionally conservative to prevent arbitrary shell execution.
const MCP_ALLOWED_COMMANDS: &[&str] = &[
    "npx",
    "node",
    "python",
    "python3",
    "uv",
    "bun",
    "deno",
];

/// Argument patterns that are rejected from MCP server command lines.
const MCP_ARG_BLACKLIST: &[&str] = &["--eval", "-e", ">", "|", "$", "`"];

/// Errors that can occur when validating an MCP server configuration.
#[derive(Debug, thiserror::Error)]
pub enum McpValidationError {
    #[error("MCP command '{0}' is not in the allowed list")]
    DisallowedCommand(String),
    #[error("MCP command '{0}' must be an absolute path inside GATEWAYD_MCP_BIN_DIR")]
    OutsideBinDir(String),
    #[error("MCP command '{0}' references a forbidden shell pattern")]
    ForbiddenArgPattern(String),
}

impl McpServerConfig {
    /// 校验配置安全可执行。
    ///
    /// 按 transport 分支：
    /// - `Http`：只校验 url 以 `http://` 或 `https://` 开头（复用 `DisallowedCommand`
    ///   变体，错误信息明确说明需要 http url）。
    /// - `Stdio`：保持原有 command/args 校验逻辑不变（白名单 / bin_dir / 参数黑名单）。
    pub fn validate(&self) -> Result<(), McpValidationError> {
        match self.transport {
            TransportKind::Http => Self::validate_http(&self.url),
            TransportKind::Stdio => self.validate_stdio(),
        }
    }

    /// Http transport 校验：url 必须是 http/https 开头。
    fn validate_http(url: &Option<String>) -> Result<(), McpValidationError> {
        let url_str = url.as_deref().unwrap_or(EMPTY_URL_FALLBACK);
        let is_valid = HTTP_URL_PREFIXES
            .iter()
            .any(|prefix| url_str.starts_with(prefix));
        if !is_valid {
            return Err(McpValidationError::DisallowedCommand(format!(
                "http url required (http:// or https://), got: {}",
                url_str
            )));
        }
        Ok(())
    }

    /// Stdio transport 校验：保持原有 command 白名单 + bin_dir + 参数黑名单逻辑。
    fn validate_stdio(&self) -> Result<(), McpValidationError> {
        let cmd = self.command.trim();

        // 裸命令名必须在白名单内。
        if !cmd.contains(std::path::MAIN_SEPARATOR) {
            if !MCP_ALLOWED_COMMANDS.contains(&cmd) {
                return Err(McpValidationError::DisallowedCommand(cmd.to_string()));
            }
        } else {
            // 绝对路径必须位于配置的 bin 目录内。
            let path = std::path::Path::new(cmd);
            let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let bin_dir = std::env::var(ENV_MCP_BIN_DIR)
                .ok()
                .and_then(|s| std::fs::canonicalize(s).ok());
            match bin_dir {
                Some(dir) if canonical.starts_with(&dir) => {}
                _ => {
                    return Err(McpValidationError::OutsideBinDir(cmd.to_string()));
                }
            }
        }

        // 拒绝危险参数模式。
        for arg in &self.args {
            if MCP_ARG_BLACKLIST.iter().any(|bad| arg.contains(bad)) {
                return Err(McpValidationError::ForbiddenArgPattern(arg.clone()));
            }
        }

        Ok(())
    }
}

/// 根据 dh-backend 下发的 `timeoutMs`（毫秒）计算 crawler MCP 客户端请求超时。
///
/// `timeoutMs` 缺失或非正时回退到 `CRAWLER_MCP_TIMEOUT_FALLBACK_MS`，
/// 保证 crawler 调用始终有明确的上限，避免请求无限挂起。
fn crawler_request_timeout(timeout_ms: i64) -> Duration {
    let ms = if timeout_ms > 0 {
        timeout_ms
    } else {
        CRAWLER_MCP_TIMEOUT_FALLBACK_MS
    };
    Duration::from_millis(ms as u64)
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
        // 查询包含新增的 transport/url 列：
        // - transport: 'stdio' | 'http'，未知值回退 Stdio
        // - url: HTTP transport 用；stdio 行可为 NULL
        let mut stmt = conn.prepare(
            "SELECT name, command, args, env, enabled, transport, url \
             FROM mcp_servers WHERE enabled = 1",
        )?;
        let rows = stmt.query_map([], |row| {
            let args_json: String = row.get(2)?;
            let env_json: String = row.get(3)?;
            let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
            let env: HashMap<String, String> = serde_json::from_str(&env_json).unwrap_or_default();
            // transport 列 NOT NULL DEFAULT 'stdio'，理论上必有值；
            // 兜底处理 NULL 以防旧库 ALTER 前的极端情况。
            let transport_str: String =
                row.get(5).unwrap_or_else(|_| TRANSPORT_KIND_STDIO.to_string());
            let transport = TransportKind::from_db_str(&transport_str);
            // url 列允许 NULL（stdio 行无需 url）；rusqlite 原生将 NULL 映射为 None。
            let url: Option<String> = row.get(6)?;
            Ok(McpServerConfig {
                name: row.get(0)?,
                command: row.get(1)?,
                args,
                env,
                transport,
                url,
                enabled: row.get::<_, i64>(4)? != 0,
            })
        })?;

        for config_result in rows {
            let config = config_result?;
            let name = config.name.clone();
            if let Err(e) = config.validate() {
                error!("MCP server '{}' configuration rejected: {}", name, e);
                continue;
            }
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

    /// 从 dh-backend 拉取 crawler 配置并注册为 Http MCP server。
    ///
    /// 调用 dh-backend 的 `GET /api/v1/admin/services/crawler`，解析返回的
    /// `{ url, maxDepth, timeoutMs }`，将 crawler 以 `Http` transport 注册到
    /// registry。成功时返回拉取到的 `maxDepth`；失败时返回 `Err`，由调用方
    /// 决定是否阻断启动（brief 要求仅 warn 不阻断，回退到默认 maxDepth）。
    ///
    /// 失败模式见 brief：网络错误、HTTP 非 2xx、JSON 解析失败、url 为空、
    /// MCP 握手失败均会返回 `Err`，调用方应记录 warn 后继续启动。
    pub async fn load_remote_from_backend(
        &mut self,
        backend_url: &str,
        token: &str,
    ) -> anyhow::Result<i64> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(CRAWLER_CONFIG_FETCH_TIMEOUT_SECS))
            .build()
            .map_err(|e| anyhow::anyhow!("build crawler config client: {e}"))?;
        let endpoint = format!(
            "{}{}",
            backend_url.trim_end_matches('/'),
            BACKEND_CRAWLER_CONFIG_PATH
        );
        let resp = client
            .get(&endpoint)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("fetch crawler config: {e}"))?;
        if !resp.status().is_success() {
            anyhow::bail!("dh-backend crawler config HTTP {}", resp.status());
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("parse crawler config: {e}"))?;
        let url = body
            .get(BACKEND_CRAWLER_URL_FIELD)
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let max_depth = body
            .get(BACKEND_CRAWLER_MAX_DEPTH_FIELD)
            .and_then(|v| v.as_i64())
            .unwrap_or(crate::mcp_proxy_server::MCP_DEFAULT_MAX_DEPTH);
        // timeoutMs 是 crawler-service 的调用超时（毫秒），在拿到响应后才可知，
        // 因此无法作用于本次拉取请求本身；将其应用于 crawler MCP 客户端，
        // 使 agent 对 crawler 的调用尊重平台下发的超时策略。
        let timeout_ms = body
            .get(BACKEND_CRAWLER_TIMEOUT_MS_FIELD)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if url.is_empty() {
            anyhow::bail!("crawler config url empty");
        }
        let config = McpServerConfig {
            name: CRAWLER_SERVER_NAME.into(),
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            transport: TransportKind::Http,
            url: Some(url.into()),
            enabled: true,
        };
        let mcp_client = McpClient::connect_http_with_timeout(url, crawler_request_timeout(timeout_ms))
            .await
            .map_err(|e| anyhow::anyhow!("connect crawler MCP: {e}"))?;
        self.clients.insert(
            CRAWLER_SERVER_NAME.into(),
            McpClientEntry {
                config,
                client: Arc::new(mcp_client),
            },
        );
        info!("crawler MCP server loaded from backend: {}", url);
        Ok(max_depth)
    }

    async fn spawn_client(config: &McpServerConfig) -> anyhow::Result<McpClient> {
        let workspace = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());

        // 按 transport 分支构造 client：
        // - Stdio：spawn 子进程，随后手动 initialize 完成协议握手
        //   （McpClient::spawn 不会自动 initialize）。
        // - Http：connect_http 内部已调用 initialize 完成握手，无需重复调用。
        let client = match config.transport {
            TransportKind::Stdio => {
                let c = McpClient::spawn(&config.command, &config.args, &config.env, &workspace)
                    .await?;
                c.initialize().await?;
                c
            }
            TransportKind::Http => {
                McpClient::connect_http(config.url.as_deref().unwrap_or(EMPTY_URL_FALLBACK)).await?
            }
        };

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

    /// Shut down all connected MCP clients and terminate their child processes.
    /// Should be called before the gatewayd process exits.
    pub async fn shutdown(&self) {
        for (name, entry) in &self.clients {
            if let Err(e) = entry.client.shutdown().await {
                error!("Failed to shutdown MCP server '{}': {}", name, e);
            } else {
                info!("MCP server '{}' shut down", name);
            }
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
            Value::String(s) if Self::looks_like_url(s) => {
                urls.push(s.clone());
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
