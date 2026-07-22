use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus, UNKNOWN_PID};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::mapper::{emit_status_changed, EventMapper};
use agent_core::process::transport::TransportHandle;
use agent_core::session_map::ConversationSessionMap;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

use crate::mapper::{detect_interaction_from_parts, InteractionRequest};
use crate::transport::{connect_opencode_sse, port_allocator, start_opencode_process, OpenCodeClient};

const LOG_SOURCE: &str = "opencode-plugin";
const LOCALHOST: &str = "http://127.0.0.1";
const STARTUP_WAIT_COUNT: u32 = 20;
const STARTUP_WAIT_MS: u64 = 500;
const SSE_CHANNEL_CAPACITY: usize = 1000;

const METHOD_QUESTION: &str = "agent.question";
const METHOD_PERMISSION: &str = "agent.permission";
const METHOD_TODO_WRITE: &str = "agent.todowrite";

const KEY_INSTANCE_ID: &str = "instance_id";
const KEY_CONVERSATION_ID: &str = "conversation_id";
const KEY_SESSION_ID: &str = "sessionID";
const KEY_INTERACTION: &str = "interaction";
const KEY_PARTS: &str = "parts";
const KEY_INFO: &str = "info";

const PLUGIN_KEY: &str = "opencode";

const ERR_SERVE_NOT_READY: &str = "opencode serve did not become ready";
const ERR_SERVE_NOT_STARTED: &str = "opencode serve not started";
const LOG_SERVE_STARTED_PREFIX: &str = "opencode serve started on ";

pub struct OpencodeInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    base_url: Mutex<Option<String>>,
    serve_process: Arc<TokioMutex<Option<tokio::process::Child>>>,
    status: Arc<Mutex<InstanceStatus>>,
    started: Arc<AtomicBool>,
    session_map: ConversationSessionMap,
    transport_handle: Arc<TokioMutex<Option<Box<dyn TransportHandle>>>>,
    startup_lock: Arc<TokioMutex<()>>,
}

impl OpencodeInstance {
    pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            event_sink,
            logger,
            base_url: Mutex::new(None),
            serve_process: Arc::new(TokioMutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            started: Arc::new(AtomicBool::new(false)),
            session_map: ConversationSessionMap::new(),
            transport_handle: Arc::new(TokioMutex::new(None)),
            startup_lock: Arc::new(TokioMutex::new(())),
        }
    }

    fn emit_status(&self, status: InstanceStatus) {
        emit_status_changed(&self.event_sink, &self.config.id, status);
    }

    fn base_url(&self) -> Option<String> {
        self.base_url.lock().unwrap().clone()
    }

    /// Attempt to start `opencode serve` on a port allocated by the global
    /// allocator.  Retries a few times if the port is occupied or the server
    /// does not become healthy, reducing the impact of the TOCTOU race in
    /// port allocation.
    async fn start_opencode_with_retry(
        work_directory: &str,
    ) -> Result<(u16, tokio::process::Child, OpenCodeClient), InstanceError> {
        const MAX_RETRIES: u32 = 5;

        for attempt in 0..MAX_RETRIES {
            let port = port_allocator().allocate();
            let base_url = format!("{}:{}", LOCALHOST, port);
            let client = OpenCodeClient::new(&base_url);

            match start_opencode_process(port, work_directory) {
                Ok(mut child) => {
                    let mut ready = false;
                    for _ in 0..STARTUP_WAIT_COUNT {
                        tokio::time::sleep(tokio::time::Duration::from_millis(STARTUP_WAIT_MS))
                            .await;
                        if client.health_check().await {
                            ready = true;
                            break;
                        }
                    }

                    if ready {
                        match child.try_wait() {
                            Ok(None) => return Ok((port, child, client)),
                            Ok(Some(status)) => {
                                log::warn!(
                                    "opencode on port {} exited during startup (status: {}), \
                                     port may be occupied by another process, retrying (attempt {})",
                                    port, status, attempt + 1
                                );
                            }
                            Err(e) => {
                                log::warn!(
                                    "failed to check child status on port {}: {}, retrying (attempt {})",
                                    port, e, attempt + 1
                                );
                                let _ = child.start_kill();
                                let _ = child.wait().await;
                            }
                        }
                    } else {
                        log::warn!(
                            "opencode serve did not become healthy on port {} (attempt {}), retrying",
                            port,
                            attempt + 1
                        );
                        let _ = child.start_kill();
                        let _ = child.wait().await;
                    }
                }
                Err(e) => {
                    log::warn!(
                        "failed to start opencode on port {} (attempt {}): {}",
                        port,
                        attempt + 1,
                        e
                    );
                }
            }
        }

        Err(InstanceError::ProcessError(format!(
            "{} after {} attempts",
            ERR_SERVE_NOT_READY, MAX_RETRIES
        )))
    }

    /// Starts `opencode serve` and the SSE listener (idempotent).
    async fn ensure_started(&self) -> Result<(), InstanceError> {
        let _guard = self.startup_lock.lock().await;

        if self.base_url().is_some() {
            return Ok(());
        }

        let (port, child, client) =
            Self::start_opencode_with_retry(&self.config.work_directory).await?;
        let base_url = format!("{}:{}", LOCALHOST, port);

        *self.base_url.lock().unwrap() = Some(base_url.clone());
        *self.serve_process.lock().await = Some(child);
        *self.status.lock().unwrap() = InstanceStatus::Running { pid: UNKNOWN_PID };
        self.emit_status(InstanceStatus::Running { pid: UNKNOWN_PID });

        self.logger.log(
            &self.config.id,
            LogLevel::Info,
            LOG_SOURCE,
            &format!("{}{}", LOG_SERVE_STARTED_PREFIX, base_url),
            None,
            Some(self.config.id.clone()),
        );

        let (tx, mut rx) = tokio::sync::mpsc::channel::<serde_json::Value>(SSE_CHANNEL_CAPACITY);
        let handle = connect_opencode_sse(&base_url, client.client().clone(), &self.config.id, tx)
            .await?;
        *self.transport_handle.lock().await = Some(handle);

        let event_sink = self.event_sink.clone();
        let instance_id = self.config.id.clone();
        let session_map = self.session_map.clone();
        tokio::spawn(async move {
            while let Some(payload) = rx.recv().await {
                let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let events = crate::mapper::map_opencode_sse(&payload);
                if !events.is_empty() {
                    let session_id = crate::mapper::extract_session_id(&payload).unwrap_or_default();
                    let conversation_id = session_map
                        .conversation_for_session(&session_id)
                        .unwrap_or_default();
                    log::info!("[relay-loop] ev={event_type} oc_sid={session_id} gw_sid={conversation_id} mapped={}", events.len());
                    for event in events {
                        let mapper = EventMapper::new(instance_id.clone(), conversation_id.clone());
                        mapper.map(event, &event_sink);
                    }
                }
            }
            log::warn!("[relay-loop] exited instance={instance_id}");
        });

        Ok(())
    }

    async fn create_opencode_session(&self) -> Result<String, InstanceError> {
        let base = self.base_url().ok_or_else(|| {
            InstanceError::NotRunning(ERR_SERVE_NOT_STARTED.into())
        })?;
        OpenCodeClient::new(base).create_session().await
    }

    async fn send_message_http(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let base = self.base_url().ok_or_else(|| {
            InstanceError::NotRunning(ERR_SERVE_NOT_STARTED.into())
        })?;
        OpenCodeClient::new(base)
            .send_message(session_id, message)
            .await
    }

    fn find_session_for_conversation(&self, conversation_id: &str) -> Option<String> {
        self.session_map.session_for_conversation(conversation_id)
    }

    fn store_session(&self, conversation_id: &str, session_id: &str) {
        self.session_map.insert(conversation_id, session_id);
    }

    fn emit_interaction(
        &self,
        method: &str,
        session_id: &str,
        conversation_id: &str,
        interaction: &InteractionRequest,
    ) {
        let interaction_json = serde_json::to_value(interaction).unwrap_or_default();
        self.event_sink.emit(
            method,
            json!({
                KEY_SESSION_ID: session_id,
                KEY_INTERACTION: interaction_json,
                KEY_CONVERSATION_ID: conversation_id,
                KEY_INSTANCE_ID: self.config.id,
            }),
        );
    }

    async fn reset_and_restart(&self) -> Result<(), InstanceError> {
        if let Some(mut child) = self.serve_process.lock().await.take() {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        if let Some(mut handle) = self.transport_handle.lock().await.take() {
            let _ = handle.close().await;
        }
        self.session_map.clear();
        *self.base_url.lock().unwrap() = None;
        *self.status.lock().unwrap() = InstanceStatus::Stopped;
        self.started.store(false, Ordering::SeqCst);
        self.emit_status(InstanceStatus::Stopped);
        self.ensure_started().await
    }

    fn detect_and_emit_interaction(
        &self,
        result: &serde_json::Value,
        conversation_id: &str,
        fallback_session_id: &str,
    ) {
        let parts = match result.get(KEY_PARTS).and_then(|v| v.as_array()) {
            Some(p) => p,
            None => return,
        };
        let interaction = match detect_interaction_from_parts(parts) {
            Some(i) => i,
            None => return,
        };
        let method = match &interaction {
            InteractionRequest::Question { .. } => METHOD_QUESTION,
            InteractionRequest::Permission { .. } => METHOD_PERMISSION,
            InteractionRequest::TodoWrite { .. } => METHOD_TODO_WRITE,
        };
        let session_id = result
            .get(KEY_INFO)
            .and_then(|i| i.get(KEY_SESSION_ID))
            .and_then(|v| v.as_str())
            .unwrap_or(fallback_session_id)
            .to_string();
        self.emit_interaction(method, &session_id, conversation_id, &interaction);
    }
}

impl AgentInstance for OpencodeInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn agent_key(&self) -> &'static str {
        PLUGIN_KEY
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn work_directory(&self) -> &str {
        &self.config.work_directory
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

            let mut session_id = match self.find_session_for_conversation(&conversation_id) {
                Some(sid) => sid,
                None => {
                    let sid = self.create_opencode_session().await?;
                    self.store_session(&conversation_id, &sid);
                    sid
                }
            };

            let result = match self.send_message_http(&session_id, &message).await {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("send_message_http failed, resetting and retrying: {e}");
                    self.reset_and_restart().await?;
                    session_id = self.create_opencode_session().await?;
                    self.store_session(&conversation_id, &session_id);
                    self.send_message_http(&session_id, &message).await?
                }
            };

            self.detect_and_emit_interaction(&result, &conversation_id, &session_id);

            self.session_map.insert(&conversation_id, &session_id);
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
            match self.send_message_http(&session_id, &message).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    log::warn!("respond send_message_http failed, resetting and retrying: {e}");
                    self.reset_and_restart().await?;
                    self.send_message_http(&session_id, &message).await?;
                    Ok(())
                }
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        Box::pin(async move {
            if let Some(mut child) = self.serve_process.lock().await.take() {
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            if let Some(mut handle) = self.transport_handle.lock().await.take() {
                let _ = handle.close().await;
            }
            *self.status.lock().unwrap() = InstanceStatus::Stopped;
            self.emit_status(InstanceStatus::Stopped);
            Ok(())
        })
    }
}
