use agent_core::error::InstanceError;
use agent_core::process::http::HttpTransport;
use agent_core::process::transport::TransportHandle;
use serde_json::json;
use std::time::Duration;
use tokio::process::Child;
use tokio::sync::mpsc;

const OPCODE_BINARY: &str = "opencode";
const ARG_SERVE: &str = "serve";
const ARG_PORT: &str = "--port";
const ARG_PURE: &str = "--pure";

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const HEALTH_PATH: &str = "/health";
const SESSION_PATH: &str = "/session";
const MESSAGE_PATH_SUFFIX: &str = "/message";
const CONTENT_TYPE_JSON: &str = "application/json";
const HEADER_CONTENT_TYPE: &str = "Content-Type";

const KEY_ID: &str = "id";
const KEY_PARTS: &str = "parts";
const KEY_TYPE: &str = "type";
const KEY_TEXT: &str = "text";
const BODY_TYPE_TEXT: &str = "text";
const ERR_MISSING_SESSION_ID: &str = "Missing session id";

const LOCALHOST_BIND_PREFIX: &str = "127.0.0.1:";

const ERR_START_OPCODE_SERVE_PREFIX: &str = "Failed to start opencode serve: ";
const ERR_CREATE_SESSION_PREFIX: &str = "create_session: ";
const ERR_SEND_MESSAGE_PREFIX: &str = "send_message: ";
const ERR_NO_AVAILABLE_PORT_PREFIX: &str = "No available port found in range ";
const ERR_SSE_CONNECT_PREFIX: &str = "SSE connect failed: ";

const PORT_RANGE_START: u16 = 3001;
const PORT_RANGE_END: u16 = 3050;

/// HTTP client for the OpenCode `serve` endpoints.
pub struct OpenCodeClient {
    client: reqwest::Client,
    base_url: String,
}

impl OpenCodeClient {
    /// Creates a client that talks to `base_url`.
    pub fn new(base_url: impl Into<String>) -> Self {
        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url: base_url.into(),
        }
    }

    /// Returns the underlying HTTP client.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Performs a health check against `/health`.
    pub async fn health_check(&self) -> bool {
        let url = format!("{}{}", self.base_url, HEALTH_PATH);
        self.client.get(&url).send().await.is_ok()
    }

    /// Creates a new OpenCode session and returns its id.
    pub async fn create_session(&self) -> Result<String, InstanceError> {
        let url = format!("{}{}", self.base_url, SESSION_PATH);
        let resp = self
            .client
            .post(&url)
            .header(HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON)
            .json(&json!({}))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("{}{}", ERR_CREATE_SESSION_PREFIX, e)))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse {}", e)))?;

        body.get(KEY_ID)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| InstanceError::SendFailed(ERR_MISSING_SESSION_ID.into()))
    }

    /// Sends `message` to the given OpenCode `session_id`.
    pub async fn send_message(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let url = format!(
            "{}{}/{}{}",
            self.base_url, SESSION_PATH, session_id, MESSAGE_PATH_SUFFIX
        );
        let resp = self
            .client
            .post(&url)
            .header(HEADER_CONTENT_TYPE, CONTENT_TYPE_JSON)
            .json(&json!({
                KEY_PARTS: [{ KEY_TYPE: BODY_TYPE_TEXT, KEY_TEXT: message }]
            }))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("{}{}", ERR_SEND_MESSAGE_PREFIX, e)))?;

        resp.json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse {}", e)))
    }
}

/// Spawns `opencode serve` on the given port.
pub fn start_opencode_process(port: u16) -> Result<Child, InstanceError> {
    let mut cmd = tokio::process::Command::new(OPCODE_BINARY);
    cmd.arg(ARG_SERVE)
        .arg(ARG_PORT)
        .arg(port.to_string())
        .arg(ARG_PURE)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    cmd.spawn()
        .map_err(|e| InstanceError::ProcessError(format!("{}{}", ERR_START_OPCODE_SERVE_PREFIX, e)))
}

/// Finds an available TCP port in the default OpenCode range.
pub fn find_available_port() -> Result<u16, String> {
    for port in PORT_RANGE_START..=PORT_RANGE_END {
        if std::net::TcpListener::bind(format!("{}{}", LOCALHOST_BIND_PREFIX, port)).is_ok() {
            return Ok(port);
        }
    }
    Err(format!(
        "{}{}-{}",
        ERR_NO_AVAILABLE_PORT_PREFIX, PORT_RANGE_START, PORT_RANGE_END
    ))
}

/// Connects the SSE stream for an OpenCode instance.
pub async fn connect_opencode_sse(
    base_url: &str,
    client: reqwest::Client,
    instance_id: &str,
    sender: mpsc::Sender<serde_json::Value>,
) -> Result<Box<dyn TransportHandle>, InstanceError> {
    HttpTransport::with_client(base_url, client)
        .connect_sse(instance_id.to_string(), sender)
        .await
        .map_err(|e| InstanceError::ProcessError(format!("{}{}", ERR_SSE_CONNECT_PREFIX, e)))
}
