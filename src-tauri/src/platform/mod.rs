//! Platform integration module.
//!
//! Connects the desktop app to a remote platform for:
//! - **Config fetch**: retrieve agent/model/skill/feature config at startup.
//! - **Reporting**: push session info, agent status, and monitoring data.
//!
//! Configuration is read from the `[platform]` section of
//! `~/.config/dh/config.toml` (via the `dh-config` crate).
//!
//! ## Architecture
//!
//! ```text
//! AgentService ──> ReportingEventSink ──┬──> WebSocketEventSink ──> Frontend (WS JSON-RPC)
//!                                       └──> mpsc channel ──> Reporter (background task)
//!                                                                ├── event: agent status ──> POST /api/report/agents
//!                                                                ├── event: session log ──> buffer
//!                                                                └── timer: periodic flush
//!                                                                     ├── DB query ──> POST /api/report/sessions
//!                                                                     ├── AgentService ──> POST /api/report/agents
//!                                                                     └── buffer ──> POST /api/report/monitoring
//! ```
//!
//! ## Initialisation (two-phase)
//!
//! Because the [`ReportingEventSink`] needs an mpsc sender while the
//! [`Reporter`] needs an [`AgentService`] handle (which itself needs the
//! sink), initialisation is split into two phases:
//!
//! 1. **[`create_reporting_sink`]** – called *before* `AgentService` is
//!    created. Returns the wrapped sink and the receiver half of the channel.
//! 2. **[`start_reporter`]** – called *after* `AgentService` is created.
//!    Consumes the receiver and spawns the background reporter task.

pub mod client;
pub mod payload;
pub mod reporter;
pub mod sink;

pub use client::PlatformClient;
pub use reporter::{ReportEvent, Reporter};
pub use sink::ReportingEventSink;

use agent_core::event_sink::EventSink;
use dh_config::{load_global, PlatformConfig};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// Loads the platform config from the global dh config file
/// (`~/.config/dh/config.toml`).
///
/// Returns `None` if the file is missing or the platform section is inactive.
pub fn load_platform_config() -> Option<PlatformConfig> {
    let loaded = load_global().ok()??;
    let platform = loaded.platform;
    if platform.is_active() {
        Some(platform)
    } else {
        None
    }
}

/// Phase 1: wraps the WebSocket sink in a [`ReportingEventSink`] and returns
/// the channel receiver for the reporter.
///
/// Returns `None` (and logs an error) if the HTTP client cannot be built,
/// in which case the caller should fall back to the original WS sink.
pub fn create_reporting_sink(
    config: &PlatformConfig,
    ws_sink: Arc<dyn EventSink>,
) -> Option<(Arc<dyn EventSink>, mpsc::UnboundedReceiver<ReportEvent>)> {
    // Validate that the HTTP client can be built before wrapping.
    if PlatformClient::new(config).is_err() {
        log::error!("[Platform] HTTP client creation failed, skipping reporter");
        return None;
    }

    let (tx, rx) = mpsc::unbounded_channel::<ReportEvent>();
    let reporting_sink: Arc<dyn EventSink> =
        Arc::new(ReportingEventSink::new(ws_sink, tx));
    Some((reporting_sink, rx))
}

/// Phase 2: spawns the background reporter task.
///
/// The reporter fetches the remote platform config on startup (best-effort),
/// then enters the event-driven + periodic-batch loop.
pub fn start_reporter(
    config: &PlatformConfig,
    rx: mpsc::UnboundedReceiver<ReportEvent>,
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    agent_service: Arc<crate::service::agent_service::AgentService>,
) {
    let sanitize = config.sanitize;
    let report_interval = std::time::Duration::from_secs(config.report_interval());

    let client = match PlatformClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            log::error!("[Platform] Failed to create HTTP client for reporter: {e}");
            return;
        }
    };

    let repository = dh_db::desktop::AppRepository::new(db_conn);
    let reporter = Reporter::new(
        client,
        repository,
        agent_service,
        rx,
        report_interval,
        sanitize,
    );

    tauri::async_runtime::spawn(async move {
        reporter.run().await;
    });

    log::info!("[Platform] Reporter started");
}
