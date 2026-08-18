use clap::Parser;
use dh_db::DbManager;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use tracing::info;

mod agents {
    #![allow(dead_code)]
    include!("agents_impl.rs");
}
pub mod agui;
pub mod agui_sink;
pub mod auth;
pub mod audit;
pub mod gateway;
pub mod handlers;
pub mod mcp_aggregator;
pub mod mcp_proxy_server;
pub mod readiness;
pub mod reporter;
pub mod runtime_reporter;
pub mod rtk;
pub mod server;
pub mod session;
pub mod workspace;

/// Shared application state used by route handlers and server runtime.
#[derive(Clone)]
pub struct ApiState {
    pub(crate) router: Arc<crate::gateway::GatewayRouter>,
    pub(crate) audit: Arc<crate::audit::AuditLogger>,
    pub(crate) rtk: Arc<crate::rtk::RtkEngine>,
    pub(crate) agent_type: Arc<Mutex<Option<String>>>,
    pub(crate) db_path: PathBuf,
    pub(crate) mcp_registry: Option<Arc<tokio::sync::Mutex<mcp_aggregator::McpRegistry>>>,
    pub(crate) agent_service: Option<Arc<agents::AgentService>>,
    pub(crate) session_manager: crate::session::SessionManager,
    pub(crate) ws_connections: Arc<AtomicUsize>,
    pub(crate) api_key: crate::auth::ApiKeyStore,
    /// crawler `web_scrape.maxDepth` 的平台默认值；Task 8 启动后由 dh-backend
    /// 拉取的配置覆盖，启动期使用 `MCP_DEFAULT_MAX_DEPTH` 占位。
    pub(crate) crawler_max_depth: Arc<std::sync::atomic::AtomicI64>,
}

/// CLI arguments for the gateway daemon.
#[derive(Parser, Debug)]
#[command(name = "dh-gatewayd")]
#[command(about = "DeepHarness LLM Gateway Daemon")]
pub struct Args {
    #[arg(long, default_value = "2345")]
    pub port: u16,

    #[arg(long, default_value = "2346")]
    pub admin_port: u16,

    #[arg(long)]
    pub daemon: bool,

    #[arg(long = "agent-type")]
    pub agent_types: Vec<String>,

    /// Attach an agent plugin on startup (e.g. opencode)
    #[arg(long)]
    pub attach: Vec<String>,
}

/// Initialize the database at the given path.
pub fn init_db<P: AsRef<std::path::Path>>(path: P) -> Result<DbManager, anyhow::Error> {
    let manager = DbManager::open(path)?;
    Ok(manager)
}

/// Waits for a shutdown signal (SIGINT/SIGTERM) and logs the event.
pub(crate) async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    info!("Received shutdown signal, starting graceful shutdown");
}
