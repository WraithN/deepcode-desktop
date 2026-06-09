use axum::{
    body::{Body, Bytes},
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use clap::Parser;
use dh_core::{AuditLogEntry, Direction, Message, Provider, Role, UnifiedRequest};
use dh_db::DbManager;
use reqwest::Client;
use rusqlite::params;
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

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

fn anthropic_to_unified(body: Value) -> UnifiedRequest {
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

// Gateway router (inlined)
struct GatewayRouter {
    client: Client,
    openai_api_key: Option<String>,
    anthropic_api_key: Option<String>,
}

impl GatewayRouter {
    fn new() -> Self {
        Self {
            client: Client::new(),
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
        }
    }

    async fn forward_openai(&self, body: String) -> Result<Response, anyhow::Error> {
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

    async fn forward_anthropic(&self, body: String) -> Result<Response, anyhow::Error> {
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
            summary_parts.push(format!("[{}:{}] {}", i, prefix, &msg.content[..msg.content.len().min(80)]));
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
            total, req.messages.len(), summarize_count
        );
    }

    fn compress_prompts(&self, req: &mut UnifiedRequest) {
        for msg in &mut req.messages {
            let original_len = msg.content.len();
            msg.content = msg.content
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            msg.content = msg.content.replace("\n\n\n", "\n\n");
            if msg.content.len() < original_len {
                info!(
                    "RTK compressed message: {} -> {} bytes",
                    original_len, msg.content.len()
                );
            }
        }
    }
}

fn unified_to_openai_json(req: &UnifiedRequest) -> Value {
    let messages: Vec<Value> = req.messages.iter().map(|m| {
        serde_json::json!({
            "role": match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            },
            "content": m.content
        })
    }).collect();

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
    let messages: Vec<Value> = req.messages.iter().filter_map(|m| {
        match m.role {
            Role::System => None,
            _ => Some(serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content
            })),
        }
    }).collect();

    let system_prompt = req.messages.iter()
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
}

async fn openai_chat_completions(
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
        } else { 0 }
    );

    let mut entry = AuditLogEntry::new(
        unified.session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        "openai".to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    state.audit.log(entry);

    match state.router.forward_openai(optimized_body).await {
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

async fn anthropic_messages(
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
        } else { 0 }
    );

    let mut entry = AuditLogEntry::new(
        unified.session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        "anthropic".to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = optimized_size;
    state.audit.log(entry);

    match state.router.forward_anthropic(optimized_body).await {
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

async fn health_check() -> Json<Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "gatewayd",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

// CLI args
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

fn init_db<P: AsRef<std::path::Path>>(path: P) -> Result<DbManager, anyhow::Error> {
    let manager = DbManager::open(path)?;
    Ok(manager)
}

fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    info!("Starting gatewayd on port {}, admin on port {}", args.port, args.admin_port);

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

    let (audit_logger, audit_receiver) = AuditLogger::new();
    let audit_storage = AuditStorage::new(db);
    tokio::spawn(run_storage_worker(audit_receiver, audit_storage));

    let gateway_router = Arc::new(GatewayRouter::new());

    let api_state = ApiState {
        router: gateway_router,
        audit: Arc::new(audit_logger),
        rtk: Arc::new(RtkEngine::default_engine()),
    };

    let api_router = Router::new()
        .route("/v1/chat/completions", post(openai_chat_completions))
        .route("/v1/messages", post(anthropic_messages))
        .layer(CorsLayer::permissive())
        .with_state(api_state.clone());

    let admin_router = Router::new()
        .route("/health", get(health_check))
        .with_state(api_state.clone());

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
    info!("OpenAI compatible endpoint: http://{}/v1/chat/completions", addr);
    info!("Anthropic compatible endpoint: http://{}/v1/messages", addr);

    let pid = std::process::id();
    let _ = dh_platform::fs::write_lock_file(pid);
    info!("Lock file written with PID: {}", pid);

    axum::serve(listener, api_router).await?;

    let _ = dh_platform::fs::remove_lock_file();

    Ok(())
}
