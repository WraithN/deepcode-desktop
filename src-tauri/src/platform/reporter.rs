//! Background reporter that pushes session, agent, and monitoring data
//! to the remote platform.
//!
//! ## Strategy
//!
//! The reporter uses a hybrid event-driven + periodic-batch approach:
//!
//! - **Event-driven**: agent status changes are reported immediately via
//!   `POST /api/report/agents` so the platform has near-real-time visibility.
//! - **Periodic batch**: every `report_interval` seconds, the reporter
//!   flushes accumulated monitoring logs and queries the local DB for the
//!   latest sessions/agent instances, sending them in batches. This ensures
//!   completeness even if an event is missed.
//!
//! The reporter runs as an async task and must never panic — all errors are
//! logged and the loop continues so transient network failures don't kill
//! reporting permanently.

use crate::platform::client::PlatformClient;
use crate::platform::payload::{
    AgentReport, AgentReportBatch, MessageReport, MonitoringReport, MonitoringReportBatch, SessionReport,
    SessionReportBatch,
};
use crate::platform::runtime_status::RuntimeStatusCollector;
use crate::service::agent_service::AgentService;
use dh_config::PlatformConfig;
use dh_db::desktop::AppRepository;
use rand::Rng;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

// ───── Limits & constants ─────

/// Maximum conversations queried per periodic flush.
const SESSION_REPORT_LIMIT: i64 = 50;

/// Maximum messages queried per conversation during a flush.
const MESSAGE_REPORT_LIMIT: i64 = 100;

/// Cap on the in-memory monitoring buffer to prevent unbounded growth
/// if the platform is unreachable for a long period.
const MAX_MONITORING_BUFFER: usize = 500;

/// Redaction placeholder used when `sanitize` is enabled.
const REDACTED_PLACEHOLDER: &str = "[redacted]";

// ───── Event types (received from the fan-out EventSink) ─────

/// Events forwarded from the [`EventSink`](agent_core::event_sink::EventSink) to the reporter.
pub enum ReportEvent {
    /// An agent instance changed status — reported immediately.
    AgentStatusChanged(Value),
    /// A session log entry was emitted — accumulated for batch flush.
    SessionLog(Value),
}

// ───── Reporter ─────

/// Background task that reports session, agent, and monitoring data to the
/// platform.
pub struct Reporter {
    client: PlatformClient,
    repository: AppRepository,
    agent_service: Arc<AgentService>,
    rx: mpsc::UnboundedReceiver<ReportEvent>,
    monitoring_buffer: Vec<MonitoringReport>,
    report_interval: Duration,
    sanitize: bool,
    platform_config: PlatformConfig,
    runtime_collector: RuntimeStatusCollector,
}

impl Reporter {
    pub fn new(
        client: PlatformClient,
        repository: AppRepository,
        agent_service: Arc<AgentService>,
        rx: mpsc::UnboundedReceiver<ReportEvent>,
        report_interval: Duration,
        sanitize: bool,
        platform_config: PlatformConfig,
    ) -> Self {
        Self {
            client,
            repository,
            agent_service,
            rx,
            monitoring_buffer: Vec::new(),
            report_interval,
            sanitize,
            platform_config,
            runtime_collector: RuntimeStatusCollector::new(),
        }
    }

    /// Runs the reporter loop until the channel is closed and drained.
    ///
    /// On startup, a best-effort fetch of the remote platform config is
    /// performed. Failures are logged but do not prevent reporting.
    pub async fn run(mut self) {
        let max_interval = self.platform_config.report_interval_max();
        match max_interval {
            Some(max) => {
                log::info!(
                    "[Reporter] Starting - interval={}s..{}s (jittered) sanitize={}",
                    self.report_interval.as_secs(),
                    max,
                    self.sanitize
                );
            }
            None => {
                log::info!(
                    "[Reporter] Starting - interval={}s sanitize={}",
                    self.report_interval.as_secs(),
                    self.sanitize
                );
            }
        }

        self.fetch_platform_config().await;

        // Don't fire immediately; the first flush happens after one
        // random/fixed interval, giving the app time to initialise.
        let mut next_deadline = tokio::time::Instant::now() + self.next_report_interval(max_interval);

        loop {
            tokio::select! {
                biased; // prefer events over the timer

                event = self.rx.recv() => {
                    if let Some(ev) = event {
                        self.handle_event(ev).await;
                    } else {
                        // Channel closed - drain remaining buffer then exit.
                        log::info!("[Reporter] Channel closed, flushing and stopping");
                        self.flush().await;
                        return;
                    }
                }
                _ = tokio::time::sleep_until(next_deadline) => {
                    self.flush().await;
                    next_deadline = tokio::time::Instant::now() + self.next_report_interval(max_interval);
                }
            }
        }
    }

    /// Returns the duration to wait until the next periodic flush.
    ///
    /// When `report_interval_max_secs` is configured and greater than the base
    /// interval, a random value in `[base, max]` is chosen for each cycle to
    /// spread out reporting traffic and prevent thundering herd.
    fn next_report_interval(&self, max_secs: Option<u64>) -> Duration {
        Duration::from_secs(jittered_interval_seconds(self.report_interval.as_secs(), max_secs))
    }

    /// Best-effort fetch of the remote platform config at startup.
    async fn fetch_platform_config(&self) {
        match self.client.fetch_config().await {
            Ok(remote) => {
                log::info!(
                    "[Reporter] Platform config fetched: {} agents, {} models, {} skills",
                    remote.agents.len(),
                    remote.models.len(),
                    remote.skills.len()
                );
            }
            Err(e) => {
                log::warn!("[Reporter] Failed to fetch platform config: {e}");
            }
        }
    }

    // ───── Event handling (event-driven path) ─────

    async fn handle_event(&mut self, event: ReportEvent) {
        match event {
            ReportEvent::AgentStatusChanged(payload) => {
                self.report_agent_status(&payload).await;
            }
            ReportEvent::SessionLog(payload) => {
                self.accumulate_log(&payload);
            }
        }
    }

    /// Immediately reports a single agent status change to the platform.
    async fn report_agent_status(&self, payload: &Value) {
        let Some(report) = build_agent_report(payload) else {
            return;
        };
        let batch = AgentReportBatch { batch: vec![report] };
        if let Err(e) = self.client.report_agents(&batch).await {
            log::warn!("[Reporter] Agent status report failed: {e}");
        }

        // Also push an updated runtime snapshot so the platform sees the
        // runtime-level impact immediately.
        self.flush_runtime_status().await;
    }

    /// Pushes a session log entry into the monitoring buffer for batch flush.
    fn accumulate_log(&mut self, payload: &Value) {
        if self.monitoring_buffer.len() >= MAX_MONITORING_BUFFER {
            // Drop the oldest entry to make room (ring-buffer semantics).
            self.monitoring_buffer.remove(0);
        }
        if let Some(entry) = build_monitoring_report(payload) {
            self.monitoring_buffer.push(entry);
        }
    }

    // ───── Periodic flush (batch path) ─────

    /// Flushes all accumulated data: sessions, agents, monitoring logs, and
    /// runtime status.
    async fn flush(&mut self) {
        self.flush_sessions().await;
        self.flush_agents().await;
        self.flush_monitoring().await;
        self.flush_runtime_status().await;
    }

    /// Queries the DB for recent conversations + messages, then POSTs them.
    async fn flush_sessions(&self) {
        let conversations = match self.repository.load_all_conversations(SESSION_REPORT_LIMIT) {
            Ok(rows) => rows,
            Err(e) => {
                log::warn!("[Reporter] Failed to load conversations: {e}");
                return;
            }
        };

        let mut batch = Vec::with_capacity(conversations.len());
        for conv in conversations {
            match self.build_session_report(&conv) {
                Ok(report) => batch.push(report),
                Err(e) => log::warn!("[Reporter] Skipping conversation: {e}"),
            }
        }

        if batch.is_empty() {
            return;
        }

        let payload = SessionReportBatch { batch };
        if let Err(e) = self.client.report_sessions(&payload).await {
            log::warn!("[Reporter] Session report failed: {e}");
        }
    }

    /// Builds a [`SessionReport`] from a conversation JSON row, including
    /// its messages.
    fn build_session_report(&self, conv: &Value) -> Result<SessionReport, String> {
        let conv_id = conv["id"].as_str().ok_or("conversation missing id")?.to_string();

        let messages = self
            .repository
            .load_messages(&conv_id, MESSAGE_REPORT_LIMIT)
            .unwrap_or_default();

        let message_reports: Vec<MessageReport> = messages
            .iter()
            .map(|m| MessageReport {
                id: str_or_empty(m, "id"),
                role: str_or_empty(m, "role"),
                content: self.maybe_sanitize(&str_or_empty(m, "content")),
                token_in: m["token_in"].as_i64(),
                token_out: m["token_out"].as_i64(),
                duration_ms: m["duration_ms"].as_i64(),
                created_at: str_or_empty(m, "created_at"),
            })
            .collect();

        Ok(SessionReport {
            id: conv_id,
            user_id: str_or_empty(conv, "user_id"),
            title: str_or_empty(conv, "title"),
            agent: str_or_empty(conv, "agent"),
            model: str_or_empty(conv, "model"),
            created_at: str_or_empty(conv, "created_at"),
            updated_at: str_or_empty(conv, "updated_at"),
            messages: message_reports,
        })
    }

    /// Queries the [`AgentService`] for all instances, then POSTs their status.
    async fn flush_agents(&self) {
        let instances = self.agent_service.list_instances().await;
        let batch: Vec<AgentReport> = instances
            .iter()
            .map(|info| AgentReport {
                id: info.id.clone(),
                agent_key: info.agent_key.clone(),
                name: info.name.clone(),
                work_directory: info.work_directory.clone(),
                status: serde_json::to_value(&info.status).unwrap_or(Value::Null),
                endpoint: info.endpoint.clone(),
            })
            .collect();

        if batch.is_empty() {
            return;
        }

        let payload = AgentReportBatch { batch };
        if let Err(e) = self.client.report_agents(&payload).await {
            log::warn!("[Reporter] Agent batch report failed: {e}");
        }
    }

    /// Flushes accumulated monitoring logs, then clears the buffer.
    async fn flush_monitoring(&mut self) {
        if self.monitoring_buffer.is_empty() {
            return;
        }

        // Move the buffer out to avoid holding a borrow across the await.
        let entries: Vec<_> = self.monitoring_buffer.drain(..).collect();
        let payload = MonitoringReportBatch { batch: entries };

        if let Err(e) = self.client.report_monitoring(&payload).await {
            log::warn!("[Reporter] Monitoring report failed: {e}");
        }
    }

    /// Queries the current agent instances, aggregates runtime metrics, and
    /// POSTs the runtime status to the DH Backend.
    async fn flush_runtime_status(&self) {
        if !self.platform_config.is_runtime_reporting_active() {
            return;
        }

        let Some(runtime_id) = self.platform_config.runtime_id.as_ref() else {
            return;
        };

        let instances = self.agent_service.list_instances().await;
        let workspace_path = self.repository.get_workspace_path().unwrap_or(None).unwrap_or_default();
        let Some(report) = self.runtime_collector.build_report(&self.platform_config, &instances, &workspace_path) else {
            return;
        };

        if let Err(e) = self.client.report_runtime_status(runtime_id, &report).await {
            log::warn!("[Reporter] Runtime status report failed: {e}");
        }
    }

    // ───── Helpers ─────

    /// Returns the content as-is, or a redaction placeholder when
    /// sanitization is enabled.
    fn maybe_sanitize(&self, content: &str) -> String {
        if self.sanitize {
            format!("{REDACTED_PLACEHOLDER}:{} chars", content.len())
        } else {
            content.to_string()
        }
    }
}

// ───── Free helper functions ─────

/// Extracts a string field from a JSON value, returning `""` if missing.
fn str_or_empty(value: &Value, field: &str) -> String {
    value[field].as_str().unwrap_or("").to_string()
}

/// Builds an [`AgentReport`] from an `agent.status_changed` event payload.
///
/// The payload may use either `camelCase` or `snake_case` keys (agents emit
/// both depending on the plugin), so we check both.
fn build_agent_report(payload: &Value) -> Option<AgentReport> {
    let id = payload
        .get("instanceId")
        .or_else(|| payload.get("instance_id"))
        .and_then(|v| v.as_str())?;
    // If status isn't inline, we still report with a null status.
    let status = payload.get("status").cloned().unwrap_or(Value::Null);

    Some(AgentReport {
        id: id.to_string(),
        agent_key: str_field(payload, &["agentKey", "agent_key"]),
        name: str_field(payload, &["name"]),
        work_directory: str_field(payload, &["workDirectory", "work_directory"]),
        status,
        endpoint: payload.get("endpoint").and_then(|v| v.as_str()).map(String::from),
    })
}

/// Builds a [`MonitoringReport`] from a `session.log` event payload.
fn build_monitoring_report(payload: &Value) -> Option<MonitoringReport> {
    let conversation_id = payload
        .get("conversationId")
        .or_else(|| payload.get("conversation_id"))
        .and_then(|v| v.as_str())?;

    Some(MonitoringReport {
        conversation_id: conversation_id.to_string(),
        instance_id: payload
            .get("instanceId")
            .or_else(|| payload.get("instance_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        timestamp: str_field(payload, &["timestamp"]),
        level: str_field(payload, &["level"]),
        source: str_field(payload, &["source"]),
        message: str_field(payload, &["message"]),
    })
}

/// Reads a string field trying multiple key names (`camelCase` then `snake_case`).
fn str_field(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(s) = value.get(*key).and_then(|v| v.as_str()) {
            return s.to_string();
        }
    }
    String::new()
}

/// Returns a jittered interval in seconds.
///
/// When `max_secs` is `Some(max)` and `max > base`, returns a random value in
/// `[base, max]`. Otherwise returns `base`.
fn jittered_interval_seconds(base: u64, max_secs: Option<u64>) -> u64 {
    match max_secs {
        Some(max) if max > base => {
            let mut rng = rand::thread_rng();
            rng.gen_range(base..=max)
        }
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jittered_interval_returns_base_when_no_max() {
        assert_eq!(jittered_interval_seconds(30, None), 30);
    }

    #[test]
    fn jittered_interval_returns_base_when_max_not_greater() {
        assert_eq!(jittered_interval_seconds(30, Some(30)), 30);
        assert_eq!(jittered_interval_seconds(30, Some(10)), 30);
    }

    #[test]
    fn jittered_interval_stays_within_bounds() {
        for _ in 0..100 {
            let value = jittered_interval_seconds(30, Some(60));
            assert!(value >= 30 && value <= 60, "value out of bounds: {value}");
        }
    }
}
