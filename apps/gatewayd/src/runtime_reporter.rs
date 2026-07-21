//! Runtime status reporter for the server-side gateway.
// rustc 1.95 panics while rendering dead-code warnings in this module
// (annotate_snippets slice index mismatch). Allow the lint locally until
// the compiler is upgraded to a fixed version.
#![allow(dead_code)]
//!
//! Reads platform configuration from `~/.config/dh/config.toml` (via `dh-config`)
//! and periodically POSTs the gateway runtime status to:
//!
//! ```text
//! POST {platform_url}/api/v1/agent-runtimes/{runtime_id}/status
//! ```
//!
//! This allows a k8s-deployed `dh-gatewayd` to be monitored from the
//! DeepHarness Enterprise Platform without running the desktop application.

use agent_core::models::InstanceInfo;
use agent_core::service::AgentService;
use chrono::{DateTime, Utc};
use rusqlite::OpenFlags;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};
use tracing::{info, warn};

/// Maximum sensible CPU percentage to report.
const MAX_CPU_PERCENT: f64 = 100.0;

/// Environment variable overrides for containerised deployments.
const ENV_PLATFORM_URL: &str = "DH_PLATFORM_URL";
const ENV_API_KEY: &str = "DH_PLATFORM_API_KEY";
const ENV_ENABLED: &str = "DH_PLATFORM_ENABLED";
const ENV_WORKSPACE_ID: &str = "DH_PLATFORM_WORKSPACE_ID";
const ENV_USER_ID: &str = "DH_PLATFORM_USER_ID";
const ENV_RUNTIME_ID: &str = "DH_PLATFORM_RUNTIME_ID";
const ENV_REPORT_INTERVAL: &str = "DH_PLATFORM_REPORT_INTERVAL_SECS";

/// Runtime status report payload (snake_case to match the platform API).
#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeStatusReport {
    workspace_id: String,
    user_id: String,
    status: String,
    uptime_seconds: u64,
    cpu_percent: f64,
    mem_percent: f64,
    sandbox_spec: String,
    agents: Vec<RuntimeAgentStatus>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RuntimeAgentStatus {
    #[serde(rename = "type")]
    agent_type: String,
    name: String,
    status: String,
    calls_today: u64,
    version: String,
    last_active: String,
}

/// Effective reporter configuration after merging file + env overrides.
pub(crate) struct Config {
    url: String,
    api_key: String,
    runtime_id: String,
    workspace_id: String,
    user_id: String,
    report_interval: Duration,
}

impl Config {
    /// Returns true when the reporter has everything it needs to run.
    fn is_active(&self) -> bool {
        !self.url.is_empty()
            && !self.api_key.is_empty()
            && !self.runtime_id.is_empty()
            && !self.workspace_id.is_empty()
    }
}

/// Loads configuration from the global dh config file, then applies environment
/// variable overrides so containerised deployments can avoid baking secrets into
/// the filesystem config.
pub(crate) fn load_config() -> Option<Config> {
    let mut cfg = dh_config::PlatformConfig::default();

    if let Some(file_cfg) = dh_config::load_global().ok().flatten().map(|c| c.platform) {
        if file_cfg.is_active() {
            cfg = file_cfg;
        }
    }

    // Environment variables always win.
    if let Ok(v) = std::env::var(ENV_ENABLED) {
        cfg.enabled = v.parse().unwrap_or(cfg.enabled);
    }
    if !cfg.enabled {
        return None;
    }
    if let Ok(v) = std::env::var(ENV_PLATFORM_URL) {
        cfg.url = Some(v);
    }
    if let Ok(v) = std::env::var(ENV_API_KEY) {
        cfg.api_key = Some(v);
    }
    if let Ok(v) = std::env::var(ENV_WORKSPACE_ID) {
        cfg.workspace_id = Some(v);
    }
    if let Ok(v) = std::env::var(ENV_USER_ID) {
        cfg.user_id = Some(v);
    }
    if let Ok(v) = std::env::var(ENV_RUNTIME_ID) {
        cfg.runtime_id = Some(v);
    }
    if let Ok(v) = std::env::var(ENV_REPORT_INTERVAL) {
        if let Ok(n) = v.parse::<u64>() {
            cfg.report_interval_secs = Some(n);
        }
    }

    let url = cfg.url.as_ref().filter(|u| !u.trim().is_empty()).cloned()?;
    let api_key = cfg.api_key.clone().unwrap_or_default();
    let workspace_id = cfg.workspace_id.clone().unwrap_or_default();
    let user_id = cfg.user_id.clone().unwrap_or_default();
    if api_key.is_empty() || workspace_id.is_empty() {
        return None;
    }

    let report_interval = Duration::from_secs(cfg.report_interval());
    let runtime_id = cfg
        .runtime_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
        });

    Some(Config {
        url,
        api_key,
        runtime_id,
        workspace_id,
        user_id,
        report_interval,
    })
}

/// Starts the background runtime status reporter.
///
/// Returns `None` when platform reporting is not configured/enabled.
pub fn start_runtime_reporter(
    agent_service: Option<Arc<AgentService>>,
    db_path: Option<PathBuf>,
) -> Option<tokio::task::JoinHandle<()>> {
    let cfg = match load_config() {
        Some(c) => c,
        None => {
            info!("[RuntimeReporter] Platform reporting not configured, skipping");
            return None;
        }
    };

    info!(
        "[RuntimeReporter] Starting for runtime_id={} to {}",
        cfg.runtime_id, cfg.url
    );

    let handle = tokio::spawn(async move {
        let start_time = Instant::now();
        let client = reqwest::Client::new();

        loop {
            tokio::time::sleep(cfg.report_interval).await;

            let report = build_report(&cfg, agent_service.as_ref(), db_path.as_ref(), start_time).await;
            let url = format!("{}/api/v1/agent-runtimes/{}/status", cfg.url, cfg.runtime_id);

            match client
                .post(&url)
                .header("Authorization", format!("Bearer {}", cfg.api_key))
                .header("Content-Type", "application/json")
                .json(&report)
                .timeout(Duration::from_secs(30))
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        info!("[RuntimeReporter] Status reported successfully");
                    } else {
                        let status = resp.status();
                        let body = resp.text().await.unwrap_or_default();
                        warn!("[RuntimeReporter] Report failed: {} {}", status, body);
                    }
                }
                Err(e) => {
                    warn!("[RuntimeReporter] Request failed: {}", e);
                }
            }
        }
    });

    Some(handle)
}

/// Builds the runtime status report from config, system metrics and agent instances.
async fn build_report(
    cfg: &Config,
    agent_service: Option<&Arc<AgentService>>,
    db_path: Option<&PathBuf>,
    start_time: Instant,
) -> RuntimeStatusReport {
    let (cpu_percent, mem_percent, sandbox_spec) = collect_metrics();

    let agents: Vec<RuntimeAgentStatus> = match agent_service {
        Some(svc) => svc
            .list_instances()
            .await
            .into_iter()
            .map(|info| map_instance(info, db_path))
            .collect(),
        None => Vec::new(),
    };

    RuntimeStatusReport {
        workspace_id: cfg.workspace_id.clone(),
        user_id: cfg.user_id.clone(),
        status: "running".to_string(),
        uptime_seconds: start_time.elapsed().as_secs(),
        cpu_percent,
        mem_percent,
        sandbox_spec,
        agents,
    }
}

/// Collects CPU and memory usage for the current process, plus a sandbox spec.
pub(crate) fn collect_metrics() -> (f64, f64, String) {
    let mut system = System::new_all();
    system.refresh_all();

    let sandbox_spec = detect_sandbox_spec(&system);
    let current_pid = Pid::from(std::process::id() as usize);
    let Some(process) = system.process(current_pid) else {
        return (0.0, 0.0, sandbox_spec);
    };

    let cpu_percent = (process.cpu_usage() as f64).clamp(0.0, MAX_CPU_PERCENT);
    let total_memory = system.total_memory();
    let mem_percent = if total_memory == 0 {
        0.0
    } else {
        (process.memory() as f64 / total_memory as f64) * 100.0
    };

    (cpu_percent, mem_percent, sandbox_spec)
}

/// Detects a sandbox spec string like "4C / 16G" from system information.
pub(crate) fn detect_sandbox_spec(system: &System) -> String {
    let cpu_count = system.physical_core_count().unwrap_or_else(|| system.cpus().len());
    let memory_gb = system.total_memory() / 1024 / 1024 / 1024;
    format!("{}C / {}G", cpu_count, memory_gb)
}

/// Maps an agent-core instance info to the platform agent status payload.
fn map_instance(info: InstanceInfo, db_path: Option<&PathBuf>) -> RuntimeAgentStatus {
    let (calls_today, last_active) = db_path
        .and_then(|path| collect_agent_stats(path, &info.agent_key).ok())
        .unwrap_or_default();

    RuntimeAgentStatus {
        agent_type: info.agent_key,
        name: info.name,
        status: format!("{:?}", info.status).to_lowercase(),
        calls_today,
        version: String::new(),
        last_active,
    }
}

/// Reads per-agent usage stats from the local gatewayd SQLite database.
///
/// Returns the number of sessions touched today and a human-readable "last active"
/// string based on the most recent `last_active_at` timestamp.
fn collect_agent_stats(db_path: &PathBuf, agent_type: &str) -> anyhow::Result<(u64, String)> {
    let today_start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Utc)
        .unwrap()
        .to_rfc3339();

    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let (count, last): (i64, Option<String>) = conn.query_row(
        "SELECT COUNT(*), MAX(last_active_at) FROM sessions WHERE agent_type = ?1 AND last_active_at >= ?2",
        rusqlite::params![agent_type, today_start],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let last_active = last
        .and_then(|ts| DateTime::parse_from_rfc3339(&ts).ok())
        .map(|dt| format_relative_time(dt.with_timezone(&Utc)))
        .unwrap_or_default();

    Ok((count.max(0) as u64, last_active))
}

/// Formats a UTC timestamp as a Chinese relative time string.
fn format_relative_time(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    if dt > now {
        return "刚刚".to_string();
    }

    let diff = now.signed_duration_since(dt);
    let seconds = diff.num_seconds();
    if seconds < 60 {
        return "刚刚".to_string();
    }

    let minutes = diff.num_minutes();
    if minutes < 60 {
        return format!("{}分钟前", minutes);
    }

    let hours = diff.num_hours();
    if hours < 24 {
        return format!("{}小时前", hours);
    }

    let days = diff.num_days();
    format!("{}天前", days)
}
