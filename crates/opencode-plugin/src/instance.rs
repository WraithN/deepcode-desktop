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
use crate::transport::{connect_opencode_sse, find_available_port, start_opencode_process, OpenCodeClient};

const LOG_SOURCE: &str = "opencode-plugin";
const LOCALHOST: &str = "http://127.0.0.1";
const STARTUP_WAIT_COUNT: u32 = 20;
const STARTUP_WAIT_MS: u64 = 500;
const READY_POLL_COUNT: u32 = 20;
const READY_POLL_MS: u64 = 200;
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
const ERR_SERVE_NOT_READY_PORT_PREFIX: &str = "opencode serve did not become ready on port ";
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
        }
    }

    fn emit_status(&self, status: InstanceStatus) {
        emit_status_changed(&self.event_sink, &self.config.id, status);
    }

    fn base_url(&self) -> Option<String> {
        self.base_url.lock().unwrap().clone()
    }

    /// Starts `opencode serve` and the SSE listener (idempotent).
    async fn ensure_started(&self) -> Result<(), InstanceError> {
        if self
            .started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            for _ in 0..READY_POLL_COUNT {
                if self.base_url().is_some() {
                    return Ok(());
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(READY_POLL_MS)).await;
            }
            return Err(InstanceError::NotRunning(ERR_SERVE_NOT_READY.into()));
        }

        let port = find_available_port().map_err(InstanceError::ProcessError)?;
        let base_url = format!("{}:{}", LOCALHOST, port);

        let mut child = start_opencode_process(port)?;

        let client = OpenCodeClient::new(&base_url);
        let mut ready = false;
        for _ in 0..STARTUP_WAIT_COUNT {
            tokio::time::sleep(tokio::time::Duration::from_millis(STARTUP_WAIT_MS)).await;
            if client.health_check().await {
                ready = true;
                break;
            }
        }

        if !ready {
            let _ = child.start_kill();
            self.started.store(false, Ordering::SeqCst);
            return Err(InstanceError::ProcessError(format!(
                "{}{}",
                ERR_SERVE_NOT_READY_PORT_PREFIX, port
            )));
        }

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
                if let Some(event) = crate::mapper::map_opencode_sse(&payload) {
                    let session_id = crate::mapper::extract_session_id(&payload).unwrap_or_default();
                    let conversation_id = session_map
                        .conversation_for_session(&session_id)
                        .unwrap_or_default();
                    let mapper = EventMapper::new(instance_id.clone(), conversation_id);
                    mapper.map(event, &event_sink);
                }
            }
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

    fn plugin_key(&self) -> &'static str {
        PLUGIN_KEY
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
