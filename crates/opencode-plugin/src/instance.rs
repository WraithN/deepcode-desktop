use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus};
use agent_core::logger::{LogLevel, SessionLogger};
use futures_util::StreamExt;
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// SSE event as emitted by `opencode serve`.
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: String,
    pub session_id: Option<String>,
    pub payload: serde_json::Value,
}

pub struct OpencodeInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    client: reqwest::Client,
    base_url: Mutex<Option<String>>,
    serve_process: Arc<tokio::sync::Mutex<Option<tokio::process::Child>>>,
    status: Arc<Mutex<InstanceStatus>>,
    event_sender: Arc<Mutex<Option<tokio::sync::broadcast::Sender<SseEvent>>>>,
    started: Arc<std::sync::atomic::AtomicBool>,
    /// conversation_id -> opencode_session_id
    sessions: Arc<Mutex<HashMap<String, String>>>,
}

impl OpencodeInstance {
    pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            event_sink,
            logger,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            base_url: Mutex::new(None),
            serve_process: Arc::new(tokio::sync::Mutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            event_sender: Arc::new(Mutex::new(None)),
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn emit_status(&self, status: InstanceStatus) {
        self.event_sink.emit(
            "agent:status_changed",
            json!({
                "instance_id": self.config.id,
                "status": status,
            }),
        );
    }

    fn base_url(&self) -> Option<String> {
        self.base_url.lock().unwrap().clone()
    }

    /// Start `opencode serve` and the SSE listener (idempotent).
    async fn ensure_started(&self) -> Result<(), InstanceError> {
        if self
            .started
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_err()
        {
            // Already started; wait until base_url is available.
            for _ in 0..20 {
                if self.base_url().is_some() {
                    return Ok(());
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
            return Err(InstanceError::NotRunning(
                "opencode serve did not become ready".into(),
            ));
        }

        let port = find_available_port()
            .map_err(|e| InstanceError::ProcessError(e))?;
        let base_url = format!("http://127.0.0.1:{}", port);

        let mut cmd = tokio::process::Command::new("opencode");
        cmd.arg("serve")
            .arg("--port")
            .arg(port.to_string())
            .arg("--pure")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| InstanceError::ProcessError(format!("Failed to start opencode serve: {}", e)))?;

        // Health-check loop (sync-style via async sleep)
        let health_url = format!("{}/health", base_url);
        let mut ready = false;
        for _ in 0..20 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            if self.client.get(&health_url).send().await.is_ok() {
                ready = true;
                break;
            }
        }
        if !ready {
            let _ = child.start_kill();
            self.started.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(InstanceError::ProcessError(
                format!("opencode serve did not become ready on port {}", port),
            ));
        }

        {
            let mut guard = self.base_url.lock().unwrap();
            *guard = Some(base_url.clone());
        }
        {
            let mut guard = self.serve_process.lock().await;
            *guard = Some(child);
        }
        {
            let mut guard = self.status.lock().unwrap();
            *guard = InstanceStatus::Running { pid: 0 };
        }
        self.emit_status(InstanceStatus::Running { pid: 0 });

        self.logger.log(
            &self.config.id,
            LogLevel::Info,
            "opencode-plugin",
            &format!("opencode serve started on {}", base_url),
            None,
            Some(self.config.id.clone()),
        );

        // Setup broadcast channel for SSE events
        let (tx, _rx) = tokio::sync::broadcast::channel::<SseEvent>(1000);
        {
            let mut guard = self.event_sender.lock().unwrap();
            *guard = Some(tx.clone());
        }

        // Spawn SSE listener
        let base_url_for_sse = base_url.clone();
        let client_for_sse = self.client.clone();
        let event_sink_for_sse = self.event_sink.clone();
        let instance_id = self.config.id.clone();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            sse_event_loop(
                &base_url_for_sse,
                client_for_sse,
                tx,
                event_sink_for_sse,
                &instance_id,
                logger,
            )
            .await;
        });

        Ok(())
    }

    async fn create_opencode_session(&self) -> Result<String, InstanceError> {
        let base = self.base_url().ok_or_else(|| {
            InstanceError::NotRunning("opencode serve not started".into())
        })?;

        let resp = self
            .client
            .post(format!("{}/session", base))
            .header("Content-Type", "application/json")
            .json(&json!({}))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("create_session: {}", e)))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse create_session: {}", e)))?;

        body.get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| InstanceError::SendFailed("Missing session id".into()))
    }

    async fn send_message_http(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let base = self.base_url().ok_or_else(|| {
            InstanceError::NotRunning("opencode serve not started".into())
        })?;

        let resp = self
            .client
            .post(format!("{}/session/{}/message", base, session_id))
            .header("Content-Type", "application/json")
            .json(&json!({
                "parts": [{ "type": "text", "text": message }]
            }))
            .send()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("send_message: {}", e)))?;

        resp.json()
            .await
            .map_err(|e| InstanceError::SendFailed(format!("parse send_message: {}", e)))
    }

    fn find_session_for_conversation(&self, conversation_id: &str) -> Option<String> {
        let guard = self.sessions.lock().unwrap();
        guard.get(conversation_id).cloned()
    }

    fn store_session(&self, conversation_id: &str, session_id: &str) {
        let mut guard = self.sessions.lock().unwrap();
        guard.insert(conversation_id.to_string(), session_id.to_string());
    }
}

impl AgentInstance for OpencodeInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn plugin_key(&self) -> &'static str {
        "opencode"
    }

    fn endpoint(&self) -> Option<String> {
        self.base_url()
    }

    fn send_message(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let conversation_id = conversation_id.to_string();
        let message = message.to_string();

        Box::pin(async move {
            self.ensure_started().await?;

            // Get or create opencode session for this conversation.
            let session_id = match self.find_session_for_conversation(&conversation_id) {
                Some(sid) => sid,
                None => {
                    let sid = self.create_opencode_session().await?;
                    self.store_session(&conversation_id, &sid);
                    sid
                }
            };

            // Send the message.
            let result = self.send_message_http(&session_id, &message).await?;

            // Detect interactions from response parts.
            if let Some(parts) = result.get("parts").and_then(|v| v.as_array()) {
                let all_parts: Vec<serde_json::Value> = parts.clone();
                if let Some(interaction) = detect_interaction_from_parts(&all_parts) {
                    let method = match &interaction {
                        InteractionRequest::Question { .. } => "agent.question",
                        InteractionRequest::Permission { .. } => "agent.permission",
                        InteractionRequest::TodoWrite { .. } => "agent.todowrite",
                    };
                    let interaction_json = serde_json::to_value(&interaction).unwrap_or_default();
                    let session_id_result = result
                        .get("info")
                        .and_then(|i| i.get("sessionID"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&session_id)
                        .to_string();
                    self.event_sink.emit(
                        method,
                        json!({
                            "sessionID": session_id_result,
                            "interaction": interaction_json,
                            "conversation_id": conversation_id,
                        }),
                    );
                }
            }

            Ok(())
        })
    }

    fn respond(
        &self,
        session_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let session_id = session_id.to_string();
        let message = message.to_string();
        Box::pin(async move {
            self.ensure_started().await?;
            self.send_message_http(&session_id, &message).await?;
            Ok(())
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(mut child) = self.serve_process.lock().await.take() {
                let _ = child.start_kill();
            }
            {
                let mut guard = self.status.lock().unwrap();
                *guard = InstanceStatus::Stopped;
            }
            self.emit_status(InstanceStatus::Stopped);
            Ok(())
        })
    }
}

// ---------------------------------------------------------------------------
// SSE event loop
// ---------------------------------------------------------------------------

async fn sse_event_loop(
    base_url: &str,
    client: reqwest::Client,
    sender: tokio::sync::broadcast::Sender<SseEvent>,
    event_sink: DynEventSink,
    instance_id: &str,
    logger: Arc<SessionLogger>,
) {
    let url = format!("{}/event", base_url);

    loop {
        match client
            .get(&url)
            .header("Accept", "text/event-stream")
            .send()
            .await
        {
            Ok(resp) => {
                let mut stream = resp.bytes_stream();
                while let Some(chunk) = stream.next().await {
                    match chunk {
                        Ok(bytes) => {
                            let text = String::from_utf8_lossy(&bytes);
                            for line in text.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if let Ok(payload) = serde_json::from_str::<serde_json::Value>(data)
                                    {
                                        let event_type = payload
                                            .get("type")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown")
                                            .to_string();
                                        let session_id = payload
                                            .get("properties")
                                            .and_then(|p| p.get("sessionID"))
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());

                                        let _ = sender.send(SseEvent {
                                            event_type: event_type.clone(),
                                            session_id: session_id.clone(),
                                            payload: payload.clone(),
                                        });

                                        // Forward to WebSocket via EventSink
                                        forward_sse_to_event_sink(
                                            &event_type,
                                            &payload,
                                            &event_sink,
                                            instance_id,
                                            &logger,
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("[opencode SSE] stream error: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("[opencode SSE] connect error: {}, retrying...", e);
            }
        }
        log::info!("[opencode SSE] disconnected, retrying in 3s...");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    }
}

/// Map raw SSE events to frontend-facing WebSocket events.
fn forward_sse_to_event_sink(
    event_type: &str,
    payload: &serde_json::Value,
    event_sink: &DynEventSink,
    instance_id: &str,
    _logger: &Arc<SessionLogger>,
) {
    match event_type {
        "message.part.delta" => {
            if let Some(props) = payload.get("properties") {
                if let Some(delta) = props.get("delta").and_then(|v| v.as_str()) {
                    if !delta.is_empty() {
                        event_sink.emit(
                            "agent.token",
                            json!({
                                "text": delta,
                                "instance_id": instance_id,
                            }),
                        );
                    }
                }
            }
        }
        "thinking" => {
            let content = payload
                .get("content")
                .and_then(|v| v.as_str())
                .or_else(|| payload.get("text").and_then(|v| v.as_str()))
                .unwrap_or("");
            event_sink.emit(
                "agent.thinking",
                json!({
                    "content": content,
                    "id": format!("thinking-{}", instance_id),
                    "type": "step-start",
                    "instance_id": instance_id,
                }),
            );
        }
        "message.part.updated" => {
            if let Some(props) = payload.get("properties") {
                if let Some(part) = props.get("part") {
                    if let Some("step-start") = part.get("type").and_then(|v| v.as_str()) {
                        let step_text = part.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        let step_id = part.get("id").and_then(|v| v.as_str()).unwrap_or("");
                        event_sink.emit(
                            "agent.thinking",
                            json!({
                                "content": step_text,
                                "id": step_id,
                                "type": "step-start",
                                "instance_id": instance_id,
                            }),
                        );
                    }
                }
            }
        }
        "session.idle" => {
            event_sink.emit(
                "agent.done",
                json!({
                    "instance_id": instance_id,
                }),
            );
        }
        "session.error" => {
            event_sink.emit(
                "agent.error",
                json!({
                    "message": payload.to_string(),
                    "instance_id": instance_id,
                }),
            );
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_available_port() -> Result<u16, String> {
    for port in 3001..=3050 {
        if std::net::TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("No available port found in range 3001-3050".to_string())
}

// Copied from legacy opencode_service.rs for interaction detection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionRequest {
    Question { questions: Vec<QuestionItem> },
    Permission { tool_name: String, action: String },
    TodoWrite { todos: Vec<TodoItem> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuestionItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub text: String,
    pub completed: bool,
}

fn detect_interaction_from_parts(parts: &[serde_json::Value]) -> Option<InteractionRequest> {
    for part in parts {
        let part_type = part.get("type").and_then(|v| v.as_str());
        match part_type {
            Some("tool_use") => {
                let tool_name = part
                    .get("toolName")
                    .or_else(|| part.get("tool_name"))
                    .and_then(|v| v.as_str());
                match tool_name {
                    Some("question") => {
                        if let Some(input) = part.get("input") {
                            if let Ok(questions) = serde_json::from_value::<Vec<QuestionItem>>(
                                input.get("questions").cloned().unwrap_or(serde_json::Value::Null),
                            ) {
                                return Some(InteractionRequest::Question { questions });
                            }
                        }
                    }
                    Some("todowrite") => {
                        if let Some(input) = part.get("input") {
                            if let Ok(todos) = serde_json::from_value::<Vec<TodoItem>>(
                                input.get("todos").cloned().unwrap_or(serde_json::Value::Null),
                            ) {
                                return Some(InteractionRequest::TodoWrite { todos });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Some("permission") | Some("ask_permission") => {
                let tool_name = part
                    .get("toolName")
                    .or_else(|| part.get("tool_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let action = part.get("action").and_then(|v| v.as_str()).unwrap_or("");
                return Some(InteractionRequest::Permission {
                    tool_name: tool_name.to_string(),
                    action: action.to_string(),
                });
            }
            _ => {}
        }
    }
    None
}
