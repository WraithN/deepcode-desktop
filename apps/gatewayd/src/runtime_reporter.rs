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
//!
//! ## Workspace path handling
//!
//! On a successful status report the platform returns a server-composed
//! `workspacePath` (currently `${workspace_root}/${workspace_id}/${user_id}`).
//! This module parses the response and caches the path in a shared
//! [`Arc<Mutex<Option<String>>>`] returned to the caller. Downstream code
//! (session manager, prewarm) can use the cached path as the default
//! `workDirectory` for new agent instances.

use agent_core::models::InstanceInfo;
use agent_core::service::AgentService;
use chrono::{DateTime, Utc};
use rusqlite::OpenFlags;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
    /// 已安装的智能体类型列表（CLI 已安装但未必正在运行）。
    /// 仅包含 opencode / claude-code / codex 三种 gatewayd 支持的类型。
    installed_agents: Vec<String>,
    /// 近 7 日会话总数（按 last_active_at 统计）。
    sessions_7d: u64,
    /// 近 1 日会话总数（按 last_active_at 统计）。
    sessions_1d: u64,
    /// 最近一次会话活跃时间（RFC3339），无会话时为 None。
    last_active_at: Option<String>,
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

/// Response shape returned by `POST /api/v1/agent-runtimes/{runtime_id}/status`.
///
/// Mirrors `object.AgentRuntime` in the enterprise platform's
/// `ReportStatus` JSON response (camelCase fields). All fields are
/// `#[serde(default)]` so a partial response does not break reporting —
/// `workspacePath` being empty simply means "platform did not assign a
/// workspace" and is treated as such by the caller.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct AgentRuntimeResponse {
    #[serde(default)]
    pub runtime_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default, alias = "workspacePath")]
    pub workspace_path: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub status: String,
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

/// Shared handle returned by [`start_runtime_reporter`] alongside the
/// background task. Holds the most recently platform-confirmed workspace
/// path and a readiness gate that downstream code (session manager,
/// prewarm) consults before starting new agent instances.
#[derive(Clone)]
pub struct RuntimeReporterHandle {
    /// Cached `workspacePath` returned by the platform on the last
    /// successful status report. `None` until the first successful sync.
    pub workspace_path: Arc<Mutex<Option<String>>>,
    /// Readiness gate. Open in local mode (platform not configured);
    /// closed until the first successful sync in platform mode.
    pub readiness: Arc<crate::readiness::WorkspacePathReadiness>,
}

/// Returns true if platform reporting would be enabled (config + env vars
/// resolve to a usable `[platform]` block). Used by the server bootstrap
/// to decide whether to construct a closed-gate [`WorkspacePathReadiness`]
/// (platform mode) or an open one (local mode) before wiring the
/// session manager and runtime reporter.
pub fn is_platform_reporting_configured() -> bool {
    load_config().is_some()
}

/// 返回当前生效的 dh-backend 平台地址 + API key，供其它模块复用
/// runtime_reporter 的配置加载逻辑（file + env 合并）。
///
/// 与 [`start_runtime_reporter`] 使用同一份 `[platform]` 配置：
/// - 未配置或未启用时返回 `None`
/// - 配置有效时返回 `Some((url, api_key))`
///
/// 调用方（如 crawler 远程拉取）据此决定是否发起 dh-backend 请求。
pub fn platform_backend() -> Option<(String, String)> {
    let cfg = load_config()?;
    Some((cfg.url, cfg.api_key))
}

/// Starts the background runtime status reporter using a caller-supplied
/// readiness tracker. The session manager and the reporter share the
/// same tracker so they agree on whether `create_agent` may proceed.
///
/// Returns `None` when platform reporting is not configured/enabled.
pub fn start_runtime_reporter_with_readiness(
    agent_service: Option<Arc<AgentService>>,
    db_path: Option<PathBuf>,
    readiness: Arc<crate::readiness::WorkspacePathReadiness>,
) -> Option<RuntimeReporterHandle> {
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

    let workspace_path = Arc::new(Mutex::new(None));

    let cached_path = workspace_path.clone();
    let readiness_for_task = readiness.clone();
    let _ = tokio::spawn(async move {
        let start_time = Instant::now();
        let client = reqwest::Client::new();

        loop {
            tokio::time::sleep(cfg.report_interval).await;

            let report = build_report(&cfg, agent_service.as_ref(), db_path.as_ref(), start_time).await;
            let url = format!("{}/api/v1/agent-runtimes/{}/status", cfg.url, cfg.runtime_id);

            let send_result = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", cfg.api_key))
                .header("Content-Type", "application/json")
                .json(&report)
                .timeout(Duration::from_secs(30))
                .send()
                .await;

            match send_result {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        warn!("[RuntimeReporter] Report failed: {} {}", status, body);
                        // Treat a non-2xx as a failure: the platform has
                        // not re-confirmed the path this cycle, so close
                        // the gate. The cached value is cleared (we no
                        // longer know if the previous path is still in
                        // effect after a sustained platform-side error).
                        readiness_for_task.mark_failed();
                        continue;
                    }

                    // Parse the response body for the server-composed
                    // `workspacePath`. A parse failure is non-fatal — the
                    // report itself succeeded; we just lose this cycle's
                    // path confirmation, so we still treat it as a
                    // failure (no fresh confirmation = no readiness).
                    match resp.json::<AgentRuntimeResponse>().await {
                        Ok(parsed) => {
                            if let Ok(mut g) = cached_path.lock() {
                                let prev = g.clone();
                                if !parsed.workspace_path.is_empty() {
                                    if prev.as_deref() != Some(parsed.workspace_path.as_str()) {
                                        info!(
                                            "[RuntimeReporter] workspacePath updated: {} -> {}",
                                            prev.as_deref().unwrap_or("<none>"),
                                            parsed.workspace_path
                                        );
                                    }
                                    *g = Some(parsed.workspace_path.clone());
                                } else {
                                    if prev.is_some() {
                                        info!(
                                            "[RuntimeReporter] workspacePath cleared by platform"
                                        );
                                    }
                                    *g = None;
                                }
                            }
                            readiness_for_task.mark_reported(&parsed.workspace_path);
                        }
                        Err(e) => {
                            warn!(
                                "[RuntimeReporter] Status report OK but failed to parse response: {e}"
                            );
                            readiness_for_task.mark_failed();
                        }
                    }
                }
                Err(e) => {
                    warn!("[RuntimeReporter] Request failed: {}", e);
                    readiness_for_task.mark_failed();
                }
            }
        }
    });

    Some(RuntimeReporterHandle {
        workspace_path,
        readiness,
    })
}

/// Convenience wrapper for callers that don't already hold a readiness
/// tracker. Constructs a fresh one in platform mode and starts the
/// reporter with it.
pub fn start_runtime_reporter(
    agent_service: Option<Arc<AgentService>>,
    db_path: Option<PathBuf>,
) -> Option<RuntimeReporterHandle> {
    if load_config().is_none() {
        return None;
    }
    let readiness = Arc::new(crate::readiness::WorkspacePathReadiness::new(true));
    start_runtime_reporter_with_readiness(agent_service, db_path, readiness)
}

/// Builds the runtime status report from config, system metrics and agent instances.
async fn build_report(
    cfg: &Config,
    agent_service: Option<&Arc<AgentService>>,
    db_path: Option<&PathBuf>,
    start_time: Instant,
) -> RuntimeStatusReport {
    let (cpu_percent, mem_percent, sandbox_spec) = collect_metrics();

    let (agents, installed_agents) = match agent_service {
        Some(svc) => {
            // 已安装的智能体：通过 list_plugins() 获取，过滤出 is_installed()=true 的。
            let installed: Vec<String> = svc
                .list_plugins()
                .into_iter()
                .filter(|p| p.installed)
                .map(|p| p.key)
                .collect();
            // 活跃的智能体实例：通过 list_instances() 获取当前已注册的实例。
            let active: Vec<RuntimeAgentStatus> = svc
                .list_instances()
                .await
                .into_iter()
                .map(|info| map_instance(info, db_path))
                .collect();
            (active, installed)
        }
        None => (Vec::new(), Vec::new()),
    };

    // 统计近 7 日 / 近 1 日会话总数及最近活跃时间。
    let (sessions_7d, sessions_1d, last_active_at) = db_path
        .and_then(|path| collect_session_counts(path).ok())
        .unwrap_or_default();

    RuntimeStatusReport {
        workspace_id: cfg.workspace_id.clone(),
        user_id: cfg.user_id.clone(),
        status: "running".to_string(),
        uptime_seconds: start_time.elapsed().as_secs(),
        cpu_percent,
        mem_percent,
        sandbox_spec,
        agents,
        installed_agents,
        sessions_7d,
        sessions_1d,
        last_active_at,
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

/// Reads total session counts and last active time from the local gatewayd SQLite database.
///
/// Returns `(sessions_7d, sessions_1d, last_active_at)`:
/// - `sessions_7d`: sessions whose `last_active_at` is within the last 7 days
/// - `sessions_1d`: sessions whose `last_active_at` is within the last 1 day
/// - `last_active_at`: the most recent `last_active_at` as RFC3339 string, or `None`
fn collect_session_counts(db_path: &PathBuf) -> anyhow::Result<(u64, u64, Option<String>)> {
    let now = Utc::now();
    let seven_days_ago = now - chrono::Duration::days(7);
    let one_day_ago = now - chrono::Duration::days(1);

    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    let (count_7d, count_1d, last): (i64, i64, Option<String>) = conn.query_row(
        "SELECT \
            SUM(CASE WHEN last_active_at >= ?1 THEN 1 ELSE 0 END), \
            SUM(CASE WHEN last_active_at >= ?2 THEN 1 ELSE 0 END), \
            MAX(last_active_at) \
         FROM sessions WHERE last_active_at >= ?1",
        rusqlite::params![
            seven_days_ago.to_rfc3339(),
            one_day_ago.to_rfc3339(),
        ],
        |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0), row.get(2).unwrap_or(None))),
    )?;

    Ok((count_7d.max(0) as u64, count_1d.max(0) as u64, last))
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
