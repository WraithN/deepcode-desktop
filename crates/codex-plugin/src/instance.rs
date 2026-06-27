use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus, UNKNOWN_PID};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::mapper::{emit_status_changed, EventMapper};
use agent_core::session_map::ConversationSessionMap;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::{mpsc, MutexGuard as TokioMutexGuard};

use crate::client::CodexClient;
use crate::constants::*;
use crate::parser::{extract_thread_id, parse_codex_notification};

pub struct CodexInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    client: Arc<TokioMutex<Option<CodexClient>>>,
    status: Arc<Mutex<InstanceStatus>>,
    started: Arc<AtomicBool>,
    startup_lock: Arc<TokioMutex<()>>,
    session_map: ConversationSessionMap,
    shutdown: Arc<AtomicBool>,
}

impl CodexInstance {
    pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
        Self {
            config,
            event_sink,
            logger,
            client: Arc::new(TokioMutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            started: Arc::new(AtomicBool::new(false)),
            startup_lock: Arc::new(TokioMutex::new(())),
            session_map: ConversationSessionMap::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    fn emit_status(&self, status: InstanceStatus) {
        emit_status_changed(&self.event_sink, &self.config.id, status);
    }

    fn set_status(&self, status: InstanceStatus) {
        *self.status.lock().unwrap() = status.clone();
        self.emit_status(status);
    }

    async fn client_guard(&self) -> TokioMutexGuard<'_, Option<CodexClient>> {
        self.client.lock().await
    }

    async fn ensure_started(&self) -> Result<(), InstanceError> {
        let _guard = self.startup_lock.lock().await;

        if self.client_guard().await.is_some() {
            return Ok(());
        }

        self.shutdown.store(false, Ordering::SeqCst);
        self.set_status(InstanceStatus::Starting);

        let (notification_tx, notification_rx) = mpsc::channel::<Value>(1000);
        let mut client = CodexClient::new(&self.config.workspace, notification_tx);
        client
            .start(&self.config.workspace)
            .await
            .map_err(|e| InstanceError::ProcessError(format!("{}: {}", ERR_START_FAILED, e)))?;

        *self.client_guard().await = Some(client);
        self.started.store(true, Ordering::SeqCst);
        self.set_status(InstanceStatus::Running { pid: UNKNOWN_PID });

        self.spawn_notification_handler(notification_rx);

        self.logger.log(
            &self.config.id,
            LogLevel::Info,
            LOG_SOURCE,
            LOG_STARTED,
            None,
            Some(self.config.id.clone()),
        );

        Ok(())
    }

    fn spawn_notification_handler(&self, mut rx: mpsc::Receiver<Value>) {
        let instance_id = self.config.id.clone();
        let session_map = self.session_map.clone();
        let event_sink = self.event_sink.clone();

        tokio::spawn(async move {
            while let Some(value) = rx.recv().await {
                if let Some(event) = parse_codex_notification(&value) {
                    let conversation_id = session_map
                        .conversation_for_session(&instance_id)
                        .unwrap_or_default();
                    let mapper = EventMapper::new(instance_id.clone(), conversation_id);
                    mapper.map(event, &event_sink);
                }
            }
        });
    }

    async fn thread_for_conversation(&self, conversation_id: &str) -> Result<String, InstanceError> {
        if let Some(thread_id) = self.session_map.session_for_conversation(conversation_id) {
            return Ok(thread_id);
        }

        let response = {
            let mut guard = self.client_guard().await;
            let client = guard.as_mut().ok_or_else(|| {
                InstanceError::ProcessError(ERR_NOT_INITIALIZED.to_string())
            })?;
            client
                .request(METHOD_THREAD_START, json!({}))
                .await
                .map_err(|e| InstanceError::ProcessError(format!("{}: {}", ERR_THREAD_START_FAILED, e)))?
        };

        let thread_id = extract_thread_id(&response).ok_or_else(|| {
            InstanceError::ProcessError(format!("{}: missing thread_id in response", ERR_THREAD_START_FAILED))
        })?;

        self.session_map.insert(conversation_id, &thread_id);
        Ok(thread_id)
    }
}

impl AgentInstance for CodexInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn plugin_key(&self) -> &'static str {
        PLUGIN_KEY
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
            let thread_id = self.thread_for_conversation(&conversation_id).await?;

            let mut guard = self.client_guard().await;
            let client = guard.as_mut().ok_or_else(|| {
                InstanceError::ProcessError(ERR_NOT_INITIALIZED.to_string())
            })?;

            client
                .request(
                    METHOD_TURN_START,
                    json!({
                        KEY_THREAD_ID: thread_id,
                        KEY_INPUT: [
                            { KEY_TYPE: "text", KEY_TEXT: message }
                        ]
                    }),
                )
                .await
                .map_err(|e| InstanceError::SendFailed(format!("{}: {}", ERR_SEND_FAILED, e)))?;

            Ok(())
        })
    }

    fn respond(
        &self,
        session_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        // Codex app-server uses turn/start for follow-up messages as well.
        self.send_message(session_id, message)
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        self.shutdown.store(true, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(mut client) = self.client_guard().await.take() {
                let _ = client.close().await;
            }

            self.started.store(false, Ordering::SeqCst);
            self.set_status(InstanceStatus::Stopped);

            self.logger.log(
                &self.config.id,
                LogLevel::Info,
                LOG_SOURCE,
                LOG_STOPPED,
                None,
                Some(self.config.id.clone()),
            );

            Ok(())
        })
    }
}
