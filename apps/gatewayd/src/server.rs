use crate::audit::{AuditLogger, AuditStorage};
use crate::gateway::GatewayRouter;
use crate::handlers::{chat, context, health};
use crate::rtk::RtkEngine;
use crate::ApiState;
use axum::{Router, middleware, routing::get, routing::post, routing::put};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tracing::{info, warn};

/// 空闲实例回收任务的扫描间隔（秒）。
const REAPER_INTERVAL_SECS: u64 = 60;

const ENV_CORS_ORIGINS: &str = "GATEWAYD_CORS_ORIGINS";
const DEFAULT_CORS_ORIGINS: &[&str] = &["http://localhost:5173", "http://127.0.0.1:5173"];

pub(crate) const MAX_RESPONSE_BODY_BYTES: usize = 10 * 1024 * 1024; // 10 MiB
const MAX_REQUEST_BODY_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Build a CorsLayer from the comma-separated `GATEWAYD_CORS_ORIGINS` environment
/// variable, falling back to localhost dev origins.
pub(crate) fn build_cors_layer() -> CorsLayer {
    let origins: Vec<_> = std::env::var(ENV_CORS_ORIGINS)
        .ok()
        .into_iter()
        .flat_map(|s| s.split(',').map(|o| o.trim().to_string()).collect::<Vec<_>>())
        .chain(DEFAULT_CORS_ORIGINS.iter().map(|o| o.to_string()))
        .filter(|o| !o.is_empty())
        .map(|o| {
            o.parse::<axum::http::HeaderValue>()
                .unwrap_or_else(|_| "http://localhost:5173".parse().unwrap())
        })
        .collect();

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(AllowMethods::list(vec![
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ]))
        .allow_headers(AllowHeaders::list(vec![
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]))
}

/// Create application state from CLI arguments and a data directory.
///
/// This is extracted from `run()` so integration tests can construct the same
/// state without starting a network server.
pub(crate) async fn create_state(
    args: &crate::Args,
    data_dir: PathBuf,
) -> anyhow::Result<(ApiState, Option<crate::reporter::ReporterHandle>)> {
    let db_path = data_dir.join("gatewayd.db");
    let reporter_db = Arc::new(Mutex::new(crate::init_db(&db_path)?));
    let api_key = crate::auth::ApiKeyStore::load_or_create(&db_path);

    let (audit_logger, audit_receiver) = AuditLogger::new();
    let audit_storage = AuditStorage::new(db_path.clone());
    tokio::spawn(crate::audit::run_storage_worker(audit_receiver, audit_storage));

    let reporter_config = crate::reporter::config::ReporterConfig::from_env();
    let reporter_handle = crate::reporter::start(reporter_db, reporter_config);

    let gateway_router = Arc::new(GatewayRouter::new());

    // Determine whether platform reporting will be enabled *before*
    // building the session manager, so we can wire the readiness gate
    // into the manager up front (no later swap, no risk of stale
    // references in the event_sink / agent_service).
    let platform_active =
        crate::runtime_reporter::is_platform_reporting_configured();
    let shared_readiness: Arc<crate::readiness::WorkspacePathReadiness> = if platform_active {
        info!(
            "[server] Platform reporting enabled; \
             workspace_path readiness gate starts CLOSED; \
             create_agent will be held until the first successful sync"
        );
        Arc::new(crate::readiness::WorkspacePathReadiness::new(true))
    } else {
        info!(
            "[server] Platform reporting not configured; \
             workspace_path readiness gate is OPEN (local mode)"
        );
        Arc::new(crate::readiness::WorkspacePathReadiness::local_mode())
    };

    // AG-UI session manager — wired to the shared readiness gate so it
    // holds `create_agent` calls when the platform has not yet confirmed
    // a workspace path.
    let session_manager =
        crate::session::SessionManager::with_readiness(shared_readiness.clone());

    // Initialize agent runtime with AG-UI event sink.
    let event_sink = Arc::new(crate::agui_sink::AguiEventSink::new(
        session_manager.clone(),
        tokio::runtime::Handle::current(),
    ));
    let agent_service = match crate::agents::init_agent_service_with_sink(event_sink) {
        Ok(service) => {
            info!("AgentService initialized");
            Some(Arc::new(service))
        }
        Err(e) => {
            warn!("Failed to initialize AgentService: {}", e);
            None
        }
    };

    // Start the DeepHarness platform runtime status reporter if configured.
    // The reporter is given the *same* readiness tracker so the session
    // manager and the reporter share one source of truth for "have we
    // received a workspace path yet?".
    let _runtime_reporter_handle = crate::runtime_reporter::start_runtime_reporter_with_readiness(
        agent_service.clone(),
        Some(db_path.clone()),
        shared_readiness,
    );

    // Auto-attach agents requested via --attach or --agent-type into a default session.
    let attach_types: Vec<String> = args
        .attach
        .iter()
        .chain(args.agent_types.iter())
        .cloned()
        .collect();
    if !attach_types.is_empty() {
        if let Some(ref service) = agent_service {
            let default_session = session_manager.create_session(None, None).await;
            for plugin_type in &attach_types {
                match session_manager
                    .create_agent(
                        &default_session,
                        plugin_type,
                        &format!("{}-instance", plugin_type),
                        std::env::current_dir()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .as_ref(),
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

    // Pre-warm: 为最近使用的 N 个 session 预热 opencode 实例，
    // 消除 gatewayd 重启后的首次请求冷启动延迟。
    const PREWARM_LIMIT: usize = 3;
    const PREWARM_CONCURRENCY: usize = 2;
    if let Some(ref service) = agent_service {
        let prewarm_mgr = session_manager.clone();
        let prewarm_svc = service.clone();
        tokio::spawn(async move {
            let recent = prewarm_mgr.recent_sessions(PREWARM_LIMIT).await;
            if recent.is_empty() {
                info!("[prewarm] no recent sessions to pre-warm");
                return;
            }
            info!("[prewarm] pre-warming {} recent sessions", recent.len());

            let semaphore = Arc::new(tokio::sync::Semaphore::new(PREWARM_CONCURRENCY));
            let mut handles = Vec::new();
            for (sid, workspace) in &recent {
                let sid = sid.clone();
                let workspace = workspace.clone();
                let mgr = prewarm_mgr.clone();
                let svc = prewarm_svc.clone();
                let permit = match semaphore.clone().acquire_owned().await {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("[prewarm] failed to acquire semaphore: {}", e);
                        continue;
                    }
                };
                handles.push(tokio::spawn(async move {
                    let _permit = permit;
                    match mgr
                        .create_agent(&sid, "opencode", "opencode-auto", &workspace, false, &svc)
                        .await
                    {
                        Ok(info) => {
                            info!("[prewarm] session={} agent ready instance={}", sid, info.id);
                        }
                        Err(e) => {
                            warn!("[prewarm] session={} failed: {}", sid, e);
                        }
                    }
                }));
            }

            for handle in handles {
                if let Err(e) = handle.await {
                    warn!("[prewarm] task join error: {}", e);
                }
            }

            info!(
                "[prewarm] done, pre-warmed {}/{} sessions",
                {
                    let mut count = 0;
                    for (sid, _) in &recent {
                        if let Some(s) = prewarm_mgr.get_session(sid).await {
                            if !s.instances().is_empty() {
                                count += 1;
                            }
                        }
                    }
                    count
                },
                recent.len()
            );
        });
    }

    let mcp_registry = match crate::mcp_aggregator::McpRegistry::load_from_db(&db_path).await {
        Ok(registry) => {
            if registry.is_empty() {
                info!("No MCP servers configured");
            }
            Some(Arc::new(Mutex::new(registry)))
        }
        Err(e) => {
            warn!("Failed to load MCP registry: {}", e);
            None
        }
    };

    Ok((
        ApiState {
            router: gateway_router,
            audit: Arc::new(audit_logger),
            rtk: Arc::new(RtkEngine::default_engine()),
            agent_type: Arc::new(std::sync::Mutex::new(None)),
            db_path: db_path.clone(),
            mcp_registry: mcp_registry.clone(),
            agent_service: agent_service.clone(),
            session_manager: session_manager.clone(),
            ws_connections: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            api_key,
        },
        reporter_handle,
    ))
}

/// Build the main API router including OpenAI/Anthropic compatible endpoints.
pub fn build_api_router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/chat/completions", post(chat::openai_chat_completions))
        .route("/v1/messages", post(chat::anthropic_messages))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            state.api_key.clone(),
            crate::auth::auth_middleware,
        ))
        .layer(build_cors_layer())
        .with_state(state)
}

/// Build the admin router including health checks, AG-UI sessions, and MCP APIs.
pub fn build_admin_router(state: ApiState) -> Router {
    let mut admin_router = Router::new()
        .route("/health", get(health::health_check))
        .route("/context", post(context::set_context))
        .route("/admin/reporter/status", get(health::reporter_status_handler))
        .route("/admin/auth/rotate", post(health::rotate_api_key_handler));

    if state.mcp_registry.is_some() {
        admin_router = admin_router
            .route("/mcp/servers", get(crate::mcp_aggregator::list_mcp_servers))
            .route("/mcp/tools", get(crate::mcp_aggregator::list_mcp_tools))
            .route(
                "/mcp/tools/{name}/call",
                post(crate::mcp_aggregator::call_mcp_tool),
            );
    }

    // AG-UI session routes
    if state.agent_service.is_some() {
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
            )
            .route(
                "/sessions/{session_id}/agents/{agent_id}/config",
                put(crate::handlers::agent::update_agent_config_handler),
            )
            .route(
                "/sessions/{session_id}/agents/{agent_id}/respond",
                post(crate::handlers::agent::respond_handler),
            );
    }

    admin_router.with_state(state)
}

/// Build routers for integration tests, using a temporary data directory.
pub async fn build_test_app() -> anyhow::Result<(Router, Router)> {
    let data_dir = std::env::temp_dir().join(format!("dh-gatewayd-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir)?;

    // Redirect both gatewayd data dir helpers to the temp directory.
    std::env::set_var("GATEWAYD_DATA_DIR", &data_dir);
    std::env::set_var("XDG_DATA_HOME", &data_dir);

    let args = crate::Args {
        port: 0,
        admin_port: 0,
        daemon: false,
        agent_types: Vec::new(),
        attach: Vec::new(),
    };
    let (state, _reporter_handle) = create_state(&args, data_dir).await?;
    Ok((build_api_router(state.clone()), build_admin_router(state)))
}

pub async fn run(args: crate::Args) -> anyhow::Result<()> {
    let data_dir = dh_platform::fs::ensure_data_dir()?;
    let (state, reporter_handle) = create_state(&args, data_dir).await?;

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    let admin_addr = SocketAddr::from(([127, 0, 0, 1], args.admin_port));

    let admin_listener = tokio::net::TcpListener::bind(admin_addr).await?;
    info!("Admin API listening on http://{}", admin_addr);

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

    // 启动空闲实例回收后台任务：定期扫描 session，回收超过 expired_time 无用户输入的实例。
    if let Some(ref service) = state.agent_service {
        let reap_service = Arc::clone(service);
        let reap_manager = state.session_manager.clone();
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

    let api_router = build_api_router(state.clone());
    let admin_router = build_admin_router(state.clone());

    // Run both servers and wait for the shutdown signal.  The shutdown signal
    // is installed on both so that either one stopping triggers the other.
    let main_server = axum::serve(listener, api_router).with_graceful_shutdown(crate::shutdown_signal());
    let admin_server =
        axum::serve(admin_listener, admin_router).with_graceful_shutdown(crate::shutdown_signal());

    let (main_result, admin_result) = tokio::join!(main_server, admin_server);
    main_result?;
    admin_result?;

    info!("Servers stopped, beginning cleanup");

    let _ = dh_platform::fs::remove_lock_file();

    if let Some(service) = state.agent_service {
        const SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
        service.stop_all_instances_with_timeout(SHUTDOWN_TIMEOUT).await;
    }

    if let Some(registry) = state.mcp_registry {
        let registry = registry.lock().await;
        registry.shutdown().await;
    }

    if let Some(handle) = reporter_handle {
        handle.shutdown().await;
    }

    info!("Cleanup complete, exiting");
    Ok(())
}
