//! HTTP client for the platform REST API.
//!
//! Wraps [`reqwest::Client`] with Bearer-token authentication and provides
//! typed methods for the four platform endpoints:
//! - `GET  /api/platform/config`
//! - `POST /api/report/sessions`
//! - `POST /api/report/agents`
//! - `POST /api/report/monitoring`

use crate::platform::payload::{
    AgentReportBatch, AgentRuntimeResponse, MonitoringReportBatch, PlatformRemoteConfig, RuntimeStatusReport,
    SessionReportBatch,
};
use dh_config::PlatformConfig;
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

// ───── API path constants ─────

const API_PATH_CONFIG: &str = "/api/platform/config";
const API_PATH_REPORT_SESSIONS: &str = "/api/report/sessions";
const API_PATH_REPORT_AGENTS: &str = "/api/report/agents";
const API_PATH_REPORT_MONITORING: &str = "/api/report/monitoring";
const API_PATH_AGENT_RUNTIMES: &str = "/api/v1/agent-runtimes";

/// HTTP client for platform API calls with Bearer-token auth.
pub struct PlatformClient {
    client: Client,
    base_url: String,
    api_key: String,
}

impl PlatformClient {
    /// Creates a client from a [`PlatformConfig`].
    ///
    /// Returns an error if the URL is missing (caller should check
    /// [`PlatformConfig::is_active`] first).
    pub fn new(config: &PlatformConfig) -> Result<Self, String> {
        let base_url = config
            .url
            .as_ref()
            .ok_or("platform url not configured")?
            .trim_end_matches('/')
            .to_string();

        let api_key = config.api_key.clone().unwrap_or_default();
        let timeout = Duration::from_secs(config.request_timeout());

        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    /// Returns the base URL the client is configured to talk to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    async fn get(&self, path: &str) -> Result<reqwest::Response, String> {
        self.client
            .get(self.url(path))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| format!("GET {path} failed: {e}"))
    }

    async fn post<T: Serialize>(&self, path: &str, body: &T) -> Result<reqwest::Response, String> {
        self.client
            .post(self.url(path))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("POST {path} failed: {e}"))
    }

    /// Fetches the remote platform configuration.
    pub async fn fetch_config(&self) -> Result<PlatformRemoteConfig, String> {
        let resp = self.get(API_PATH_CONFIG).await?;
        let resp = ensure_success(resp, API_PATH_CONFIG).await?;
        resp.json()
            .await
            .map_err(|e| format!("failed to parse platform config: {e}"))
    }

    /// Reports a batch of session data.
    pub async fn report_sessions(&self, batch: &SessionReportBatch) -> Result<(), String> {
        let resp = self.post(API_PATH_REPORT_SESSIONS, batch).await?;
        ensure_success(resp, API_PATH_REPORT_SESSIONS).await?;
        Ok(())
    }

    /// Reports a batch of agent status snapshots.
    pub async fn report_agents(&self, batch: &AgentReportBatch) -> Result<(), String> {
        let resp = self.post(API_PATH_REPORT_AGENTS, batch).await?;
        ensure_success(resp, API_PATH_REPORT_AGENTS).await?;
        Ok(())
    }

    /// Reports a batch of monitoring (session log) entries.
    pub async fn report_monitoring(&self, batch: &MonitoringReportBatch) -> Result<(), String> {
        let resp = self.post(API_PATH_REPORT_MONITORING, batch).await?;
        ensure_success(resp, API_PATH_REPORT_MONITORING).await?;
        Ok(())
    }

    /// Reports the runtime status for a given runtime id and returns the
    /// platform's response.
    ///
    /// Endpoint: `POST /api/v1/agent-runtimes/{runtimeId}/status`
    ///
    /// The platform responds with the upserted [`AgentRuntimeResponse`],
    /// including the server-side `workspacePath` composed from
    /// `${workspace_root}/${workspace_id}/${user_id}`. Callers use that path
    /// as the canonical working directory for agent instances and as the
    /// readiness signal that gates user-facing operations.
    ///
    /// If the platform returns a non-2xx status the response body is included
    /// in the error message for diagnostics. A successful HTTP response whose
    /// body cannot be parsed as JSON is **not** an error — the response
    /// object is returned with empty fields so the caller can decide whether
    /// to treat it as "no workspace path assigned".
    pub async fn report_runtime_status(
        &self,
        runtime_id: &str,
        report: &RuntimeStatusReport,
    ) -> Result<AgentRuntimeResponse, String> {
        let path = format!("{API_PATH_AGENT_RUNTIMES}/{runtime_id}/status");
        let resp = self.post(&path, report).await?;
        let resp = ensure_success(resp, &path).await?;

        match resp.json::<AgentRuntimeResponse>().await {
            Ok(parsed) => Ok(parsed),
            Err(e) => {
                // Body present but not in the expected shape — log and let the
                // caller proceed with an empty response so reporting does not
                // get stuck on a single bad payload.
                log::warn!(
                    "[Platform] Failed to parse AgentRuntimeResponse from {path}: {e}"
                );
                Ok(AgentRuntimeResponse::default())
            }
        }
    }
}

/// Verifies the HTTP response status is 2xx. On success, returns the
/// response so the caller can consume the body. On failure, returns an
/// error message with the response body for diagnostics.
async fn ensure_success(resp: reqwest::Response, path: &str) -> Result<reqwest::Response, String> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_else(|_| "<no response body>".to_string());
    Err(format!("{path} returned {status}: {body}"))
}
