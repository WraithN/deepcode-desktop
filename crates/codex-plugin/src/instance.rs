use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus, UNKNOWN_PID};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::event::ProcessEvent;
use agent_core::process::mapper::{EventMapper, emit_status_changed};
use agent_core::process::stdio::StdioTransport;
use agent_core::process::transport::{Transport, TransportHandle};
use agent_core::session_map::ConversationSessionMap;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::MutexGuard as TokioMutexGuard;
use tokio::time::{Duration, timeout};

use crate::constants::*;
use crate::parser::{extract_thread_id, parse_codex_value};

/// A running Codex app-server instance.
pub struct CodexInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    transport: Arc<TokioMutex<Option<Box<dyn TransportHandle>>>>,
    status: Arc<Mutex<InstanceStatus>>,
    started: Arc<AtomicBool>,
    session_map: ConversationSessionMap,
    active_thread_id: Arc<Mutex<Option<String>>>,
    startup_lock: Arc<TokioMutex<()>>,
    out_tx: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Value>>>,
    out_rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Value>>>,
    shutdown: Arc<AtomicBool>,
    request_counter: Arc<AtomicI64>,
    pending_requests: Arc<Mutex<HashMap<i64, tokio::sync::oneshot::Sender<Value>>>>,
}

impl Clone for CodexInstance {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_sink: self.event_sink.clone(),
            logger: self.logger.clone(),
            transport: self.transport.clone(),
            status: self.status.clone(),
            started: self.started.clone(),
            session_map: self.session_map.clone(),
            active_thread_id: self.active_thread_id.clone(),
            startup_lock: self.startup_lock.clone(),
            out_tx: Mutex::new(self.out_tx.lock().unwrap().clone()),
            out_rx: Mutex::new(None),
            shutdown: self.shutdown.clone(),
            request_counter: self.request_counter.clone(),
            pending_requests: self.pending_requests.clone(),
        }
    }
}

impl CodexInstance {
    pub fn new(
        config: InstanceConfig,
        event_sink: DynEventSink,
        logger: Arc<SessionLogger>,
    ) -> Self {
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        Self {
            config,
            event_sink,
            logger,
            transport: Arc::new(TokioMutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            started: Arc::new(AtomicBool::new(false)),
            session_map: ConversationSessionMap::new(),
            active_thread_id: Arc::new(Mutex::new(None)),
            startup_lock: Arc::new(TokioMutex::new(())),
            out_tx: Mutex::new(Some(out_tx)),
            out_rx: Mutex::new(Some(out_rx)),
            shutdown: Arc::new(AtomicBool::new(false)),
            request_counter: Arc::new(AtomicI64::new(1)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self) -> Result<(), InstanceError> {
        self.ensure_started().await
    }

    pub fn start_in_background(&self) {
        let instance = self.clone();
        tokio::spawn(async move {
            if let Err(e) = instance.start().await {
                log::error!(
                    "{}: failed to start codex instance {}: {}",
                    LOG_SOURCE,
                    instance.id(),
                    e
                );
            }
        });
    }

    fn active_thread(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.active_thread_id.lock().unwrap()
    }

    async fn transport_guard(&self) -> TokioMutexGuard<'_, Option<Box<dyn TransportHandle>>> {
        self.transport.lock().await
    }

    fn emit_status(&self, status: InstanceStatus) {
        emit_status_changed(&self.event_sink, &self.config.id, status);
    }

    fn set_status(&self, status: InstanceStatus) {
        *self.status.lock().unwrap() = status.clone();
        self.emit_status(status);
    }

    fn next_request_id(&self) -> i64 {
        self.request_counter.fetch_add(1, Ordering::SeqCst)
    }

    fn build_transport(&self) -> StdioTransport {
        let args = vec![APP_SERVER_SUBCOMMAND.into(), STDIO_FLAG.into()];
        StdioTransport::new(PROGRAM_CODEX, args, self.config.work_directory.clone())
    }

    async fn ensure_started(&self) -> Result<(), InstanceError> {
        let _guard = self.startup_lock.lock().await;

        if self.transport_guard().await.is_some() {
            return Ok(());
        }

        self.shutdown.store(false, Ordering::SeqCst);
        self.set_status(InstanceStatus::Starting);

        let transport = self.build_transport();
        let handle = transport
            .start()
            .await
            .map_err(|e| InstanceError::ProcessError(format!("{}: {}", ERR_START_FAILED, e)))?;

        *self.transport_guard().await = Some(handle);

        if let Some(out_rx) = self.out_rx.lock().unwrap().take() {
            self.spawn_reader(out_rx);
        }

        self.initialize().await?;
        self.start_thread().await?;

        self.started.store(true, Ordering::SeqCst);
        self.set_status(InstanceStatus::Running { pid: UNKNOWN_PID });

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

    /// Performs the app-server initialize handshake.
    async fn initialize(&self) -> Result<(), InstanceError> {
        let init_req = json!({
            "method": METHOD_INITIALIZE,
            "id": 0,
            "params": {
                "clientInfo": {
                    "name": "deepharness-gatewayd",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        let _response = self
            .send_request_and_wait(0, init_req)
            .await
            .map_err(|_| InstanceError::ProcessError(ERR_INIT_TIMEOUT.into()))?;

        let initialized_note = json!({
            "method": METHOD_INITIALIZED,
            "params": {}
        });
        self.do_send_raw(initialized_note)?;
        Ok(())
    }

    /// Starts a new Codex thread and records its id.
    async fn start_thread(&self) -> Result<(), InstanceError> {
        let mut params = serde_json::Map::new();
        params.insert("cwd".to_string(), json!(self.config.work_directory));

        if let Some(model) = &self.config.model {
            params.insert("model".to_string(), json!(model));
        }
        if let Some(permission) = &self.config.permission_mode {
            params.insert("approvalPolicy".to_string(), json!(permission));
        }

        let id = self.next_request_id();
        let req = json!({ "method": METHOD_THREAD_START, "id": id, "params": params });

        let response = self
            .send_request_and_wait(id, req)
            .await
            .map_err(|_| InstanceError::ProcessError(ERR_INIT_TIMEOUT.into()))?;

        let thread_id = response
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| extract_thread_id(&parse_codex_value(&response).unwrap_or_default()));

        if let Some(tid) = thread_id {
            *self.active_thread() = Some(tid);
        }

        Ok(())
    }

    /// Sends a JSON-RPC request and waits for the matching response.
    async fn send_request_and_wait(&self, id: i64, payload: Value) -> Result<Value, InstanceError> {
        let (tx, rx) = tokio::sync::oneshot::channel::<Value>();
        self.pending_requests.lock().unwrap().insert(id, tx);

        self.do_send_raw(payload)?;

        match timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS), rx).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) | Err(_) => Err(InstanceError::ProcessError(ERR_INIT_TIMEOUT.into())),
        }
    }

    fn do_send_raw(&self, payload: Value) -> Result<(), InstanceError> {
        let tx = self
            .out_tx
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| InstanceError::SendFailed(ERR_SEND_FAILED.into()))?;
        tx.send(payload)
            .map_err(|_| InstanceError::SendFailed(ERR_SEND_FAILED.into()))
    }

    fn spawn_reader(&self, mut out_rx: tokio::sync::mpsc::UnboundedReceiver<Value>) {
        let instance = self.clone();

        tokio::spawn(async move {
            loop {
                if instance.shutdown.load(Ordering::Relaxed) {
                    break;
                }

                let try_out = out_rx.try_recv();
                if matches!(
                    try_out,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
                ) {
                    break;
                }

                let mut guard = instance.transport.lock().await;
                let Some(handle) = guard.as_mut() else {
                    break;
                };

                if let Ok(payload) = try_out {
                    Self::send_outgoing_or_log(handle, payload).await;
                }

                let next = Self::receive_with_timeout(handle).await;
                drop(guard);

                if let Some(value) = next {
                    instance.dispatch_message(value);
                } else {
                    tokio::task::yield_now().await;
                }
            }
        });
    }

    async fn send_outgoing_or_log(handle: &mut Box<dyn TransportHandle>, payload: Value) {
        if let Err(e) = handle.send(payload).await {
            log::debug!("{}: outgoing send failed: {e}", LOG_SOURCE);
        }
    }

    async fn receive_with_timeout(handle: &mut Box<dyn TransportHandle>) -> Option<Value> {
        match timeout(Duration::from_millis(RECEIVE_TIMEOUT_MS), handle.receive()).await {
            Ok(Ok(value)) => Some(value),
            Ok(Err(_)) | Err(_) => None,
        }
    }

    fn dispatch_message(&self, value: Value) {
        let Some(msg) = parse_codex_value(&value) else {
            log::debug!("{}: failed to parse codex line: {}", LOG_SOURCE, value);
            return;
        };

        // Route JSON-RPC responses to pending request channels.
        if msg.is_response() {
            if let Some(id) = msg.response_id() {
                if let Some(tx) = self.pending_requests.lock().unwrap().remove(&id) {
                    let _ = tx.send(value);
                }
            }
            return;
        }

        // Update active thread id from notifications/results when available.
        if let Some(tid) = extract_thread_id(&msg) {
            let mut active = self.active_thread();
            if active.as_deref() != Some(tid.as_str()) {
                *active = Some(tid);
            }
        }

        let Some(event) = crate::parser::to_process_event(&msg) else {
            return;
        };

        if let ProcessEvent::Error { message } = &event {
            self.logger.log(
                &self.config.id,
                LogLevel::Error,
                LOG_SOURCE,
                message,
                Some(value),
                Some(self.config.id.clone()),
            );
        }

        let conversation_id = self
            .active_thread()
            .clone()
            .and_then(|tid| self.session_map.conversation_for_session(&tid))
            .unwrap_or_default();

        let mapper = EventMapper::new(self.config.id.clone(), conversation_id);
        mapper.map(event, &self.event_sink);
    }

    fn build_turn_start_payload(thread_id: &str, message: &str) -> Value {
        json!({
            "method": METHOD_TURN_START,
            "id": 0,
            "params": {
                "threadId": thread_id,
                "input": [{ "type": "text", "text": message }]
            }
        })
    }

    fn resolve_thread_for_conversation(
        &self,
        conversation_id: &str,
    ) -> Result<String, InstanceError> {
        self.session_map
            .session_for_conversation(conversation_id)
            .or_else(|| self.active_thread().clone())
            .ok_or_else(|| InstanceError::NotRunning(ERR_NO_ACTIVE_THREAD.into()))
    }
}

impl AgentInstance for CodexInstance {
    fn id(&self) -> &str {
        &self.config.id
    }

    fn status(&self) -> InstanceStatus {
        self.status.lock().unwrap().clone()
    }

    fn agent_key(&self) -> &'static str {
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
            let thread_id = self.resolve_thread_for_conversation(&conversation_id)?;
            self.session_map.insert(&conversation_id, &thread_id);

            let id = self.next_request_id();
            let payload = Self::build_turn_start_payload(&thread_id, &message);
            let _response = self.send_request_and_wait(id, payload).await;
            Ok(())
        })
    }

    fn respond(
        &self,
        _session_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let message = message.to_string();

        Box::pin(async move {
            self.ensure_started().await?;
            let thread_id = self
                .active_thread()
                .clone()
                .ok_or_else(|| InstanceError::NotRunning(ERR_NO_ACTIVE_THREAD.into()))?;
            let id = self.next_request_id();
            let payload = Self::build_turn_start_payload(&thread_id, &message);
            let _response = self.send_request_and_wait(id, payload).await;
            Ok(())
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        self.shutdown.store(true, Ordering::SeqCst);
        Box::pin(async move {
            if let Some(mut handle) = self.transport_guard().await.take() {
                let _ = handle.close().await;
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
