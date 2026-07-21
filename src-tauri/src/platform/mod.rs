//! Platform integration module.
//!
//! Connects the desktop app to a remote platform for:
//! - **Config fetch**: retrieve agent/model/skill/feature config at startup.
//! - **Reporting**: push session info, agent status, monitoring data, and
//!   Agent Runtime status.
//!
//! Configuration is read from the `[platform]` section of
//! `~/.config/dh/config.toml` (via the `dh-config` crate). The runtime id
//! used for Agent Runtime status reporting is auto-generated as a UUID and
//! persisted in the application data directory when not explicitly configured.
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
//!                                                                     ├── AgentService ──> POST /api/v1/agent-runtimes/{runtimeId}/status
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
pub mod ready_state;
pub mod reporter;
pub mod runtime_status;
pub mod sink;

pub use client::PlatformClient;
pub use ready_state::WorkspacePathReadiness;
pub use reporter::{ReportEvent, Reporter};
pub use runtime_status::RuntimeStatusCollector;
pub use sink::ReportingEventSink;

use agent_core::event_sink::EventSink;
use dh_config::{load_global, PlatformConfig};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// File name used to persist the auto-generated runtime id in the application
/// data directory.
const RUNTIME_ID_FILE: &str = ".dh-runtime-id";

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
    let reporting_sink: Arc<dyn EventSink> = Arc::new(ReportingEventSink::new(ws_sink, tx));
    Some((reporting_sink, rx))
}

/// Phase 2: spawns the background reporter task.
///
/// The reporter fetches the remote platform config on startup (best-effort),
/// then enters the event-driven + periodic-batch loop.
///
/// The shared `readiness` tracker is updated every time a runtime status
/// report completes (success or failure) and is read by the gateway to gate
/// user-facing operations.
pub fn start_reporter(
    config: &PlatformConfig,
    app_data_dir: &Path,
    rx: mpsc::UnboundedReceiver<ReportEvent>,
    db_conn: Arc<Mutex<rusqlite::Connection>>,
    agent_service: Arc<crate::service::agent_service::AgentService>,
    readiness: Arc<WorkspacePathReadiness>,
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

    // Resolve the runtime id: prefer the config value, otherwise load or
    // generate a UUID and persist it for stable identity across restarts.
    let runtime_id = resolve_runtime_id(app_data_dir, config);
    let mut config = config.clone();
    config.runtime_id = Some(runtime_id);

    let repository = dh_db::desktop::AppRepository::new(db_conn);
    let reporter = Reporter::new(
        client,
        repository,
        agent_service,
        rx,
        report_interval,
        sanitize,
        config,
        readiness,
    );

    tauri::async_runtime::spawn(async move {
        reporter.run().await;
    });

    log::info!("[Platform] Reporter started");
}

/// Resolves the runtime id to use for Agent Runtime status reporting.
///
/// Priority:
/// 1. `runtime_id` from `[platform]` config if non-empty.
/// 2. Previously persisted id in `app_data_dir/.dh-runtime-id`.
/// 3. Newly generated UUID v4, persisted for future restarts.
fn resolve_runtime_id(app_data_dir: &Path, config: &PlatformConfig) -> String {
    if let Some(id) = config.runtime_id.as_ref().filter(|id| !id.trim().is_empty()) {
        return id.clone();
    }

    let path = runtime_id_path(app_data_dir);
    if let Ok(contents) = std::fs::read_to_string(&path) {
        let id = contents.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = persist_runtime_id(&path, &id) {
        log::warn!("[Platform] Failed to persist runtime id: {e}");
    }
    id
}

/// Returns the path to the runtime id persistence file.
fn runtime_id_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(RUNTIME_ID_FILE)
}

/// Writes the runtime id to disk, creating parent directories as needed.
fn persist_runtime_id(path: &Path, id: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn resolve_runtime_id_uses_config_value() {
        let dir = tempfile::tempdir().unwrap();
        let config = PlatformConfig {
            runtime_id: Some("configured-id".to_string()),
            ..PlatformConfig::default()
        };
        assert_eq!(resolve_runtime_id(dir.path(), &config), "configured-id");
    }

    #[test]
    fn resolve_runtime_id_persists_generated_uuid() {
        let dir = tempfile::tempdir().unwrap();
        let config = PlatformConfig::default();

        let id1 = resolve_runtime_id(dir.path(), &config);
        assert!(!id1.is_empty());
        assert!(dir.path().join(RUNTIME_ID_FILE).exists());

        // A second call should return the same persisted id.
        let id2 = resolve_runtime_id(dir.path(), &config);
        assert_eq!(id1, id2);
    }

    #[test]
    fn resolve_runtime_id_loads_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(RUNTIME_ID_FILE);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(b"existing-id\n")
            .unwrap();

        let config = PlatformConfig::default();
        assert_eq!(resolve_runtime_id(dir.path(), &config), "existing-id");
    }
}
