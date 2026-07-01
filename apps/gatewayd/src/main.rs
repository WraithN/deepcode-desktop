use axum::{
    Router,
    body::{Body, Bytes},
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
};
use clap::Parser;
use dh_core::{AuditLogEntry, Direction, Message, Provider, Role, UnifiedRequest, estimate_tokens};
use dh_db::DbManager;
use reqwest::Client;
use rusqlite::params;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

mod agents {
    #![allow(dead_code)]
    include!("agents_impl.rs");
}
mod agui;
mod agui_sink;
mod handlers;
mod mcp_aggregator;
mod reporter;
mod session;

/// 空闲实例回收任务的扫描间隔（秒）。
const REAPER_INTERVAL_SECS: u64 = 60;

// Audit module (inlined to avoid rustc 1.95 ICE with directory modules)
struct AuditLogger {
    sender: mpsc::UnboundedSender<AuditLogEntry>,
}

impl AuditLogger {
    fn new() -> (Self, mpsc::UnboundedReceiver<AuditLogEntry>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Self { sender }, receiver)
    }

    fn log(&self, entry: AuditLogEntry) {
        if let Err(e) = self.sender.send(entry) {
            error!("Failed to send audit log: {}", e);
        }
    }
}

struct AuditStorage {
    db: DbManager,
}

impl AuditStorage {
    fn new(db: DbManager) -> Self {
        Self { db }
    }

    fn insert(&mut self, entry: &AuditLogEntry) -> anyhow::Result<()> {
        let conn = self.db.conn_mut();
        conn.execute(
            r#"
            INSERT INTO audit_logs (
                id, session_id, request_id, direction, provider, model,
                agent_type, payload, payload_size_bytes, prompt_tokens, completion_tokens, total_tokens,
                timestamp, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                &entry.id,
                &entry.session_id,
                &entry.request_id,
                format!("{:?}", entry.direction).to_lowercase(),
                &entry.provider,
                &entry.model,
                entry.agent_type.as_deref(),
                entry.payload.as_deref(),
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

/// 从 JSON 响应体中提取 usage
fn extract_usage_from_json(body: &[u8]) -> Option<dh_core::TokenUsage> {
    let json: Value = serde_json::from_slice(body).ok()?;
    let usage = json.get("usage")?;
    Some(dh_core::TokenUsage {
        prompt_tokens: usage.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage.get("completion_tokens")?.as_u64()? as u32,
        total_tokens: usage.get("total_tokens")?.as_u64()? as u32,
    })
}

/// 从 SSE 流文本中提取最后一个 usage chunk
fn extract_usage_from_sse(text: &str) -> Option<dh_core::TokenUsage> {
    let mut last_usage: Option<dh_core::TokenUsage> = None;
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if let Some(usage) = extract_usage_from_json(data.as_bytes()) {
                last_usage = Some(usage);
            }
        }
    }
    last_usage
}

/// 创建 Response 审计日志并发送
fn log_response_audit(
    audit: &AuditLogger,
    session_id: String,
    request_id: String,
    provider: String,
    model: String,
    bytes: &[u8],
    request_body: &str,
) {
    let usage = extract_usage_from_json(bytes).or_else(|| {
        let text = String::from_utf8_lossy(bytes);
        extract_usage_from_sse(&text)
    });

    let usage = match usage {
        Some(u) => u,
        None => {
            let response_text = String::from_utf8_lossy(bytes);
            dh_core::TokenUsage {
                prompt_tokens: estimate_tokens(request_body, &model),
                completion_tokens: estimate_tokens(&response_text, &model),
                total_tokens: 0,
            }
        }
    };

    let mut entry =
        AuditLogEntry::new(session_id, request_id, Direction::Response, provider, model);
    entry.token_usage = Some(usage);
    entry.payload_size_bytes = bytes.len();
    entry.metadata = serde_json::json!({
        "token_source": if extract_usage_from_json(bytes).is_some() || extract_usage_from_sse(&String::from_utf8_lossy(bytes)).is_some() {
            "provider"
        } else {
            "estimated"
        }
    });
    audit.log(entry);
}

async fn run_storage_worker(
    mut receiver: mpsc::UnboundedReceiver<AuditLogEntry>,
    mut storage: AuditStorage,
) {
    info!("Audit storage worker started");
    while let Some(entry) = receiver.recv().await {
        if let Err(e) = storage.insert(&entry) {
            error!("Failed to persist audit log: {}", e);
        }
    }
    info!("Audit storage worker stopped");
}

// Gateway transformer (inlined)
fn openai_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::OpenAi,
        body.get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("gpt-4o")
            .to_string(),
    );

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let content = m.get("content")?.as_str()?;
                let role = match role {
                    "system" => Role::System,
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Some(Message {
                    role,
                    content: content.to_string(),
                })
            })
            .collect();
    }

    req.temperature = body
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);
    req.max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    req.stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(true);

    req
}

fn anthropic_to_unified(body: Value) -> UnifiedRequest {
    let mut req = UnifiedRequest::new(
        Provider::Anthropic,
        body.get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("claude-sonnet-4")
            .to_string(),
    );

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        req.messages = messages
            .iter()
            .filter_map(|m| {
                let role = m.get("role")?.as_str()?;
                let content = m.get("content")?.as_str()?;
                let role = match role {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    _ => Role::User,
                };
                Some(Message {
                    role,
                    content: content.to_string(),
                })
            })
            .collect();
    }

    if let Some(system) = body.get("system").and_then(|v| v.as_str()) {
        req.prepend_system_message(system.to_string());
    }

    req.max_tokens = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    req.temperature = body
        .get("temperature")
        .and_then(|v| v.as_f64())
        .map(|v| v as f32);

    req
}

// Gateway router (inlined)
struct GatewayRouter {
    client: Client,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
    deepseek_api_key: Option<String>,
}

impl GatewayRouter {
    fn new() -> Self {
        Self {
            client: Client::new(),
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
        }
    }

    async fn forward_openai(
        &self,
        provider: &str,
        body: String,
    ) -> Result<Response, anyhow::Error> {
        let (url, api_key) = match provider {
            "deepseek" => {
                let key = self
                    .deepseek_api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("DEEPSEEK_API_KEY not set"))?;
                ("https://api.deepseek.com/v1/chat/completions", key)
            }
            _ => {
                let key = self
                    .openai_api_key
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY not set"))?;
                ("https://api.openai.com/v1/chat/completions", key)
            }
        };

        info!("Forwarding {} request to {}", provider, url);

        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();
        let bytes = resp.bytes().await?;

        let mut builder = Response::builder().status(status);
        builder = builder.header("Content-Type", content_type);
        let response = builder.body(Body::from(bytes))?;

        Ok(response)
    }

    async fn forward_anthropic(&self, body: String) -> Result<Response, anyhow::Error> {
        let api_key = self
            .anthropic_api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

        info!("Forwarding request to Anthropic API");

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await?;

        let status = resp.status();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();
        let bytes = resp.bytes().await?;

        let mut builder = Response::builder().status(status);
        builder = builder.header("Content-Type", content_type);
        let response = builder.body(Body::from(bytes))?;

        Ok(response)
    }
}

// RTK Token Killer (inlined)
struct RtkConfig {
    enabled: bool,
    max_context_messages: usize,
    summary_threshold: usize,
    compress_whitespace: bool,
    deduplicate_system: bool,
}

impl Default for RtkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_context_messages: 20,
            summary_threshold: 10,
            compress_whitespace: true,
            deduplicate_system: true,
        }
    }
}

struct RtkEngine {
    config: RtkConfig,
}

impl RtkEngine {
    fn new(config: RtkConfig) -> Self {
        Self { config }
    }

    fn default_engine() -> Self {
        Self::new(RtkConfig::default())
    }

    fn optimize(&self, req: &mut UnifiedRequest) {
        if !self.config.enabled {
            return;
        }

        if self.config.deduplicate_system {
            self.deduplicate_system_prompts(req);
        }

        if req.messages.len() > self.config.summary_threshold {
            self.apply_sliding_window(req);
        }

        if self.config.compress_whitespace {
            self.compress_prompts(req);
        }

        info!(
            "RTK optimized request: {} messages -> {} messages",
            req.messages.len(),
            req.messages.len()
        );
    }

    fn deduplicate_system_prompts(&self, req: &mut UnifiedRequest) {
        let mut seen_system = false;
        req.messages.retain(|m| {
            if matches!(m.role, Role::System) {
                if seen_system {
                    return false;
                }
                seen_system = true;
            }
            true
        });
    }

    fn apply_sliding_window(&self, req: &mut UnifiedRequest) {
        let total = req.messages.len();
        if total <= self.config.max_context_messages {
            return;
        }

        let keep_recent = self.config.max_context_messages / 2;
        let summarize_count = total - keep_recent;

        let mut summarized = Vec::new();
        let mut summary_parts = Vec::new();

        for (i, msg) in req.messages.iter().take(summarize_count).enumerate() {
            let prefix = match msg.role {
                Role::System => "SYS",
                Role::User => "USR",
                Role::Assistant => "AST",
                Role::Tool => "TL",
            };
            summary_parts.push(format!(
                "[{}:{}] {}",
                i,
                prefix,
                &msg.content[..msg.content.len().min(80)]
            ));
        }

        if !summary_parts.is_empty() {
            summarized.push(Message {
                role: Role::System,
                content: format!(
                    "Previous conversation summary ({} messages condensed): {}",
                    summarize_count,
                    summary_parts.join("; ")
                ),
            });
        }

        summarized.extend(req.messages.drain(summarize_count..));
        req.messages = summarized;

        info!(
            "RTK sliding window: {} -> {} messages (summarized {})",
            total,
            req.messages.len(),
            summarize_count
        );
    }

    fn compress_prompts(&self, req: &mut UnifiedRequest) {
        for msg in &mut req.messages {
            let original_len = msg.content.len();
            msg.content = msg
                .content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            msg.content = msg.content.replace("\n\n\n", "\n\n");
            if msg.content.len() < original_len {
                info!(
                    "RTK compressed message: {} -> {} bytes",
                    original_len,
                    msg.content.len()
                );
            }
        }
    }
}

fn unified_to_openai_json(req: &UnifiedRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                },
                "content": m.content
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
        "stream": req.stream,
    });

    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }

    body
}

fn unified_to_anthropic_json(req: &UnifiedRequest) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .filter_map(|m| match m.role {
            Role::System => None,
            _ => Some(serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content
            })),
        })
        .collect();

    let system_prompt = req
        .messages
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let mut body = serde_json::json!({
        "model": req.model,
        "messages": messages,
    });

    if !system_prompt.is_empty() {
        body["system"] = serde_json::json!(system_prompt);
    }
    if let Some(max_tokens) = req.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = serde_json::json!(temp);
    }

    body
}

// Server state (inlined)
#[derive(Clone)]
struct ApiState {
    router: Arc<GatewayRouter>,
    audit: Arc<AuditLogger>,
    rtk: Arc<RtkEngine>,
    agent_type: Arc<std::sync::Mutex<Option<String>>>,
    db_path: std::path::PathBuf,
    mcp_registry: Option<Arc<tokio::sync::Mutex<mcp_aggregator::McpRegistry>>>,
    agent_service: Option<Arc<agents::AgentService>>,
    session_manager: crate::session::SessionManager,
}

fn resolve_provider(model: &str) -> &'static str {
    if model.starts_with("deepseek") {
        "deepseek"
    } else if model.starts_with("gpt") || model.starts_with("text-") {
        "openai"
    } else if model.starts_with("claude") {
        "anthropic"
    } else {
        "unknown"
    }
}

#[derive(serde::Deserialize)]
struct ContextPayload {
    agent_type: String,
    session_id: String,
    work_directory: Option<String>,
    model: Option<String>,
}

fn open_db(path: &std::path::Path) -> anyhow::Result<rusqlite::Connection> {
    rusqlite::Connection::open(path).map_err(Into::into)
}

fn upsert_session(
    db_path: &std::path::Path,
    session_id: &str,
    agent_type: &str,
    model: &str,
    work_directory: Option<&str>,
) -> anyhow::Result<()> {
    let conn = open_db(db_path)?;
    let work_directory = work_directory.unwrap_or("");
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, agent_type, model, workspace, started_at, last_active_at, status)          VALUES (?1, ?2, ?3, ?4, ?5, ?5, 'active')          ON CONFLICT(id) DO UPDATE SET          agent_type = excluded.agent_type,          model = excluded.model,          workspace = excluded.workspace,          last_active_at = excluded.last_active_at",
        rusqlite::params![session_id, agent_type, model, work_directory, now],
    )?;
    Ok(())
}

fn touch_session(db_path: &std::path::Path, session_id: &str, model: &str) -> anyhow::Result<()> {
    let conn = open_db(db_path)?;
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET last_active_at = ?1, model = ?2 WHERE id = ?3",
        rusqlite::params![now, model, session_id],
    )?;
    Ok(())
}

async fn set_context(
    State(state): State<ApiState>,
    Json(payload): Json<ContextPayload>,
) -> Json<Value> {
    let mut guard = state.agent_type.lock().unwrap();
    *guard = Some(payload.agent_type.clone());
    let model = payload.model.as_deref().unwrap_or("unknown");
    let _ = upsert_session(
        &state.db_path,
        &payload.session_id,
        &payload.agent_type,
        model,
        payload.work_directory.as_deref(),
    );
    info!(
        "Context updated: agent_type = {}, session = {}",
        payload.agent_type, payload.session_id
    );
    Json(
        serde_json::json!({"status": "ok", "agent_type": payload.agent_type, "session_id": payload.session_id}),
    )
}

async fn openai_chat_completions(State(state): State<ApiState>, body: Bytes) -> Response {
    info!("Received OpenAI chat completions request");

    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    let mut unified = openai_to_unified(body_json.clone());
    let original_size = serde_json::to_string(&body_json).unwrap_or_default().len();
    state.rtk.optimize(&mut unified);
    let optimized_json = unified_to_openai_json(&unified);
    let optimized_body = serde_json::to_string(&optimized_json).unwrap_or_default();
    let optimized_size = optimized_body.len();

    info!(
        "RTK optimized OpenAI request: {} -> {} bytes ({}% reduction)",
        original_size,
        optimized_size,
        if original_size > 0 {
            ((original_size - optimized_size) * 100 / original_size) as i32
        } else {
            0
        }
    );

    let session_id = unified.session_id.clone();
    let mut entry = AuditLogEntry::new(
        session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        resolve_provider(&unified.model).to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    entry.payload = Some(optimized_body.clone());
    entry.agent_type = state.agent_type.lock().unwrap().clone();
    let _ = touch_session(&state.db_path, &session_id, &unified.model);
    state.audit.log(entry);

    let provider = resolve_provider(&unified.model);
    match state
        .router
        .forward_openai(provider, optimized_body.clone())
        .await
    {
        Ok(response) => {
            info!("Successfully forwarded request to {}", provider);
            let (parts, body) = response.into_parts();
            match axum::body::to_bytes(body, usize::MAX).await {
                Ok(bytes) => {
                    log_response_audit(
                        state.audit.as_ref(),
                        session_id.clone(),
                        unified.id.clone(),
                        provider.to_string(),
                        unified.model.clone(),
                        &bytes,
                        &optimized_body,
                    );
                    let body = axum::body::Body::from(bytes);
                    axum::response::Response::from_parts(parts, body)
                }
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    (
                        StatusCode::BAD_GATEWAY,
                        "Gateway error: failed to read response",
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to forward request to {}: {}", provider, e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}

async fn anthropic_messages(State(state): State<ApiState>, body: Bytes) -> Response {
    info!("Received Anthropic messages request");

    let body_str = String::from_utf8_lossy(&body);
    let body_json: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            error!("Failed to parse request body: {}", e);
            return (StatusCode::BAD_REQUEST, "Invalid JSON").into_response();
        }
    };

    let mut unified = anthropic_to_unified(body_json.clone());
    let original_size = serde_json::to_string(&body_json).unwrap_or_default().len();
    state.rtk.optimize(&mut unified);
    let optimized_json = unified_to_anthropic_json(&unified);
    let optimized_body = serde_json::to_string(&optimized_json).unwrap_or_default();
    let optimized_size = optimized_body.len();

    info!(
        "RTK optimized Anthropic request: {} -> {} bytes ({}% reduction)",
        original_size,
        optimized_size,
        if original_size > 0 {
            ((original_size - optimized_size) * 100 / original_size) as i32
        } else {
            0
        }
    );

    let session_id = unified.session_id.clone();
    let mut entry = AuditLogEntry::new(
        session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        resolve_provider(&unified.model).to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    entry.payload = Some(optimized_body.clone());
    entry.agent_type = state.agent_type.lock().unwrap().clone();
    let _ = touch_session(&state.db_path, &session_id, &unified.model);
    state.audit.log(entry);

    let provider = resolve_provider(&unified.model);
    match state.router.forward_anthropic(optimized_body.clone()).await {
        Ok(response) => {
            info!("Successfully forwarded request to {}", provider);
            let (parts, body) = response.into_parts();
            match axum::body::to_bytes(body, usize::MAX).await {
                Ok(bytes) => {
                    log_response_audit(
                        state.audit.as_ref(),
                        session_id.clone(),
                        unified.id.clone(),
                        provider.to_string(),
                        unified.model.clone(),
                        &bytes,
                        &optimized_body,
                    );
                    let body = axum::body::Body::from(bytes);
                    axum::response::Response::from_parts(parts, body)
                }
                Err(e) => {
                    error!("Failed to read response body: {}", e);
                    (
                        StatusCode::BAD_GATEWAY,
                        "Gateway error: failed to read response",
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            error!("Failed to forward request to {}: {}", provider, e);
            (StatusCode::BAD_GATEWAY, format!("Gateway error: {}", e)).into_response()
        }
    }
}

async fn health_check() -> Json<Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "gatewayd",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn reporter_status_handler(State(state): State<ApiState>) -> Json<Value> {
    let cursor = match dh_db::DbManager::open(&state.db_path) {
        Ok(db) => db.get_reporter_cursor().unwrap_or(0),
        Err(_) => 0,
    };
    let (pending, dead) = match dh_db::DbManager::open(&state.db_path) {
        Ok(db) => db.get_queue_stats().unwrap_or((0, 0)),
        Err(_) => (0, 0),
    };

    axum::Json(serde_json::json!({
        "enabled": std::env::var("DH_REPORTER_ENABLED").is_ok(),
        "endpoint": std::env::var("DH_REPORTER_ENDPOINT").ok(),
        "last_sync_rowid": cursor,
        "queue_pending": pending,
        "queue_dead": dead,
    }))
}

// CLI args
#[derive(Parser, Debug)]
#[command(name = "dh-gatewayd")]
#[command(about = "DeepHarness LLM Gateway Daemon")]
struct Args {
    #[arg(long, default_value = "2345")]
    port: u16,

    #[arg(long, default_value = "2346")]
    admin_port: u16,

    #[arg(long)]
    daemon: bool,

    #[arg(long = "agent-type")]
    agent_types: Vec<String>,

    /// Attach an agent plugin on startup (e.g. opencode)
    #[arg(long)]
    attach: Vec<String>,
}

fn init_db<P: AsRef<std::path::Path>>(path: P) -> Result<DbManager, anyhow::Error> {
    let manager = DbManager::open(path)?;
    Ok(manager)
}

fn main() {
    // tracing-subscriber 开启 tracing-log feature 后会自动桥接 log crate，
    // 使 claude-plugin 等依赖 log 的 crate 也能输出到同一 subscriber。
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    if !args.agent_types.is_empty() {
        info!("Auto-start agents: {:?}", args.agent_types);
    }
    info!(
        "Starting gatewayd on port {}, admin on port {}",
        args.port, args.admin_port
    );

    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(e) = rt.block_on(run(args)) {
        eprintln!("gatewayd error: {}", e);
        std::process::exit(1);
    }
}

async fn run(args: Args) -> anyhow::Result<()> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let db_path = data_dir.join("gatewayd.db");
    let db = init_db(&db_path)?;
    let reporter_db = Arc::new(std::sync::Mutex::new(init_db(&db_path)?));

    let (audit_logger, audit_receiver) = AuditLogger::new();
    let audit_storage = AuditStorage::new(db);
    tokio::spawn(run_storage_worker(audit_receiver, audit_storage));

    let reporter_config = reporter::config::ReporterConfig::from_env();
    let reporter_handle = reporter::start(reporter_db, reporter_config);

    let gateway_router = Arc::new(GatewayRouter::new());

    // AG-UI session manager.
    let session_manager = crate::session::SessionManager::new();

    // Initialize agent runtime with AG-UI event sink.
    let event_sink = Arc::new(crate::agui_sink::AguiEventSink::new(
        session_manager.clone(),
    ));
    let agent_service = match agents::init_agent_service_with_sink(event_sink) {
        Ok(service) => {
            info!("AgentService initialized");
            Some(Arc::new(service))
        }
        Err(e) => {
            warn!("Failed to initialize AgentService: {}", e);
            None
        }
    };

    // Auto-attach agents requested via --attach or --agent-type into a default session.
    let attach_types: Vec<String> = args
        .attach
        .iter()
        .chain(args.agent_types.iter())
        .cloned()
        .collect();
    if !attach_types.is_empty() {
        if let Some(ref service) = agent_service {
            let default_session = session_manager.create_session(None);
            for plugin_type in &attach_types {
                match session_manager
                    .create_agent(
                        &default_session,
                        plugin_type,
                        &format!("{}-instance", plugin_type),
                        &std::env::current_dir()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        false,
                        service,
                    )
                    .await
                {
                    Ok(info) => info!("Attached agent: {} (id={})", plugin_type, info.id),
                    Err(e) => warn!("Failed to attach agent {}: {}", plugin_type, e),
                }
            }
        }
    }

    let mcp_registry = match mcp_aggregator::McpRegistry::load_from_db(&db_path).await {
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

    let api_state = ApiState {
        router: gateway_router,
        audit: Arc::new(audit_logger),
        rtk: Arc::new(RtkEngine::default_engine()),
        agent_type: Arc::new(std::sync::Mutex::new(None)),
        db_path: db_path.clone(),
        mcp_registry: mcp_registry.clone(),
        agent_service: agent_service.clone(),
        session_manager: session_manager.clone(),
    };

    let api_router = Router::new()
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/messages", post(anthropic_messages))
        .layer(CorsLayer::permissive())
        .with_state(api_state.clone());

    let mut admin_router = Router::new()
        .route("/health", get(health_check))
        .route("/context", post(set_context))
        .route("/admin/reporter/status", get(reporter_status_handler));

    if mcp_registry.is_some() {
        admin_router = admin_router
            .route("/mcp/servers", get(mcp_aggregator::list_mcp_servers))
            .route("/mcp/tools", get(mcp_aggregator::list_mcp_tools))
            .route(
                "/mcp/tools/{name}/call",
                post(mcp_aggregator::call_mcp_tool),
            );
    }

    // AG-UI session routes
    if agent_service.is_some() {
        admin_router = admin_router
            .route(
                "/sessions",
                post(crate::handlers::session::create_session_handler),
            )
            .route(
                "/sessions/{session_id}/agents",
                post(crate::handlers::session::create_agent_handler),
            )
            .route(
                "/sessions/{session_id}/events",
                get(crate::handlers::websocket::session_events_handler),
            )
            .route(
                "/sessions/{session_id}/chat",
                post(crate::handlers::sse::chat_handler),
            );
    }

    let admin_router = admin_router.with_state(api_state.clone());

    // 启动空闲实例回收后台任务：定期扫描 session，回收超过 expired_time 无用户输入的实例。
    if let Some(ref service) = agent_service {
        let reap_service = Arc::clone(service);
        let reap_manager = session_manager.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(REAPER_INTERVAL_SECS));
            // 跳过首次立即触发，避免启动瞬间就执行回收。
            interval.tick().await;
            loop {
                interval.tick().await;
                reap_manager.reap_expired(&reap_service).await;
            }
        });
    }

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let admin_addr = SocketAddr::from(([127, 0, 0, 1], args.admin_port));

    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await?;
    info!("Admin API listening on http://{}", admin_addr);
    tokio::spawn(async move {
        if let Err(e) = axum::serve(admin_listener, admin_router).await {
            warn!("Admin server error: {}", e);
        }
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("API server listening on http://{}", addr);
    info!(
        "OpenAI compatible endpoint: http://{}/v1/chat/completions",
        addr
    );
    info!("Anthropic compatible endpoint: http://{}/v1/messages", addr);

    let pid = std::process::id();
    let _ = dh_platform::fs::write_lock_file(pid);
    info!("Lock file written with PID: {}", pid);

    axum::serve(listener, api_router).await?;

    let _ = dh_platform::fs::remove_lock_file();

    if let Some(handle) = reporter_handle {
        handle.shutdown().await;
    }

    Ok(())
}
