//! Data transfer types for the platform integration.
//!
//! These types define the contract between the desktop app and the remote
//! platform:
//! - [`PlatformRemoteConfig`] is the response model for `GET /api/platform/config`.
//! - `*Batch` / `*Report` types are request bodies for the report endpoints.

use serde::{Deserialize, Serialize};

// ───── Remote platform config (GET /api/platform/config) ─────

/// Configuration fetched from the platform at startup.
///
/// The platform can push down agent defaults, available models, feature flags,
/// and skill definitions that the desktop app should adopt.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct PlatformRemoteConfig {
    /// Agent definitions the platform recommends.
    #[serde(default)]
    pub agents: Vec<RemoteAgentInfo>,

    /// Available models the platform exposes.
    #[serde(default)]
    pub models: Vec<RemoteModelInfo>,

    /// Arbitrary feature flags (e.g. `{"enableReporting": true}`).
    #[serde(default)]
    pub features: serde_json::Value,

    /// Skill definitions available on the platform.
    #[serde(default)]
    pub skills: Vec<RemoteSkillInfo>,
}

/// Agent definition pushed down from the platform.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RemoteAgentInfo {
    pub key: String,
    pub name: String,
    pub default_model: Option<String>,
}

/// Model definition pushed down from the platform.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RemoteModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
}

/// Skill definition pushed down from the platform.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RemoteSkillInfo {
    pub name: String,
    pub description: Option<String>,
}

// ───── Session report (POST /api/report/sessions) ─────

#[derive(Clone, Debug, Serialize)]
pub struct SessionReportBatch {
    pub batch: Vec<SessionReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionReport {
    pub id: String,
    pub user_id: String,
    pub title: String,
    pub agent: String,
    pub model: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<MessageReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MessageReport {
    pub id: String,
    pub role: String,
    pub content: String,
    pub token_in: Option<i64>,
    pub token_out: Option<i64>,
    pub duration_ms: Option<i64>,
    pub created_at: String,
}

// ───── Agent report (POST /api/report/agents) ─────

#[derive(Clone, Debug, Serialize)]
pub struct AgentReportBatch {
    pub batch: Vec<AgentReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentReport {
    pub id: String,
    pub agent_key: String,
    pub name: String,
    pub work_directory: String,
    pub status: serde_json::Value,
    pub endpoint: Option<String>,
}

// ───── Monitoring report (POST /api/report/monitoring) ─────

#[derive(Clone, Debug, Serialize)]
pub struct MonitoringReportBatch {
    pub batch: Vec<MonitoringReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MonitoringReport {
    pub conversation_id: String,
    pub instance_id: Option<String>,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

// ───── Agent Runtime status report (POST /api/v1/agent-runtimes/{runtimeId}/status) ─────

/// Runtime-level status report sent to the DeepHarness Enterprise Platform.
///
/// Mirrors the request body documented in the enterprise platform README.
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeStatusReport {
    pub workspace_id: String,
    pub workspace_path: String,
    pub user_id: String,
    pub status: String,
    pub uptime_seconds: u64,
    pub cpu_percent: f64,
    pub mem_percent: f64,
    pub sandbox_spec: String,
    pub agents: Vec<RuntimeAgentStatus>,
}

/// Status of a single agent instance inside a runtime.
#[derive(Clone, Debug, Serialize)]
pub struct RuntimeAgentStatus {
    #[serde(rename = "type")]
    pub agent_type: String,
    pub name: String,
    pub status: String,
    pub calls_today: u64,
    pub version: String,
    pub last_active: String,
}

// ───── Agent Runtime status response (from POST /api/v1/agent-runtimes/{runtimeId}/status) ─────

/// Response returned by the DeepHarness Enterprise Platform after a successful
/// Agent Runtime status report.
///
/// The platform composes `workspacePath` from `${workspace_root}/${workspace_id}/${user_id}`
/// on the server side. The desktop app uses this value as the canonical
/// working directory for agent instances and to gate operations until the
/// first successful sync lands.
///
/// All fields are deserialized leniently (`#[serde(default)]`) so a partial
/// response does not break status reporting. A missing or empty `workspacePath`
/// is treated as "platform did not assign a workspace" and the readiness gate
/// stays closed.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentRuntimeResponse {
    #[serde(default)]
    pub runtime_id: String,
    #[serde(default)]
    pub workspace_id: String,
    /// camelCase to match `object.AgentRuntime.WorkspacePath` in the
    /// enterprise platform's `ReportStatus` JSON response.
    #[serde(default, alias = "workspace_path")]
    pub workspace_path: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub status: String,
}
