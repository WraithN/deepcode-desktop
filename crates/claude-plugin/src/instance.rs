use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus, UNKNOWN_PID};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::event::ProcessEvent;
use agent_core::process::mapper::{emit_status_changed, EventMapper};
use agent_core::process::stdio::StdioTransport;
use agent_core::process::transport::{Transport, TransportError, TransportHandle};
use agent_core::session_map::ConversationSessionMap;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::MutexGuard as TokioMutexGuard;
use tokio::time::{timeout, Duration};

use crate::constants::*;
use crate::parser::{parse_claude_value, ClaudeRawEvent};

pub struct ClaudeInstance {
    config: InstanceConfig,
    event_sink: DynEventSink,
    logger: Arc<SessionLogger>,
    transport: Arc<TokioMutex<Option<Box<dyn TransportHandle>>>>,
    status: Arc<Mutex<InstanceStatus>>,
    started: Arc<AtomicBool>,
    session_map: ConversationSessionMap,
    active_session_id: Arc<Mutex<Option<String>>>,
    startup_lock: Arc<TokioMutex<()>>,
    out_tx: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Value>>>,
    out_rx: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Value>>>,
    shutdown: Arc<AtomicBool>,
    // 用于 TTFT 调试：记录最近一次 send_message 的开始时间，以及是否已打印首事件日志。
    run_start: Arc<Mutex<Option<std::time::Instant>>>,
    first_raw_event: Arc<AtomicBool>,
    first_token_event: Arc<AtomicBool>,
}

impl Clone for ClaudeInstance {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            event_sink: self.event_sink.clone(),
            logger: self.logger.clone(),
            transport: self.transport.clone(),
            status: self.status.clone(),
            started: self.started.clone(),
            session_map: self.session_map.clone(),
            active_session_id: self.active_session_id.clone(),
            startup_lock: self.startup_lock.clone(),
            out_tx: Mutex::new(self.out_tx.lock().unwrap().clone()),
            out_rx: Mutex::new(None),
            shutdown: self.shutdown.clone(),
            run_start: self.run_start.clone(),
            first_raw_event: self.first_raw_event.clone(),
            first_token_event: self.first_token_event.clone(),
        }
    }
}

impl ClaudeInstance {
    pub fn new(config: InstanceConfig, event_sink: DynEventSink, logger: Arc<SessionLogger>) -> Self {
        let (out_tx, out_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();

        Self {
            config,
            event_sink,
            logger,
            transport: Arc::new(TokioMutex::new(None)),
            status: Arc::new(Mutex::new(InstanceStatus::Stopped)),
            started: Arc::new(AtomicBool::new(false)),
            session_map: ConversationSessionMap::new(),
            active_session_id: Arc::new(Mutex::new(None)),
            startup_lock: Arc::new(TokioMutex::new(())),
            out_tx: Mutex::new(Some(out_tx)),
            out_rx: Mutex::new(Some(out_rx)),
            shutdown: Arc::new(AtomicBool::new(false)),
            run_start: Arc::new(Mutex::new(None)),
            first_raw_event: Arc::new(AtomicBool::new(false)),
            first_token_event: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the Claude process lazily before the first user message.
    pub async fn start(&self) -> Result<(), InstanceError> {
        self.ensure_started().await
    }

    fn active_session(&self) -> MutexGuard<'_, Option<String>> {
        self.active_session_id.lock().unwrap()
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

    fn build_transport(&self) -> StdioTransport {
        let mut args: Vec<String> = vec![
            PROMPT_FLAG.into(),
            INPUT_FORMAT_FLAG.into(),
            OUTPUT_FORMAT_FLAG.into(),
            VERBOSE_FLAG.into(),
            INCLUDE_PARTIAL_MESSAGES_FLAG.into(),
        ];

        let permission_mode = self
            .config
            .permission_mode
            .as_deref()
            .unwrap_or(DEFAULT_PERMISSION_MODE);
        args.push(format!("{}{}", PERMISSION_MODE_PREFIX, permission_mode));

        if let Some(model) = &self.config.model {
            args.push(format!("{}{}", MODEL_PREFIX, model));
        }

        if let Some(session_id) = &self.config.session_id {
            args.push(format!("{}{}", RESUME_PREFIX, session_id));
        }

        StdioTransport::new(PROGRAM_CLAUDE, args, self.config.workspace.clone())
    }

    /// Start the Claude process and mark the instance running.
    async fn ensure_started(&self) -> Result<(), InstanceError> {
        let _guard = self.startup_lock.lock().await;

        if self.transport_guard().await.is_some() {
            return Ok(());
        }

        self.shutdown.store(false, Ordering::SeqCst);
        self.set_status(InstanceStatus::Starting);

        let start = std::time::Instant::now();
        log::info!(
            "[claude-plugin] instance={} starting Claude process...",
            self.config.id
        );
        let transport = self.build_transport();
        let handle = transport
            .start()
            .await
            .map_err(|e| InstanceError::ProcessError(format!("{}: {}", ERR_START_FAILED, e)))?;
        log::info!(
            "[claude-plugin] instance={} Claude process started after {:?}",
            self.config.id,
            start.elapsed()
        );

        *self.transport_guard().await = Some(handle);
        self.started.store(true, Ordering::SeqCst);
        self.set_status(InstanceStatus::Running { pid: UNKNOWN_PID });

        if let Some(out_rx) = self.out_rx.lock().unwrap().take() {
            self.spawn_reader(out_rx, self.transport.clone(), self.shutdown.clone());
        }

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

    fn build_user_message_payload(message: &str) -> Value {
        json!({
            KEY_TYPE: ROLE_USER,
            KEY_MESSAGE: {
                KEY_ROLE: ROLE_USER,
                KEY_CONTENT: [
                    { KEY_TYPE: CONTENT_TYPE_TEXT, KEY_TEXT: message }
                ]
            }
        })
    }

    fn do_send(&self, payload: Value) -> Result<(), InstanceError> {
        let tx = self
            .out_tx
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| InstanceError::SendFailed(ERR_SEND_FAILED.into()))?;
        tx.send(payload)
            .map_err(|_| InstanceError::SendFailed(ERR_SEND_FAILED.into()))
    }

    fn spawn_reader(
        &self,
        mut out_rx: tokio::sync::mpsc::UnboundedReceiver<Value>,
        transport: Arc<TokioMutex<Option<Box<dyn TransportHandle>>>>,
        shutdown: Arc<AtomicBool>,
    ) {
        let instance = self.clone();

        tokio::spawn(async move {
            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }

                let try_payload = out_rx.try_recv();
                if matches!(
                    try_payload,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
                ) {
                    break;
                }

                let mut guard = transport.lock().await;
                let Some(handle) = guard.as_mut() else {
                    break;
                };

                if let Ok(payload) = try_payload {
                    Self::send_outgoing_or_log(handle, payload).await;
                }

                let next = Self::receive_with_timeout(handle).await;
                drop(guard);

                if let Some(value) = next {
                    instance.process_received_value(value);
                } else {
                    // Allow checking shutdown and outgoing queue between receives.
                    tokio::task::yield_now().await;
                }
            }
        });
    }

    async fn send_outgoing(
        handle: &mut Box<dyn TransportHandle>,
        payload: Value,
    ) -> Result<(), TransportError> {
        handle.send(payload).await
    }

    async fn send_outgoing_or_log(handle: &mut Box<dyn TransportHandle>, payload: Value) {
        if let Err(e) = Self::send_outgoing(handle, payload).await {
            log::debug!("{}: outgoing send failed: {e}", LOG_SOURCE);
        }
    }

    async fn receive_with_timeout(handle: &mut Box<dyn TransportHandle>) -> Option<Value> {
        match timeout(Duration::from_millis(RECEIVE_TIMEOUT_MS), handle.receive()).await {
            Ok(Ok(value)) => Some(value),
            Ok(Err(_)) | Err(_) => None,
        }
    }

    /// Routes a single raw JSON value from Claude:
    /// 1. Updates the active session mapping from `system/init` or `result` events.
    /// 2. Converts the raw event to a normalized `ProcessEvent`.
    /// 3. Logs error events through the session logger.
    /// 4. Maps the event to frontend-facing events via `EventMapper`, using the
    ///    conversation id currently associated with the active session.
    fn process_received_value(&self, value: Value) {
        if !self.first_raw_event.swap(true, Ordering::SeqCst) {
            if let Some(start) = *self.run_start.lock().unwrap() {
                let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
                let subtype = value.get("subtype").and_then(|v| v.as_str()).unwrap_or("-");
                log::info!(
                    "[claude-plugin] instance={} first raw event from Claude CLI after {:?}: type={} subtype={}",
                    self.config.id,
                    start.elapsed(),
                    event_type,
                    subtype
                );
            }
        }

        let Some(raw) = parse_claude_value(&value) else {
            log::debug!("{}: failed to parse line: {}", LOG_SOURCE, value);
            return;
        };

        if let Some(session_id) = extract_session_id(&raw) {
            let mut active = self.active_session();
            if active.as_deref() != Some(session_id.as_str()) {
                *active = Some(session_id);
            }
        }

        let Some(event) = crate::parser::to_process_event(&raw) else {
            return;
        };

        if !self.first_token_event.swap(true, Ordering::SeqCst) {
            if let Some(start) = *self.run_start.lock().unwrap() {
                let event_name = match &event {
                    ProcessEvent::TextDelta { .. } => "TextDelta",
                    ProcessEvent::Thinking { .. } => "Thinking",
                    ProcessEvent::ToolUse { .. } => "ToolUse",
                    ProcessEvent::ToolResult { .. } => "ToolResult",
                    ProcessEvent::Done => "Done",
                    ProcessEvent::Error { .. } => "Error",
                    _ => "Other",
                };
                log::info!(
                    "[claude-plugin] instance={} first ProcessEvent after {:?}: type={}",
                    self.config.id,
                    start.elapsed(),
                    event_name
                );
            }
        }

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

        let active_session_id = self.active_session().clone();
        let conversation_id = active_session_id
            .and_then(|sid| self.session_map.conversation_for_session(&sid))
            .unwrap_or_default();

        let mapper = EventMapper::new(self.config.id.clone(), conversation_id);
        mapper.map(event, &self.event_sink);
    }
}

impl AgentInstance for ClaudeInstance {
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
            let start = std::time::Instant::now();
            *self.run_start.lock().unwrap() = Some(start);
            self.first_raw_event.store(false, Ordering::SeqCst);
            self.first_token_event.store(false, Ordering::SeqCst);
            log::info!(
                "[claude-plugin] instance={} send_message begin conversation={}",
                self.config.id,
                conversation_id
            );
            self.session_map.insert(&conversation_id, &conversation_id);
            self.ensure_started().await?;
            self.do_send(Self::build_user_message_payload(&message))?;
            log::info!(
                "[claude-plugin] instance={} user message sent after {:?}",
                self.config.id,
                start.elapsed()
            );
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
            self.do_send(Self::build_user_message_payload(&message))
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

fn extract_session_id(raw: &ClaudeRawEvent) -> Option<String> {
    match raw {
        ClaudeRawEvent::System { subtype, extra } if subtype == SUBTYPE_INIT => {
            extra.get(KEY_SESSION_ID).and_then(|v| v.as_str()).map(String::from)
        }
        ClaudeRawEvent::Result { session_id, .. } => Some(session_id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::event_sink::{DynEventSink, EventSink};
    use std::sync::{Arc, Mutex};

    const TEST_INSTANCE_ID: &str = "i-1";
    const TEST_CONVERSATION_ID: &str = "c-1";

    #[derive(Clone, Default)]
    struct MockSink {
        events: Arc<Mutex<Vec<(String, Value)>>>,
    }

    impl EventSink for MockSink {
        fn emit(&self, event_type: &str, payload: Value) {
            self.events
                .lock()
                .unwrap()
                .push((event_type.to_string(), payload));
        }
    }

    fn dummy_logger() -> Arc<SessionLogger> {
        Arc::new(SessionLogger::new(
            Arc::new(MockSink::default()) as DynEventSink,
            rusqlite::Connection::open_in_memory().unwrap(),
            None,
        ))
    }

    fn dummy_config() -> InstanceConfig {
        InstanceConfig {
            id: TEST_INSTANCE_ID.into(),
            name: "test".into(),
            workspace: "/tmp".into(),
            session_id: None,
            model: None,
            permission_mode: None,
        }
    }

    #[test]
    fn test_build_user_message_payload() {
        let payload = ClaudeInstance::build_user_message_payload("hello");
        assert_eq!(payload[KEY_TYPE], ROLE_USER);
        assert_eq!(payload[KEY_MESSAGE][KEY_ROLE], ROLE_USER);
        assert_eq!(payload[KEY_MESSAGE][KEY_CONTENT][0][KEY_TYPE], CONTENT_TYPE_TEXT);
        assert_eq!(payload[KEY_MESSAGE][KEY_CONTENT][0][KEY_TEXT], "hello");
    }

    #[test]
    fn test_session_map_insert() {
        let logger = dummy_logger();
        let sink: DynEventSink = Arc::new(MockSink::default());
        let instance = ClaudeInstance::new(dummy_config(), sink, logger);

        instance.session_map.insert(TEST_CONVERSATION_ID, "s-1");

        assert_eq!(
            instance.session_map.session_for_conversation(TEST_CONVERSATION_ID),
            Some("s-1".to_string())
        );
        assert_eq!(
            instance.session_map.conversation_for_session("s-1"),
            Some(TEST_CONVERSATION_ID.to_string())
        );
    }

    #[test]
    fn test_extract_session_id_from_result() {
        let raw = ClaudeRawEvent::Result {
            result: "ok".into(),
            session_id: "s-2".into(),
        };
        assert_eq!(extract_session_id(&raw), Some("s-2".to_string()));
    }
}
