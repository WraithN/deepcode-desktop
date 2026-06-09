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

// Server state (inlined)
#[derive(Clone)]
struct ApiState {
    router: Arc<GatewayRouter>,
    audit: Arc<AuditLogger>,
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

    let unified = openai_to_unified(body_json.clone());

    let mut entry = AuditLogEntry::new(
        unified.session_id.clone(),
        unified.id.clone(),
        Direction::Request,
        "openai".to_string(),
        unified.model.clone(),
    );
    entry.payload_size_bytes = body.len();
    state.audit.log(entry);

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

    let unified = anthropic_to_unified(body_json.clone());

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
