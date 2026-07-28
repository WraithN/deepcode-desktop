use agent_core::error::InstanceError;
use agent_core::event_sink::DynEventSink;
use agent_core::instance::{AgentInstance, InstanceConfig, InstanceStatus, UNKNOWN_PID};
use agent_core::logger::{LogLevel, SessionLogger};
use agent_core::process::event::ProcessEvent;
use agent_core::process::mapper::{emit_status_changed, EventMapper};
use agent_core::process::transport::TransportHandle;
use agent_core::session_map::ConversationSessionMap;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;

use crate::mapper::{detect_interaction_from_parts, InteractionRequest};
use crate::transport::{
    connect_opencode_sse, port_allocator, start_opencode_process, OpenCodeClient,
};

const LOG_SOURCE: &str = "opencode-plugin";
const LOCALHOST: &str = "http://127.0.0.1";
const STARTUP_WAIT_COUNT: u32 = 20;
const STARTUP_WAIT_MS: u64 = 500;
const SSE_CHANNEL_CAPACITY: usize = 1000;

/// SSE 事件活跃度看门狗阈值:超过此时长未收到任何 agent 事件即判定卡死。
/// agent 正常思考/工具执行期间 opencode 会流式推送 thinking/tool_use 等事件,
/// 阈值需大于正常事件间隔。卡死(如 LLM API 挂起、死锁)时无事件,触发重建重试。
const WATCHDOG_STALL_THRESHOLD_SECS: u64 = 120;
/// 看门狗周期性检查 SSE 活跃度的时间间隔。
const WATCHDOG_CHECK_INTERVAL_SECS: u64 = 10;
/// 消息发送卡死/失败时的最大重试次数,每次重试会重建 agent 进程并新建 session。
/// 总尝试次数 = 1 + MAX_SEND_RETRIES。
const MAX_SEND_RETRIES: u32 = 2;

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
    /// 最近一次收到 agent SSE 事件的时间,供看门狗判定卡死。
    last_event_at: Arc<Mutex<Option<Instant>>>,
    /// 用于取消当前在途 HTTP 请求的发送端。当 SSE relay loop 检测到 question
    /// 等交互式工具时，立即取消 `send_message_http` 的阻塞等待，从而结束本 run，
    /// 让前端可以在新 run 中发送响应，避免 opencode 不能并发处理两条消息。
    cancel_send: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// 从持久化存储恢复的 session ID（由 gatewayd 传入），首次发消息时优先使用。
    /// opencode 将 session 持久化到磁盘，新 `opencode serve` 进程可通过旧 session_id
    /// 直接发消息（POST /session/{id}/message），恢复上下文。
    initial_session_id: Mutex<Option<String>>,
    /// 当前活跃的 opencode session ID，供 reaper 持久化以支持 resume。
    last_session_id: Arc<Mutex<Option<String>>>,
}

impl OpencodeInstance {
    pub fn new(
        config: InstanceConfig,
        event_sink: DynEventSink,
        logger: Arc<SessionLogger>,
    ) -> Self {
        let initial_session_id = config.session_id.clone();
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
            last_event_at: Arc::new(Mutex::new(None)),
            cancel_send: Arc::new(Mutex::new(None)),
            initial_session_id: Mutex::new(initial_session_id),
            last_session_id: Arc::new(Mutex::new(None)),
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
        let handle =
            connect_opencode_sse(&base_url, client.client().clone(), &self.config.id, tx).await?;
        *self.transport_handle.lock().await = Some(handle);

        let event_sink = self.event_sink.clone();
        let instance_id = self.config.id.clone();
        let session_map = self.session_map.clone();
        let last_event_at = self.last_event_at.clone();
        let cancel_send = self.cancel_send.clone();
        tokio::spawn(async move {
            while let Some(payload) = rx.recv().await {
                let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let mut events = crate::mapper::map_opencode_sse(&payload);
                if !events.is_empty() {
                    // 收到 agent 事件,刷新看门狗活跃时间戳。
                    *last_event_at.lock().unwrap() = Some(Instant::now());
                    let session_id =
                        crate::mapper::extract_session_id(&payload).unwrap_or_default();
                    let conversation_id = session_map
                        .conversation_for_session(&session_id)
                        .unwrap_or_default();

                    // 检测 question 工具调用：将其转换为交互事件，并结束当前 run，
                    // 让前端可以发送 respond。opencode 不能并发处理两条消息，
                    // 因此必须取消在途 HTTP 请求。
                    let mut question_detected = false;
                    let mut question_events = Vec::new();
                    for event in &events {
                        if let ProcessEvent::ToolUse { name, input } = event {
                            if name == "question" {
                                if let Some(interaction) =
                                    crate::mapper::detect_question_tool_input(input)
                                {
                                    question_detected = true;
                                    question_events
                                        .push(crate::mapper::map_interaction(&interaction));
                                }
                            }
                        }
                    }

                    if question_detected {
                        // 移除 question 的 ToolUse 事件，避免前端同时看到工具调用卡片和交互弹窗。
                        events.retain(|e| {
                            !matches!(e, ProcessEvent::ToolUse { name, .. } if name == "question")
                        });
                        events.extend(question_events);
                        events.push(ProcessEvent::Done);
                        if let Some(tx) = cancel_send.lock().unwrap().take() {
                            let _ = tx.send(());
                            log::info!(
                                "[relay-loop] instance={instance_id} cancelled HTTP request for question interaction"
                            );
                        }
                    }

                    log::info!(
                        "[relay-loop] ev={event_type} oc_sid={session_id} gw_sid={conversation_id} mapped={}",
                        events.len()
                    );
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
        let base = self
            .base_url()
            .ok_or_else(|| InstanceError::NotRunning(ERR_SERVE_NOT_STARTED.into()))?;
        OpenCodeClient::new(base).create_session().await
    }

    async fn send_message_http(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let base = self
            .base_url()
            .ok_or_else(|| InstanceError::NotRunning(ERR_SERVE_NOT_STARTED.into()))?;
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

    /// 发送一次 HTTP 消息并与 SSE 活跃度看门狗竞争。
    ///
    /// HTTP POST `/session/{id}/message` 会阻塞至 agent run 结束。正常情况下
    /// agent 期间持续推送 SSE 事件;若 agent 卡死(LLM API 挂起、死锁等),
    /// HTTP 既不返回也无事件。看门狗在阈值内无事件时中断本次请求。
    ///
    /// 另外，当 relay loop 检测到 question 等交互式工具时，会通过 `cancel_send`
    /// 取消本次 HTTP 请求，让 run 立即结束，避免 opencode 不能并发处理两条消息。
    async fn http_with_watchdog(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        // 从本次发送开始计时活跃度
        *self.last_event_at.lock().unwrap() = Some(Instant::now());

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
        *self.cancel_send.lock().unwrap() = Some(cancel_tx);

        let result = tokio::select! {
            r = self.send_message_http(session_id, message) => r,
            _ = cancel_rx => {
                log::info!(
                    "[opencode-plugin] instance={} session={} HTTP request cancelled by interaction",
                    self.config.id,
                    session_id
                );
                Err(InstanceError::InteractionCancelled(
                    "interaction awaiting user response".into(),
                ))
            }
            _ = self.watchdog_until_stalled() => Err(InstanceError::ProcessError(format!(
                "agent stalled: no SSE events for {WATCHDOG_STALL_THRESHOLD_SECS}s"
            ))),
        };

        // 请求结束（无论成败）后清空取消句柄，防止 relay loop 取消已不存在的请求。
        *self.cancel_send.lock().unwrap() = None;
        result
    }

    /// 看门狗 future:周期性检查 SSE 事件活跃度,直到判定卡死时返回。
    /// 在 `select!` 中与 HTTP 请求竞争;HTTP 先完成则本 future 被 drop。
    async fn watchdog_until_stalled(&self) {
        let threshold = Duration::from_secs(WATCHDOG_STALL_THRESHOLD_SECS);
        let interval = Duration::from_secs(WATCHDOG_CHECK_INTERVAL_SECS);
        loop {
            tokio::time::sleep(interval).await;
            let stalled = self
                .last_event_at
                .lock()
                .unwrap()
                .map(|t| t.elapsed() > threshold)
                .unwrap_or(false);
            if stalled {
                return;
            }
        }
    }

    /// 发送消息并带看门狗重试:卡死或失败时重建 agent 进程并新建 session 后重试。
    /// 重试上限 `MAX_SEND_RETRIES`,每次重建都会刷新 opencode serve 进程。
    async fn send_with_watchdog_retry(
        &self,
        session_id: &mut String,
        message: &str,
        conversation_id: &str,
    ) -> Result<serde_json::Value, InstanceError> {
        let mut attempt = 0u32;
        loop {
            match self.http_with_watchdog(session_id, message).await {
                Ok(value) => return Ok(value),
                // question 等交互式工具取消的请求不应重试：relay loop 已 emit 交互
                // 事件与 agent.done，重试会重建进程并浪费 LLM 调用。
                Err(InstanceError::InteractionCancelled(_)) => {
                    log::info!(
                        "[opencode-plugin] instance={} interaction cancelled, not retrying",
                        self.config.id
                    );
                    return Err(InstanceError::InteractionCancelled(
                        "interaction awaiting user response".into(),
                    ));
                }
                Err(e) if attempt < MAX_SEND_RETRIES => {
                    attempt += 1;
                    log::warn!(
                        "send_message attempt {attempt}/{} failed ({}), rebuilding agent and retrying",
                        MAX_SEND_RETRIES + 1,
                        e
                    );
                    self.reset_and_restart().await?;
                    *session_id = self.create_opencode_session().await?;
                    self.store_session(conversation_id, session_id);
                    *self.last_session_id.lock().unwrap() = Some(session_id.clone());
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// 响应交互(ask_permission/ask_user)并带看门狗。
    ///
    /// 与 `send_with_watchdog_retry` 不同,respond 只有 opencode session_id
    /// 而无 conversation 映射,重建后旧 session 必然失效,因此不做自动重试;
    /// 卡死/失败时重建 agent 进程,使后续交互可在新进程上重试。
    async fn respond_with_watchdog(
        &self,
        session_id: &str,
        message: &str,
    ) -> Result<(), InstanceError> {
        match self.http_with_watchdog(session_id, message).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::warn!("respond failed ({}), rebuilding agent", e);
                let _ = self.reset_and_restart().await;
                Err(e)
            }
        }
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

    fn active_session_id(&self) -> Option<String> {
        self.last_session_id.lock().unwrap().clone()
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
                    // 优先使用持久化恢复的 session ID（opencode 将 session 持久化到磁盘，
                    // 新进程可通过旧 session_id 直接发消息恢复上下文）。
                    // 若恢复失败（session 已删除），send_with_watchdog_retry 会自动
                    // 重建进程并新建 session。
                    let initial = self.initial_session_id.lock().unwrap().take();
                    let sid = if let Some(initial) = initial {
                        log::info!(
                            "[opencode-plugin] instance={} resuming persisted session={}",
                            self.config.id,
                            initial
                        );
                        initial
                    } else {
                        self.create_opencode_session().await?
                    };
                    self.store_session(&conversation_id, &sid);
                    sid
                }
            };
            *self.last_session_id.lock().unwrap() = Some(session_id.clone());

            let result = self
                .send_with_watchdog_retry(&mut session_id, &message, &conversation_id)
                .await;
            let result = match result {
                Ok(value) => value,
                Err(InstanceError::InteractionCancelled(_)) => {
                    // question 交互已被 relay loop 转换为 agent.question + agent.done，
                    // run 已通过 agent.done 正常结束，这里直接返回成功，避免上层再广播错误。
                    log::info!(
                        "[opencode-plugin] instance={} conversation={} send_message cancelled by interaction; treating as success",
                        self.config.id,
                        conversation_id
                    );
                    return Ok(());
                }
                Err(e) => return Err(e),
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
            self.respond_with_watchdog(&session_id, &message).await
        })
    }

    fn respond_by_conversation(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), InstanceError>> + Send + '_>> {
        let conversation_id = conversation_id.to_string();
        let message = message.to_string();
        Box::pin(async move {
            // 避免与 send_message 中持有的 startup_lock 产生死锁：当 agent 正在执
            // 行交互式工具（如 question）时，send_message 仍阻塞在 ensure_started 后的
            // HTTP 请求上，respond 不应再尝试获取 startup_lock。只要 base_url 已设置
            // 说明 opencode serve 已启动，可直接发送响应。
            if self.base_url().is_none() {
                return Err(InstanceError::NotRunning(
                    "opencode serve not started".into(),
                ));
            }
            let session_id = self
                .find_session_for_conversation(&conversation_id)
                .ok_or_else(|| {
                    InstanceError::NotFound(format!(
                        "no opencode session for conversation {}",
                        conversation_id
                    ))
                })?;
            self.respond_with_watchdog(&session_id, &message).await
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
